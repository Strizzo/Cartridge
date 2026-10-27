//! Independently signed browser components. Never modifies the OS, boot or games.
//!
//! Storage is fixed at `<home>/.cartridges/components/browser-engine`. Stop the
//! browser before changing its component. Verified releases are read-only, not a
//! security boundary against their owning user. No API accepts unsigned engines
//! or a replacement production trust key. Local artifacts use the same verifier
//! as HTTPS artifacts. The signature covers the exact UTF-8 payload, as with OTA.
use ed25519_dalek::{Signature, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const KEY_ID: &str = "cartridge-os-v1";
pub const TARGET: &str = "r36s-plus-aarch64";
pub const MANIFEST_NAME: &str = "browser-component.json";
const RECEIPT: &str = ".browser-release.json";
const PUBLIC_KEY: &str = include_str!("../../../scripts/system-update-public-key.hex");
const MAX_MANIFEST: u64 = 32 * 1024 * 1024;
const MAX_ARCHIVE: u64 = 4 * 1024 * 1024 * 1024;
const MAX_STORED: u64 = 4 * 1024 * 1024 * 1024;
const MAX_LOGICAL: u64 = 8 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 65536;
const RESERVE: u64 = 32 * 1024 * 1024;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentArchive {
    pub url: String,
    pub size: u64,
    pub sha256: String,
    /// Sum of regular bodies in the tar; hardlinks have no body.
    pub stored_bytes: u64,
    /// All regular paths including hardlink aliases.
    pub logical_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum ComponentEntry {
    Directory {
        path: String,
    },
    File {
        path: String,
        size: u64,
        sha256: String,
        mode: u32,
    },
    Symlink {
        path: String,
        target: String,
    },
    Hardlink {
        path: String,
        target: String,
    },
}
impl ComponentEntry {
    pub fn path(&self) -> &str {
        match self {
            Self::Directory { path }
            | Self::File { path, .. }
            | Self::Symlink { path, .. }
            | Self::Hardlink { path, .. } => path,
        }
    }
    fn target(&self) -> Option<&str> {
        match self {
            Self::Symlink { target, .. } | Self::Hardlink { target, .. } => Some(target),
            _ => None,
        }
    }
    fn mode(&self) -> u32 {
        match self {
            Self::Directory { .. } => 0o555,
            Self::File { mode, .. } => *mode,
            Self::Symlink { .. } => 0o777,
            Self::Hardlink { .. } => 0,
        }
    }
    fn kind(&self) -> u8 {
        match self {
            Self::Directory { .. } => b'5',
            Self::File { .. } => b'0',
            Self::Symlink { .. } => b'2',
            Self::Hardlink { .. } => b'1',
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentFileReference {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// Signed references only. Install does not fetch these or assert availability.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentSourceArtifact {
    pub role: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentProvenance {
    pub package_list: ComponentFileReference,
    pub build_inventory: ComponentFileReference,
    pub sources: Vec<ComponentSourceArtifact>,
}

/// Metadata is retained, not a claim that corresponding sources are distributed.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentRelease {
    pub schema: u32,
    pub component: String,
    pub target: String,
    pub version: String,
    pub revision: String,
    pub min_runtime: String,
    pub chromium_version: String,
    pub source_distribution: String,
    pub provenance: ComponentProvenance,
    pub archive: ComponentArchive,
    pub entries: Vec<ComponentEntry>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    key_id: String,
    payload: String,
    signature: String,
}

/// Constructed only after authenticating and validating a complete envelope.
#[derive(Clone, Debug)]
pub struct VerifiedComponent {
    release: ComponentRelease,
    envelope: String,
    id: String,
}
impl VerifiedComponent {
    pub fn release(&self) -> &ComponentRelease {
        &self.release
    }
    /// Full signed-payload hash, never an untrusted filename or truncated version.
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn signed_manifest(&self) -> &str {
        &self.envelope
    }
    pub fn verify_directory(&self, directory: &Path) -> Result<()> {
        verify_tree(directory, self, false, false)
    }
}

pub fn verify_manifest(body: &str) -> Result<VerifiedComponent> {
    verify_with_key(body, &hex::<32>(PUBLIC_KEY.trim())?)
}
pub fn inspect_local(manifest: &Path) -> Result<VerifiedComponent> {
    verify_manifest(&read_text(manifest, MAX_MANIFEST)?)
}
/// Read and authenticate the entire HTTPS manifest; redirects cannot downgrade.
pub fn inspect_https(url: &str) -> Result<VerifiedComponent> {
    verify_manifest(&fetch_manifest(url)?)
}
fn verify_with_key(body: &str, key: &[u8; 32]) -> Result<VerifiedComponent> {
    if body.len() as u64 > MAX_MANIFEST {
        return Err("Component manifest exceeds 32 MiB".into());
    }
    let envelope: Envelope = serde_json::from_str(body).map_err(error)?;
    if envelope.key_id != KEY_ID {
        return Err("Unknown component signing key".into());
    }
    VerifyingKey::from_bytes(key)
        .map_err(error)?
        .verify_strict(
            envelope.payload.as_bytes(),
            &Signature::from_bytes(&hex::<64>(&envelope.signature)?),
        )
        .map_err(|_| "Component signature did not verify")?;
    let release: ComponentRelease = serde_json::from_str(&envelope.payload).map_err(error)?;
    validate_release(&release)?;
    Ok(VerifiedComponent {
        id: digest(envelope.payload.as_bytes()),
        release,
        envelope: body.into(),
    })
}
fn validate_release(r: &ComponentRelease) -> Result<()> {
    if r.schema != 1
        || r.component != "browser-engine"
        || r.target != TARGET
        || r.source_distribution != "metadata-only; corresponding-source distribution not asserted"
        || r.chromium_version.is_empty()
        || r.chromium_version.len() > 128
    {
        return Err("Unsupported browser component identity or source metadata".into());
    }
    for v in [&r.version, &r.min_runtime] {
        let parsed = Version::parse(v).map_err(error)?;
        if v.len() > 64 || !parsed.pre.is_empty() || !parsed.build.is_empty() {
            return Err("Component versions must be stable semantic versions".into());
        }
    }
    if r.revision.len() != 40 || !lower_hex(&r.revision) {
        return Err("Invalid build revision".into());
    }
    https_url(&r.archive.url)?;
    hex::<32>(&r.archive.sha256)?;
    if r.archive.size == 0
        || r.archive.size > MAX_ARCHIVE
        || r.archive.stored_bytes > MAX_STORED
        || r.archive.logical_bytes > MAX_LOGICAL
        || r.entries.is_empty()
        || r.entries.len() > MAX_ENTRIES
    {
        return Err("Component exceeds independent archive limits".into());
    }
    let mut entries: BTreeMap<&str, &ComponentEntry> = BTreeMap::new();
    let (mut stored, mut logical) = (0u64, 0u64);
    let mut last = "";
    for entry in &r.entries {
        let path = entry.path();
        safe_path(path)?;
        if path <= last || path == RECEIPT {
            return Err("Component entries must be unique and sorted".into());
        }
        last = path;
        if let Some((parent, _)) = path.rsplit_once('/') {
            if !matches!(entries.get(parent), Some(ComponentEntry::Directory { .. })) {
                return Err(format!("Non-directory or missing parent: {path}"));
            }
        }
        match entry {
            ComponentEntry::File {
                size, sha256, mode, ..
            } => {
                hex::<32>(sha256)?;
                if ![0o444, 0o555].contains(mode) {
                    return Err("Files must be read-only and unprivileged".into());
                }
                stored = stored.checked_add(*size).ok_or("Size overflow")?;
                logical = logical.checked_add(*size).ok_or("Size overflow")?;
            }
            ComponentEntry::Hardlink { target, .. } => {
                safe_path(target)?;
                if target.len() > 100 {
                    return Err("Hardlink target exceeds USTAR limit".into());
                }
                if let Some(ComponentEntry::File { size, .. }) = entries.get(target.as_str()) {
                    logical = logical.checked_add(*size).ok_or("Size overflow")?;
                } else {
                    return Err("Hardlink must reference an earlier regular file".into());
                }
            }
            ComponentEntry::Symlink { target, .. } => {
                if target.is_empty()
                    || target.len() > 100
                    || !printable(target)
                    || target.starts_with('/')
                {
                    return Err("Unsafe symbolic link target".into());
                }
            }
            _ => (),
        }
        entries.insert(path, entry);
    }
    if stored != r.archive.stored_bytes
        || logical != r.archive.logical_bytes
        || stored > MAX_STORED
        || logical > MAX_LOGICAL
    {
        return Err("Component size accounting mismatch".into());
    }
    for entry in &r.entries {
        if matches!(entry, ComponentEntry::Symlink { .. }) {
            resolve_link(entry.path(), &entries)?;
        }
    }
    for path in [
        "chromium",
        "bubblewrap",
        "component.json",
        "packages.tsv",
        "notices/bubblewrap.txt",
        "notices/static-libc.txt",
        "notices/static-libcap.txt",
        "notices/sources.txt",
        "rootfs/usr/lib/chromium/chromium",
    ] {
        let entry = entries
            .get(path)
            .ok_or_else(|| format!("Required browser/notice file missing: {path}"))?;
        let file = match entry {
            ComponentEntry::Hardlink { target, .. } => entries[target.as_str()],
            _ => entry,
        };
        if !matches!(file, ComponentEntry::File { size, mode, .. } if *size > 0 &&
            (!matches!(path, "chromium" | "bubblewrap" | "rootfs/usr/lib/chromium/chromium") || *mode == 0o555))
        {
            return Err(format!(
                "Required component file is not regular/executable: {path}"
            ));
        }
    }
    if !r
        .entries
        .iter()
        .any(|e| e.path().starts_with("rootfs/usr/share/doc/") && e.path().ends_with("/copyright"))
    {
        return Err("Debian copyright notices are required".into());
    }
    for (reference, name) in [
        (&r.provenance.package_list, "packages.tsv"),
        (&r.provenance.build_inventory, "component.json"),
    ] {
        let entry = entries.get(name).ok_or("Missing provenance file")?;
        let file = match entry {
            ComponentEntry::Hardlink { target, .. } => entries[target.as_str()],
            _ => entry,
        };
        if reference.path != name
            || !matches!(file, ComponentEntry::File { size, sha256, .. }
            if *size == reference.size && *sha256 == reference.sha256)
        {
            return Err("Provenance reference differs from signed file".into());
        }
    }
    if r.provenance.sources.len() > 4096 {
        return Err("Too many source artifact references".into());
    }
    let mut urls = std::collections::HashSet::new();
    for source in &r.provenance.sources {
        https_url(&source.url)?;
        hex::<32>(&source.sha256)?;
        if !["index", "archive"].contains(&source.role.as_str())
            || source.size == 0
            || source.size > 1024 * 1024 * 1024 * 1024
            || !urls.insert(&source.url)
        {
            return Err("Invalid/duplicate source artifact reference".into());
        }
    }
    Ok(())
}
fn resolve_link(path: &str, entries: &BTreeMap<&str, &ComponentEntry>) -> Result<()> {
    let mut todo: VecDeque<String> = path.split('/').map(String::from).collect();
    let mut resolved = Vec::<String>::new();
    let mut hops = 0;
    while let Some(part) = todo.pop_front() {
        match part.as_str() {
            "" | "." => continue,
            ".." => {
                if resolved.pop().is_none() {
                    return Err("Symbolic link escapes component".into());
                }
            }
            _ => {
                resolved.push(part);
                if let Some(ComponentEntry::Symlink { target, .. }) =
                    entries.get(resolved.join("/").as_str())
                {
                    hops += 1;
                    if hops > 40 {
                        return Err("Symbolic link cycle or excessive chain".into());
                    }
                    resolved.pop();
                    for item in target.split('/').rev() {
                        todo.push_front(item.into());
                    }
                }
            }
        }
    }
    Ok(())
}
fn printable(s: &str) -> bool {
    s.bytes().all(|b| (32..127).contains(&b) && b != b'\\')
}
fn safe_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 255
        || !printable(path)
        || path.split('/').count() > 32
        || path
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == ".." || s.len() > 100)
    {
        return Err(format!("Unsafe component path: {path:?}"));
    }
    Ok(())
}
fn https_url(url: &str) -> Result<()> {
    // Parsing is also performed by ureq. Exclude credentials, fragments and
    // ambiguous authorities; any signed HTTPS origin may host a release.
    let rest = url
        .strip_prefix("https://")
        .ok_or("Component transport requires HTTPS")?;
    let host = rest.split('/').next().unwrap_or("");
    if host.is_empty()
        || host.starts_with(':')
        || url.len() > 4096
        || !url.is_ascii()
        || url.bytes().any(|c| c <= 32 || c == 127)
        || url.contains(['@', '#', '\\'])
    {
        return Err("Invalid component HTTPS URL".into());
    }
    Ok(())
}
fn lower_hex(s: &str) -> bool {
    s.bytes()
        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn hex<const N: usize>(s: &str) -> Result<[u8; N]> {
    if s.len() != N * 2 || !lower_hex(s) {
        return Err("Invalid lowercase hex digest/signature".into());
    }
    let mut out = [0; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(error)?;
    }
    Ok(out)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(5)
        .timeout_global(Some(Duration::from_secs(3600)))
        .http_status_as_error(false)
        .build()
        .into()
}
fn response(url: &str) -> Result<ureq::http::Response<ureq::Body>> {
    https_url(url)?;
    let result = agent()
        .get(url)
        .header("Accept-Encoding", "identity")
        .call()
        .map_err(error)?;
    if result.status().as_u16() != 200 {
        return Err(format!("Component server returned {}", result.status()));
    }
    if result
        .headers()
        .get("content-encoding")
        .is_some_and(|v| v.to_str().ok() != Some("identity"))
    {
        return Err("Unexpected HTTP content encoding".into());
    }
    Ok(result)
}
fn fetch_manifest(url: &str) -> Result<String> {
    let mut bytes = Vec::new();
    response(url)?
        .into_body()
        .into_reader()
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut bytes)
        .map_err(error)?;
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err("Component manifest is too large".into());
    }
    String::from_utf8(bytes).map_err(error)
}
fn open_regular(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(error)?;
    if !file.metadata().map_err(error)?.is_file() {
        return Err("Expected a regular component file".into());
    }
    Ok(file)
}
fn read_text(path: &Path, limit: u64) -> Result<String> {
    let mut text = String::new();
    open_regular(path)?
        .take(limit + 1)
        .read_to_string(&mut text)
        .map_err(error)?;
    if text.len() as u64 > limit {
        return Err("Component metadata is too large".into());
    }
    Ok(text)
}
fn hash_file(path: &Path, size: u64, sha: &str) -> Result<()> {
    let mut file = open_regular(path)?;
    if file.metadata().map_err(error)?.len() != size {
        return Err(format!("File size mismatch: {}", path.display()));
    }
    let mut hash = Sha256::new();
    let copied =
        std::io::copy(&mut Read::by_ref(&mut file).take(size + 1), &mut hash).map_err(error)?;
    if copied != size || format!("{:x}", hash.finalize()) != sha {
        return Err(format!("File hash mismatch: {}", path.display()));
    }
    Ok(())
}

#[derive(Clone, Debug, Default)]
pub struct ComponentStatus {
    pub current: Option<String>,
    pub previous: Option<String>,
}

/// Persistent store. All mutations are serialized, private and Linux-only.
/// Callers must stop the browser before install/adopt/rollback. Retired releases
/// are retained for explicit recovery. Current/previous supply verified file reuse.
pub struct BrowserComponents {
    root: PathBuf,
    key: [u8; 32],
}
impl BrowserComponents {
    pub fn from_environment() -> Result<Self> {
        let home = std::env::var_os("CARTRIDGE_HOME")
            .or_else(|| std::env::var_os("HOME"))
            .ok_or("Missing user home")?;
        Self::at_home(Path::new(&home))
    }
    pub fn at_home(home: &Path) -> Result<Self> {
        Self::open_home(home, hex::<32>(PUBLIC_KEY.trim())?)
    }
    fn open_home(home: &Path, key: [u8; 32]) -> Result<Self> {
        if !cfg!(target_os = "linux") && !cfg!(test) {
            return Err("Persistent browser components require a private Linux filesystem".into());
        }
        let home = home.canonicalize().map_err(error)?;
        if home.parent().is_none()
            || [
                "/boot", "/usr", "/lib", "/lib64", "/etc", "/bin", "/sbin", "/dev", "/proc",
                "/sys", "/roms", "/games", "/mnt", "/media",
            ]
            .iter()
            .any(|p| home.starts_with(p))
        {
            return Err("Component home must be user storage, outside OS/boot/games".into());
        }
        check_directory(&home, false)?;
        linux_filesystem(&home)?;
        let mut root = home;
        for part in [".cartridges", "components", "browser-engine"] {
            root.push(part);
            ensure_directory(&root)?;
            linux_filesystem(&root)?;
        }
        // The payload is never staged on FAT/exFAT, network mounts or tmpfs.
        linux_filesystem(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(error)?;
        for part in ["releases", "generations"] {
            ensure_directory(&root.join(part))?;
        }
        Ok(Self { root, key })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn inspect(&self, body: &str) -> Result<VerifiedComponent> {
        verify_with_key(body, &self.key)
    }
    pub fn status(&self) -> Result<ComponentStatus> {
        let _guard = self.lock()?;
        self.read_status()
    }
    pub fn verify_current(&self) -> Result<VerifiedComponent> {
        let _guard = self.lock()?;
        let id = self
            .read_status()?
            .current
            .ok_or("No browser component installed")?;
        self.verify_installed(&id)
    }
    fn verify_installed(&self, id: &str) -> Result<VerifiedComponent> {
        hex::<32>(id)?;
        let path = self.root.join("releases").join(id);
        check_directory(&path, true)?;
        let release = self.inspect(&read_text(&path.join(RECEIPT), MAX_MANIFEST)?)?;
        if release.id != id {
            return Err("Installed component identity mismatch".into());
        }
        verify_tree(&path, &release, true, false)?;
        Ok(release)
    }
    /// Verifies all bytes, hashes and metadata without executing the component.
    /// No compressed archive copy is written to the device.
    pub fn install_local(
        &self,
        manifest: &Path,
        archive: &Path,
        progress: impl FnMut(String),
    ) -> Result<VerifiedComponent> {
        let release = self.inspect(&read_text(manifest, MAX_MANIFEST)?)?;
        self.install_reader(&release, open_regular(archive)?, progress)
    }
    pub fn install_https(
        &self,
        manifest_url: &str,
        progress: impl FnMut(String),
    ) -> Result<VerifiedComponent> {
        let release = self.inspect(&fetch_manifest(manifest_url)?)?;
        let result = response(&release.release.archive.url)?;
        if result.headers().get("content-length").is_some_and(|v| {
            v.to_str().ok().and_then(|s| s.parse::<u64>().ok())
                != Some(release.release.archive.size)
        }) {
            return Err("HTTP archive size differs from signed manifest".into());
        }
        self.install_reader(&release, result.into_body().into_reader(), progress)
    }
    fn install_reader(
        &self,
        release: &VerifiedComponent,
        reader: impl Read,
        mut progress: impl FnMut(String),
    ) -> Result<VerifiedComponent> {
        let _guard = self.lock()?;
        self.compatible(release)?;
        self.read_status()?;
        let destination = self.root.join("releases").join(&release.id);
        if destination.try_exists().map_err(error)? {
            self.verify_installed(&release.id)?;
            self.activate(&release.id)?;
            return Ok(release.clone());
        }
        let staging = self.root.join(".incoming");
        remove_tree(&staging)?;
        ensure_directory(&staging)?;
        let result = (|| {
            progress("Checking verified files available for reuse".into());
            let reuse = self.reuse_files(release)?;
            let required = release
                .release
                .entries
                .iter()
                .filter_map(|e| match e {
                    ComponentEntry::File { path, size, .. } if !reuse.contains_key(path) => {
                        Some(size.saturating_add(4095) / 4096 * 4096)
                    }
                    _ => None,
                })
                .sum::<u64>()
                .saturating_add(RESERVE)
                .saturating_add(release.envelope.len() as u64)
                .saturating_add(release.release.entries.len() as u64 * 4096);
            space_check(&self.root, required)?;
            progress(format!(
                "Streaming and verifying {} entries ({} reusable files)",
                release.release.entries.len(),
                reuse.len()
            ));
            extract(reader, &staging, release, &reuse)?;
            write_new(&staging.join(RECEIPT), release.envelope.as_bytes())?;
            seal_tree(&staging)?;
            verify_tree(&staging, release, true, false)?;
            fs::rename(&staging, &destination).map_err(error)?;
            sync_dir(&self.root.join("releases"))?;
            interruption("release-renamed")?;
            self.activate(&release.id)?;
            progress(format!(
                "Activated verified browser component {}",
                release.release.version
            ));
            Ok(release.clone())
        })();
        let _ = remove_tree(&staging);
        result
    }
    /// Offline first-install migration. Verify first, then rename; NEVER copy or
    /// fall back across filesystems. Retry after interruption with the same
    /// manifest/source: an already moved, verified release is recovered in place.
    pub fn adopt(&self, manifest: &Path, source: &Path) -> Result<VerifiedComponent> {
        let release = self.inspect(&read_text(manifest, MAX_MANIFEST)?)?;
        let _guard = self.lock()?;
        self.compatible(&release)?;
        self.read_status()?;
        let destination = self.root.join("releases").join(&release.id);
        if destination.try_exists().map_err(error)? {
            self.verify_installed(&release.id)?;
            self.activate(&release.id)?;
            return Ok(release);
        }
        // A supplied root symlink is not an adoption authority. Source must be
        // privately owned and quiescent; it is sealed before the atomic move.
        let source_meta = fs::symlink_metadata(source).map_err(error)?;
        if !source_meta.is_dir() || source_meta.file_type().is_symlink() {
            return Err("Adoption source must be a real directory".into());
        }
        let source = source.canonicalize().map_err(error)?;
        if source.starts_with(&self.root) || self.root.starts_with(&source) {
            return Err("Cannot adopt store directories".into());
        }
        check_directory(&source, false)?;
        if source_meta.dev() != fs::metadata(&self.root).map_err(error)?.dev() {
            return Err(
                "Adoption requires the same Linux filesystem; copying is never attempted".into(),
            );
        }
        fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).map_err(error)?;
        verify_tree(&source, &release, false, true)?;
        // Reserve is only for metadata: the 0.94 GiB tree is moved in place.
        space_check(
            &self.root,
            RESERVE.min(4 * 1024 * 1024) + release.envelope.len() as u64,
        )?;
        if !source.join(RECEIPT).try_exists().map_err(error)? {
            write_new(&source.join(RECEIPT), release.envelope.as_bytes())?;
        }
        seal_tree(&source)?;
        verify_tree(&source, &release, true, true)?;
        let parent = source.parent().ok_or("Missing source parent")?;
        fs::rename(&source, &destination).map_err(|e| {
            format!("Adoption rename failed; source is retained, no copy attempted: {e}")
        })?;
        sync_dir(parent)?;
        sync_dir(&self.root.join("releases"))?;
        interruption("adopt-renamed")?;
        self.activate(&release.id)?;
        Ok(release)
    }
    pub fn rollback(&self) -> Result<VerifiedComponent> {
        let _guard = self.lock()?;
        let state = self.read_status()?;
        let previous = state
            .previous
            .ok_or("No previous browser component to roll back to")?;
        let release = self.verify_installed(&previous)?;
        // Both candidates must still be verified; a tampered current does not
        // become a rollback candidate. Recovery can still select the good one.
        let current = state.current.filter(|id| self.verify_installed(id).is_ok());
        self.commit_state(&previous, current.as_deref())?;
        Ok(release)
    }
    fn compatible(&self, r: &VerifiedComponent) -> Result<()> {
        if Version::parse(&r.release.min_runtime).map_err(error)?
            > Version::parse(env!("CARGO_PKG_VERSION")).map_err(error)?
        {
            return Err("Browser component requires a newer Cartridge runtime".into());
        }
        if let Some(id) = self.read_status()?.current {
            if id != r.id {
                let path = self.root.join("releases").join(&id).join(RECEIPT);
                let old = self.inspect(&read_text(&path, MAX_MANIFEST)?)?;
                if old.id != id {
                    return Err("Active component receipt identity mismatch".into());
                }
                if Version::parse(&r.release.version).map_err(error)?
                    <= Version::parse(&old.release.version).map_err(error)?
                {
                    return Err("Install/adopt requires a newer component version; use rollback for the previous release".into());
                }
            }
        }
        Ok(())
    }
    fn lock(&self) -> Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(self.root.join(".lock"))
            .map_err(error)?;
        let meta = file.metadata().map_err(error)?;
        if !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } || meta.nlink() != 1 {
            return Err("Unsafe component lock".into());
        }
        file.try_lock()
            .map_err(|_| "Another browser component operation is in progress")?;
        Ok(file)
    }
    fn read_status(&self) -> Result<ComponentStatus> {
        let state = self.root.join("state");
        let target = match fs::read_link(&state) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ComponentStatus::default());
            }
            Err(e) => return Err(error(e)),
        };
        let id = target
            .to_str()
            .and_then(|s| s.strip_prefix("generations/"))
            .ok_or("Invalid component state pointer")?;
        hex::<32>(id)?;
        let dir = self.root.join(&target);
        check_directory(&dir, true)?;
        let read = |name: &str| -> Result<Option<String>> {
            match fs::read_link(dir.join(name)) {
                Ok(t) => {
                    let id = t
                        .to_str()
                        .and_then(|s| s.strip_prefix("../../releases/"))
                        .ok_or("Invalid release pointer")?;
                    hex::<32>(id)?;
                    Ok(Some(id.into()))
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(error(e)),
            }
        };
        let result = ComponentStatus {
            current: read("current")?,
            previous: read("previous")?,
        };
        if result.current.is_none()
            || state_id(
                result.current.as_deref().unwrap(),
                result.previous.as_deref(),
            ) != id
        {
            return Err("Incomplete component activation state".into());
        }
        Ok(result)
    }
    fn activate(&self, id: &str) -> Result<()> {
        self.verify_installed(id)?;
        let status = self.read_status()?;
        if status.current.as_deref() == Some(id) {
            return self.ensure_discovery();
        }
        // Only verified current components are retained as rollback candidates.
        let previous = status
            .current
            .filter(|old| self.verify_installed(old).is_ok());
        self.commit_state(id, previous.as_deref())
    }
    fn ensure_discovery(&self) -> Result<()> {
        for (name, target) in [
            ("chromium", "current/chromium"),
            ("current", "state/current"),
            ("previous", "state/previous"),
        ] {
            let path = self.root.join(name);
            match fs::read_link(&path) {
                Ok(t) if t == Path::new(target) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    symlink(target, &path).map_err(error)?;
                }
                _ => {
                    return Err(format!(
                        "Refusing unexpected discovery path: {}",
                        path.display()
                    ));
                }
            }
        }
        sync_dir(&self.root)
    }
    fn commit_state(&self, current: &str, previous: Option<&str>) -> Result<()> {
        self.ensure_discovery()?;
        let generation = state_id(current, previous);
        let dir = self.root.join("generations").join(&generation);
        if !dir.try_exists().map_err(error)? {
            // A fresh generation can be abandoned at any instruction boundary.
            // Only the final state symlink rename makes it visible.
            let staging = self.root.join(".generation");
            remove_tree(&staging)?;
            ensure_directory(&staging)?;
            symlink(format!("../../releases/{current}"), staging.join("current")).map_err(error)?;
            if let Some(id) = previous {
                symlink(format!("../../releases/{id}"), staging.join("previous")).map_err(error)?;
            }
            sync_dir(&staging)?;
            fs::rename(staging, &dir).map_err(error)?;
            sync_dir(&self.root.join("generations"))?;
        }
        // Read back even a generation left by an interrupted prior transaction.
        check_directory(&dir, true)?;
        if fs::read_link(dir.join("current")).map_err(error)?
            != PathBuf::from(format!("../../releases/{current}"))
            || match previous {
                Some(p) => {
                    fs::read_link(dir.join("previous")).map_err(error)?
                        != PathBuf::from(format!("../../releases/{p}"))
                }
                None => fs::symlink_metadata(dir.join("previous")).is_ok(),
            }
        {
            return Err("Damaged activation generation".into());
        }
        interruption("generation-ready")?;
        let temporary = self.root.join(".state.new");
        remove_link(&temporary)?;
        symlink(format!("generations/{generation}"), &temporary).map_err(error)?;
        sync_dir(&self.root)?;
        interruption("before-state-switch")?;
        fs::rename(&temporary, self.root.join("state")).map_err(error)?;
        interruption("after-state-switch")?;
        sync_dir(&self.root)
    }
    fn reuse_files(&self, release: &VerifiedComponent) -> Result<HashMap<String, PathBuf>> {
        let mut candidates = HashMap::new();
        // Prefer current/previous; do not trust an arbitrary local file merely
        // because its name matches a signed path or because it is hardlinked.
        let state = self.read_status()?;
        for id in [state.current, state.previous].into_iter().flatten() {
            let Ok(old) = self.verify_installed(&id) else {
                continue;
            };
            for entry in old.release.entries {
                if let ComponentEntry::File {
                    path,
                    size,
                    sha256,
                    mode,
                } = entry
                {
                    candidates
                        .entry((size, sha256, mode))
                        .or_insert_with(|| self.root.join("releases").join(&id).join(path));
                }
            }
        }
        let mut reuse = HashMap::new();
        for entry in &release.release.entries {
            if let ComponentEntry::File {
                path,
                size,
                sha256,
                mode,
            } = entry
            {
                if let Some(source) = candidates.get(&(*size, sha256.clone(), *mode)) {
                    reuse.insert(path.clone(), source.clone());
                }
            }
        }
        Ok(reuse)
    }
}
#[cfg(test)]
thread_local! { static FAIL_AT: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) }; }
fn interruption(phase: &str) -> Result<()> {
    #[cfg(test)]
    if FAIL_AT.with(|f| f.get() == Some(phase)) {
        return Err(format!("Simulated interruption: {phase}"));
    }
    let _ = phase;
    Ok(())
}
fn state_id(current: &str, previous: Option<&str>) -> String {
    digest(format!("{current}:{}", previous.unwrap_or("")).as_bytes())
}
fn check_directory(path: &Path, private: bool) -> Result<()> {
    let m = fs::symlink_metadata(path).map_err(error)?;
    if !m.is_dir()
        || m.file_type().is_symlink()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & if private { 0o077 } else { 0o022 } != 0
    {
        return Err(format!(
            "Expected an owned {}directory: {}",
            if private {
                "private "
            } else {
                "non-writable-by-others "
            },
            path.display()
        ));
    }
    Ok(())
}
fn ensure_directory(path: &Path) -> Result<()> {
    if !path.try_exists().map_err(error)? {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(error)?;
        sync_dir(path.parent().ok_or("Missing directory parent")?)?;
    }
    check_directory(path, false)
}
fn linux_filesystem(path: &Path) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(error)?;
        let mut output = std::mem::MaybeUninit::<libc::statfs>::uninit();
        if unsafe { libc::statfs(name.as_ptr(), output.as_mut_ptr()) } != 0 {
            return Err(error(std::io::Error::last_os_error()));
        }
        let kind = unsafe { output.assume_init() }.f_type as u64;
        if ![0xef53, 0x58465342, 0x9123683e, 0x794c7630].contains(&kind) {
            return Err("Browser storage requires a local Linux filesystem (ext4, XFS, Btrfs, overlay); games/FAT/exFAT/network/tmpfs are unsupported".into());
        }
    }
    let _ = path;
    Ok(())
}
fn space_check(path: &Path, required: u64) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(error)?;
    let mut output = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(name.as_ptr(), output.as_mut_ptr()) } != 0 {
        return Err(error(std::io::Error::last_os_error()));
    }
    let output = unsafe { output.assume_init() };
    let available = (output.f_bavail as u64).saturating_mul(output.f_frsize as u64);
    if available < required {
        return Err(format!(
            "Insufficient component space: need {} MiB, available {} MiB; no active component changed",
            required.div_ceil(1048576),
            available / 1048576
        ));
    }
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(error)?;
    file.write_all(bytes).map_err(error)?;
    file.sync_all().map_err(error)
}
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path).and_then(|f| f.sync_all()).map_err(error)
}
fn remove_link(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => fs::remove_file(path).map_err(error),
        Ok(_) => Err("Unexpected object in component transaction".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(error(e)),
    }
}
fn remove_tree(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {
            // Only directories are unsealed: shared immutable files are never chmod'd.
            fn unseal(path: &Path) -> Result<()> {
                fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(error)?;
                for e in fs::read_dir(path).map_err(error)? {
                    let e = e.map_err(error)?;
                    if e.file_type().map_err(error)?.is_dir() {
                        unseal(&e.path())?;
                    }
                }
                Ok(())
            }
            unseal(path)?;
            fs::remove_dir_all(path).map_err(error)
        }
        Ok(_) => Err("Unsafe staging object; refusing cleanup".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(error(e)),
    }
}

/// Authenticate a local artifact's complete compressed and expanded contents,
/// including the tar trailer, without writing or executing anything.
pub fn verify_archive(component: &VerifiedComponent, path: &Path) -> Result<()> {
    unpack(open_regular(path)?, None, component, &HashMap::new())
}
fn extract(
    reader: impl Read,
    root: &Path,
    component: &VerifiedComponent,
    reuse: &HashMap<String, PathBuf>,
) -> Result<()> {
    unpack(reader, Some(root), component, reuse)
}
struct HashedReader<R> {
    reader: R,
    hash: Sha256,
    count: u64,
    limit: u64,
}
impl<R: Read> Read for HashedReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let n = self.reader.read(bytes)?;
        self.count = self.count.saturating_add(n as u64);
        if self.count > self.limit {
            return Err(std::io::Error::other(
                "Compressed archive exceeds signed size",
            ));
        }
        self.hash.update(&bytes[..n]);
        Ok(n)
    }
}
fn unpack(
    reader: impl Read,
    root: Option<&Path>,
    component: &VerifiedComponent,
    reuse: &HashMap<String, PathBuf>,
) -> Result<()> {
    let info = &component.release.archive;
    let reader = HashedReader {
        reader,
        hash: Sha256::new(),
        count: 0,
        limit: info.size,
    };
    let mut stream = flate2::read::MultiGzDecoder::new(reader);
    let mut modes = HashMap::new();
    for expected in &component.release.entries {
        let mut block = [0; 512];
        stream.read_exact(&mut block).map_err(error)?;
        let header = tar::Header::from_byte_slice(&block);
        let checksum = block
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32u32
                } else {
                    *b as u32
                }
            })
            .sum::<u32>();
        if &block[257..265] != b"ustar\x0000" || header.cksum().map_err(error)? != checksum {
            return Err("Invalid USTAR component header/checksum".into());
        }
        let path = header.path().map_err(error)?;
        let link = header.link_name().map_err(error)?;
        let mode = if let ComponentEntry::Hardlink { target, .. } = expected {
            *modes.get(target).ok_or("Missing hardlink target")?
        } else {
            expected.mode()
        };
        let size = if let ComponentEntry::File { size, .. } = expected {
            *size
        } else {
            0
        };
        // Accept precisely the signed, sorted entries. PAX/GNU extensions,
        // duplicates, alternate spellings, special files and metadata are denied.
        let path_matches = path.to_str() == Some(expected.path())
            || (matches!(expected, ComponentEntry::Directory { .. })
                && path.to_str() == Some(&format!("{}/", expected.path())));
        if !path_matches
            || header.entry_type().as_byte() != expected.kind()
            || link.as_ref().and_then(|p| p.to_str()) != expected.target()
            || header.size().map_err(error)? != size
            || header.mode().map_err(error)? != mode
            || header.uid().map_err(error)? != 0
            || header.gid().map_err(error)? != 0
            || header.mtime().map_err(error)? != 0
            || header.username_bytes().is_some_and(|s| !s.is_empty())
            || header.groupname_bytes().is_some_and(|s| !s.is_empty())
            || block[329..345].iter().any(|b| *b != 0 && *b != b'0')
        {
            return Err(format!(
                "Archive metadata differs from signed entry: {}",
                expected.path()
            ));
        }
        let destination = root.map(|r| r.join(expected.path()));
        match expected {
            ComponentEntry::Directory { .. } => {
                if let Some(path) = &destination {
                    ensure_directory(path)?;
                }
            }
            ComponentEntry::Symlink { target, .. } => {
                if let Some(path) = &destination {
                    symlink(target, path).map_err(error)?;
                }
            }
            ComponentEntry::Hardlink { target, .. } => {
                if let (Some(root), Some(path)) = (root, &destination) {
                    fs::hard_link(root.join(target), path).map_err(error)?;
                }
            }
            ComponentEntry::File {
                path,
                size,
                sha256,
                mode,
            } => {
                let reused = reuse.get(path);
                let mut output = if reused.is_none() {
                    destination
                        .as_ref()
                        .map(|p| {
                            OpenOptions::new()
                                .write(true)
                                .create_new(true)
                                .mode(0o600)
                                .open(p)
                                .map_err(error)
                        })
                        .transpose()?
                } else {
                    None
                };
                let mut hash = Sha256::new();
                let mut remaining = *size;
                let mut buffer = [0; 65536];
                while remaining > 0 {
                    let amount = remaining.min(buffer.len() as u64) as usize;
                    stream.read_exact(&mut buffer[..amount]).map_err(error)?;
                    hash.update(&buffer[..amount]);
                    if let Some(file) = &mut output {
                        file.write_all(&buffer[..amount]).map_err(error)?;
                    }
                    remaining -= amount as u64;
                }
                if format!("{:x}", hash.finalize()) != *sha256 {
                    return Err(format!("Archive file hash mismatch: {path}"));
                }
                if let Some(file) = output {
                    file.set_permissions(fs::Permissions::from_mode(*mode))
                        .map_err(error)?;
                    file.sync_all().map_err(error)?;
                }
                if let (Some(source), Some(destination)) = (reused, &destination) {
                    // Recheck after streaming. Never chmod an inode shared with a
                    // committed release and never fall back to an unplanned copy.
                    hash_file(source, *size, sha256)?;
                    if fs::symlink_metadata(source).map_err(error)?.mode() & 0o7777 != *mode {
                        return Err("Reuse file mode changed".into());
                    }
                    fs::hard_link(source, destination).map_err(error)?;
                }
                modes.insert(path.clone(), *mode);
                let pad = (512 - size % 512) % 512;
                let mut padding = [0; 512];
                stream
                    .read_exact(&mut padding[..pad as usize])
                    .map_err(error)?;
                if padding[..pad as usize].iter().any(|b| *b != 0) {
                    return Err("Invalid tar file padding".into());
                }
            }
        }
    }
    let mut tail = Vec::new();
    stream
        .by_ref()
        .take(10753)
        .read_to_end(&mut tail)
        .map_err(error)?;
    if tail.len() < 1024
        || tail.len() > 10752
        || tail.len() % 512 != 0
        || tail.iter().any(|b| *b != 0)
    {
        return Err("Unexpected archive entries or invalid tar trailer".into());
    }
    let mut reader = stream.into_inner();
    // Also require EOF in the compressed source (no ignored trailing bytes).
    let mut extra = [0; 1];
    if reader.read(&mut extra).map_err(error)? != 0
        || reader.count != info.size
        || format!("{:x}", reader.hash.finalize()) != info.sha256
    {
        return Err("Compressed component archive hash/size mismatch".into());
    }
    Ok(())
}
fn verify_tree(
    root: &Path,
    component: &VerifiedComponent,
    sealed: bool,
    internal_links_only: bool,
) -> Result<()> {
    let root_meta = fs::symlink_metadata(root).map_err(error)?;
    if !root_meta.is_dir()
        || root_meta.file_type().is_symlink()
        || root_meta.uid() != unsafe { libc::geteuid() }
        || root_meta.mode() & 0o022 != 0
    {
        return Err("Component tree must be an owned directory, not writable by others".into());
    }
    let expected: BTreeMap<_, _> = component
        .release
        .entries
        .iter()
        .map(|e| (e.path(), e))
        .collect();
    let mut seen = 0usize;
    fn walk(
        root: &Path,
        dir: &Path,
        expected: &BTreeMap<&str, &ComponentEntry>,
        seen: &mut usize,
    ) -> Result<()> {
        for entry in fs::read_dir(dir).map_err(error)? {
            let entry = entry.map_err(error)?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(error)?
                .to_str()
                .ok_or("Non-UTF8 component path")?
                .to_string();
            if relative == RECEIPT {
                continue;
            }
            let wanted = expected
                .get(relative.as_str())
                .ok_or_else(|| format!("Unlisted component entry: {relative}"))?;
            *seen += 1;
            let kind = entry.file_type().map_err(error)?;
            if kind.is_dir() {
                if !matches!(wanted, ComponentEntry::Directory { .. }) {
                    return Err("Unexpected directory in component".into());
                }
                walk(root, &path, expected, seen)?;
            }
        }
        Ok(())
    }
    walk(root, root, &expected, &mut seen)?;
    if seen != expected.len() {
        return Err("Component inventory is incomplete".into());
    }
    let mut inodes = HashMap::<String, (u64, u64, u64)>::new();
    let mut counts = HashMap::<String, u64>::new();
    for entry in &component.release.entries {
        let path = root.join(entry.path());
        let m = fs::symlink_metadata(&path).map_err(error)?;
        if !m.file_type().is_symlink()
            && (m.uid() != unsafe { libc::geteuid() }
                || (sealed && m.mode() & 0o7022 != 0)
                || (!sealed && m.is_file() && m.mode() & 0o7000 != 0))
        {
            return Err(format!(
                "Unowned, writable-by-others or privileged component entry: {}",
                entry.path()
            ));
        }
        match entry {
            ComponentEntry::Directory { .. } => {
                if !m.is_dir() || (sealed && m.mode() & 0o7777 != 0o555) {
                    return Err("Component directory type/mode mismatch".into());
                }
            }
            ComponentEntry::File {
                path: name,
                size,
                sha256,
                mode,
            } => {
                if !m.is_file()
                    || (sealed && m.mode() & 0o7777 != *mode)
                    || (!sealed && ((m.mode() & 0o111 != 0) != (*mode == 0o555)))
                {
                    return Err("Component file type/mode mismatch".into());
                }
                hash_file(&path, *size, sha256)?;
                inodes.insert(name.clone(), (m.dev(), m.ino(), m.nlink()));
                counts.insert(name.clone(), 1);
            }
            ComponentEntry::Symlink { target, .. } => {
                if !m.file_type().is_symlink()
                    || fs::read_link(&path).map_err(error)?.to_str() != Some(target)
                {
                    return Err("Component symlink mismatch".into());
                }
            }
            ComponentEntry::Hardlink { target, .. } => {
                let (dev, ino, _) = inodes
                    .get(target)
                    .ok_or("Hardlink without canonical inode")?;
                if !m.is_file() || m.dev() != *dev || m.ino() != *ino {
                    return Err("Component hardlink mismatch".into());
                }
                *counts.get_mut(target).ok_or("Missing hardlink count")? += 1;
            }
        }
    }
    if internal_links_only && inodes.iter().any(|(p, (_, _, nlink))| counts[p] != *nlink) {
        return Err("Adoption refuses regular files hardlinked outside the component".into());
    }
    match fs::symlink_metadata(root.join(RECEIPT)) {
        Ok(m) => {
            if !m.is_file()
                || m.nlink() != 1
                || m.uid() != unsafe { libc::geteuid() }
                || (sealed && m.mode() & 0o7777 != 0o444)
                || read_text(&root.join(RECEIPT), MAX_MANIFEST)? != component.envelope
            {
                return Err("Unexpected or different signed component receipt".into());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !sealed => (),
        Err(e) => return Err(error(e)),
    }
    Ok(())
}
fn seal_tree(root: &Path) -> Result<()> {
    fn visit(path: &Path) -> Result<()> {
        for entry in fs::read_dir(path).map_err(error)? {
            let entry = entry.map_err(error)?;
            let m = fs::symlink_metadata(entry.path()).map_err(error)?;
            if m.is_dir() {
                visit(&entry.path())?;
            } else if m.is_file() {
                // Already sealed shared files are untouched.
                let mode = if m.mode() & 0o111 != 0 { 0o555 } else { 0o444 };
                if m.mode() & 0o7777 != mode {
                    fs::set_permissions(entry.path(), fs::Permissions::from_mode(mode))
                        .map_err(error)?;
                }
                open_regular(&entry.path())?.sync_all().map_err(error)?;
            } else if !m.file_type().is_symlink() {
                return Err("Special component file cannot be sealed".into());
            }
        }
        fs::set_permissions(path, fs::Permissions::from_mode(0o555)).map_err(error)?;
        sync_dir(path)
    }
    // The enclosing release directory remains private, unlike payload subdirs.
    visit(root)?;
    fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(error)?;
    sync_dir(root)
}

#[cfg(test)]
#[path = "browser_component_tests.rs"]
mod tests;
