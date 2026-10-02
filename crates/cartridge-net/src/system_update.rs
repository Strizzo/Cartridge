//! Signed, staged payload updates. Never writes the boot partition or Linux root.
//! The stable Python session supervisor owns activation and startup rollback.
use ed25519_dalek::{Signature, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const UPDATE_URL: &str =
    "https://github.com/Strizzo/Cartridge/releases/latest/download/system-update.json";
pub const KEY_ID: &str = "cartridge-os-v1";
pub const TARGET: &str = "r36s-plus-aarch64";
const PUBLIC_KEY: &str = include_str!("../../../scripts/system-update-public-key.hex");
const MAX_MANIFEST: u64 = 4 * 1024 * 1024;
const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_UNPACKED: u64 = 384 * 1024 * 1024;
const MAX_FILES: usize = 8192;
const RESERVE: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveInfo {
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub unpacked_size: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub schema: u32,
    pub channel: String,
    pub version: String,
    pub revision: String,
    pub target: String,
    pub min_runtime: String,
    pub min_supervisor: u32,
    pub notes: String,
    pub archive: ArchiveInfo,
    pub files: Vec<ReleaseFile>,
    #[serde(skip)]
    envelope: String,
}
impl Release {
    pub fn id(&self) -> String {
        format!("{}-{}", self.version, &self.revision[..12])
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    key_id: String,
    payload: String,
    signature: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateStatus {
    pub schema: u32,
    pub active: Option<String>,
    pub previous: Option<String>,
    pub pending: Option<String>,
    pub trial: Option<String>,
    pub last_result: String,
}
impl Default for UpdateStatus {
    fn default() -> Self {
        Self {
            schema: 1,
            active: None,
            previous: None,
            pending: None,
            trial: None,
            last_result: String::new(),
        }
    }
}

#[derive(Clone)]
pub struct SystemUpdater {
    root: PathBuf,
    state: PathBuf,
    agent: ureq::Agent,
}
impl SystemUpdater {
    /// Only the updated session supervisor can authorize installation paths.
    pub fn from_environment() -> Result<Self, String> {
        if std::env::var("CARTRIDGE_UPDATE_SUPERVISOR").as_deref() != Ok("1") {
            return Err("Install the updater-enabled CartridgeOS bundle once from your Mac. Future system updates will then be available here over Wi-Fi.".into());
        }
        if std::env::var("CARTRIDGE_UPDATE_TARGET").as_deref() != Ok(TARGET) {
            return Err("System updates are not supported on this device profile".into());
        }
        let root = PathBuf::from(
            std::env::var_os("CARTRIDGE_UPDATE_ROOT").ok_or("Missing update installation path")?,
        );
        let state = PathBuf::from(
            std::env::var_os("CARTRIDGE_UPDATE_STATE").ok_or("Missing update state path")?,
        );
        Self::at_paths(root, state)
    }
    fn at_paths(root: PathBuf, state: PathBuf) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|e| format!("Installation directory: {e}"))?;
        let state = state
            .canonicalize()
            .map_err(|e| format!("Session state directory: {e}"))?;
        if !root.is_dir() || !state.is_dir() || state.starts_with(&root) || root.starts_with(&state)
        {
            return Err("System state must be separate from the release storage".into());
        }
        reject_link(&root.join("cartridge"))?;
        if !root.join("cartridge").is_file() {
            return Err("Stable recovery executable is missing".into());
        }
        ensure_state_filesystem(&state)?;
        Ok(Self {
            root,
            state,
            agent: ureq::Agent::config_builder()
                .user_agent(concat!("CartridgeOS/", env!("CARGO_PKG_VERSION")))
                .http_status_as_error(false)
                .timeout_global(Some(Duration::from_secs(300)))
                .https_only(true)
                .max_redirects(5)
                .build()
                .into(),
        })
    }
    pub fn status(&self) -> Result<UpdateStatus, String> {
        read_state(&self.state)
    }
    pub fn check(&self) -> Result<Option<Release>, String> {
        let response = self
            .agent
            .get(UPDATE_URL)
            .header("Accept-Encoding", "identity")
            .config()
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .call()
            .map_err(|e| format!("Could not check for system updates: {e}"))?;
        if response.status().as_u16() == 404 {
            return Err("No signed system release has been published yet. Your current version is unchanged.".into());
        }
        if response.status().as_u16() != 200 {
            return Err(format!("Update server returned {}", response.status()));
        }
        let mut bytes = Vec::new();
        response
            .into_body()
            .into_reader()
            .take(MAX_MANIFEST + 1)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err("Update manifest is too large".into());
        }
        let body = std::str::from_utf8(&bytes).map_err(err)?;
        let release = verify_manifest(body)?;
        let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(err)?;
        if Version::parse(&release.min_runtime)
            .map_err(err)?
            .cmp_precedence(&current)
            .is_gt()
        {
            return Err(format!(
                "This update needs CartridgeOS {} first. Install its bootstrap bundle from your Mac.",
                release.min_runtime
            ));
        }
        if !Version::parse(&release.version)
            .map_err(err)?
            .cmp_precedence(&current)
            .is_gt()
        {
            return Ok(None);
        }
        Ok(Some(release))
    }
    /// Explicit UI confirmation authorizes staging only. Restart is a separate action.
    pub fn stage(
        &self,
        release: &Release,
        battery: Option<u8>,
        charging: bool,
        mut progress: impl FnMut(String),
    ) -> Result<(), String> {
        if !cfg!(all(target_os = "linux", target_arch = "aarch64")) {
            return Err("System installation requires the ARM Linux handheld or ARM VM. The Mac simulator can preview the update screen and check releases.".into());
        }
        // Public display fields never authorize an install; re-read their signed source.
        let release = verify_manifest(&release.envelope)?;
        self.stage_inner(
            &release,
            battery,
            charging,
            &decode_hex::<32>(PUBLIC_KEY.trim())?,
            &mut progress,
            |info, path, progress| download(&self.agent, info, path, progress),
        )
    }
    fn stage_inner(
        &self,
        release: &Release,
        battery: Option<u8>,
        charging: bool,
        key: &[u8; 32],
        progress: &mut impl FnMut(String),
        fetch: impl FnOnce(&ArchiveInfo, &Path, &mut dyn FnMut(String)) -> Result<(), String>,
    ) -> Result<(), String> {
        let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(err)?;
        if !Version::parse(&release.version)
            .map_err(err)?
            .cmp_precedence(&current)
            .is_gt()
        {
            return Err("Only a newer signed system release can be installed".into());
        }
        if Version::parse(&release.min_runtime)
            .map_err(err)?
            .cmp_precedence(&current)
            .is_gt()
        {
            return Err("This release requires a newer bootstrap installation".into());
        }
        power_check(battery, charging)?;
        let _lock = state_lock(&self.state)?;
        let mut state = read_state(&self.state)?;
        if state.trial.is_some() {
            return Err("Wait for the current system update to finish its startup check".into());
        }
        if state.pending.is_some() {
            return Err(
                "An update is already ready. Restart CartridgeOS to apply it first.".into(),
            );
        }
        let releases = self.root.join("releases");
        ensure_directory(&releases)?;
        let required = release
            .archive
            .size
            .checked_add(release.archive.unpacked_size)
            .and_then(|s| s.checked_add(RESERVE))
            .ok_or("Invalid update storage size")?;
        space_check(available_space(&releases)?, required)?;
        space_check(available_space(&self.state)?, 1024 * 1024)?;
        let incoming = releases.join(".incoming");
        // The lock guarantees that only our abandoned staging area is cleaned.
        remove_staging(&incoming)?;
        fs::create_dir(&incoming).map_err(err)?;
        let result = (|| {
            let archive = incoming.join("download.tar.gz");
            progress(format!("Downloading CartridgeOS {}…", release.version));
            fetch(&release.archive, &archive, progress)?;
            let extracted = incoming.join("payload");
            fs::create_dir(&extracted).map_err(err)?;
            progress("Verifying and unpacking system files…".into());
            extract(&archive, &extracted, &release)?;
            write_new_synced(
                &extracted.join("system-release.json"),
                release.envelope.as_bytes(),
            )?;
            sync_tree(&extracted)?;
            let destination = releases.join(release.id());
            if destination.exists() {
                // A power loss after rename but before state commit leaves a valid
                // unreferenced release. It may be reused, never overwritten.
                let existing = verify_dir_with_key(&destination, key)?;
                if existing.envelope != release.envelope {
                    return Err("A different release already uses this version identifier".into());
                }
            } else {
                reject_link(&destination)?;
                fs::rename(&extracted, &destination).map_err(err)?;
                sync_dir(&releases)?;
            }
            // Read back the final directory before it becomes a boot candidate.
            verify_dir_with_key(&destination, key)?;
            state.pending = Some(release.id());
            state.last_result = format!(
                "CartridgeOS {} is ready. Restart to install.",
                release.version
            );
            write_state(&self.state, &state)?;
            progress("Verified update ready. Restart CartridgeOS when convenient.".into());
            Ok(())
        })();
        let _ = remove_staging(&incoming);
        result
    }
}

pub fn verify_manifest(body: &str) -> Result<Release, String> {
    verify_with_key(body, &decode_hex::<32>(PUBLIC_KEY.trim())?)
}
fn verify_with_key(body: &str, public: &[u8; 32]) -> Result<Release, String> {
    if body.len() as u64 > MAX_MANIFEST {
        return Err("System manifest is too large".into());
    }
    let envelope: Envelope =
        serde_json::from_str(body).map_err(|e| format!("Invalid signed system manifest: {e}"))?;
    if envelope.key_id != KEY_ID {
        return Err("Unknown system release signing key".into());
    }
    VerifyingKey::from_bytes(public)
        .map_err(err)?
        .verify_strict(
            envelope.payload.as_bytes(),
            &Signature::from_bytes(&decode_hex::<64>(&envelope.signature)?),
        )
        .map_err(|_| "System release signature did not verify")?;
    let mut release: Release = serde_json::from_str(&envelope.payload).map_err(err)?;
    validate_release(&release)?;
    release.envelope = body.to_string();
    Ok(release)
}
fn validate_release(r: &Release) -> Result<(), String> {
    if r.schema != 1 || r.channel != "stable" || r.target != TARGET || r.min_supervisor != 1 {
        return Err("Unsupported system release or device target".into());
    }
    let version = Version::parse(&r.version).map_err(err)?;
    Version::parse(&r.min_runtime).map_err(err)?;
    if !version.pre.is_empty()
        || !version.build.is_empty()
        || r.version.len() > 64
        || r.revision.len() != 40
        || !lower_hex(&r.revision)
        || r.notes.len() > 8000
    {
        return Err("Invalid system release identity".into());
    }
    validate_id(&r.id())?;
    let expected = format!(
        "https://github.com/Strizzo/Cartridge/releases/download/v{}/cartridgeos-r36s-plus-{}.tar.gz",
        r.version, r.version
    );
    if r.archive.url != expected
        || r.archive.size == 0
        || r.archive.size > MAX_ARCHIVE
        || r.archive.unpacked_size == 0
        || r.archive.unpacked_size > MAX_UNPACKED
    {
        return Err("Invalid system archive URL or size".into());
    }
    decode_hex::<32>(&r.archive.sha256)?;
    if r.files.is_empty() || r.files.len() > MAX_FILES {
        return Err("Invalid number of system files".into());
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    for f in &r.files {
        validate_path(&f.path)?;
        decode_hex::<32>(&f.sha256)?;
        if !names.insert(f.path.as_str()) {
            return Err("Duplicate system file".into());
        }
        total = total
            .checked_add(f.size)
            .ok_or("System file size overflow")?;
        if total > MAX_UNPACKED {
            return Err("System files exceed size limit".into());
        }
    }
    for name in ["cartridge", "game-library.py", "registry.json"] {
        if !names.contains(name) {
            return Err(format!("Required system file missing: {name}"));
        }
    }
    if !names
        .iter()
        .any(|p| p.starts_with("assets/fonts/") && p.ends_with(".ttf"))
        || total != r.archive.unpacked_size
    {
        return Err("Missing system fonts or incorrect unpacked size".into());
    }
    Ok(())
}
fn lower_hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn decode_hex<const N: usize>(s: &str) -> Result<[u8; N], String> {
    if s.len() != N * 2 || !s.is_ascii() {
        return Err("Invalid checksum or signature encoding".into());
    }
    let mut bytes = [0; N];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(err)?;
    }
    Ok(bytes)
}
fn validate_id(id: &str) -> Result<(), String> {
    if id.len() > 100 {
        return Err("Invalid release directory identifier".into());
    }
    let (version, revision) = id
        .rsplit_once('-')
        .ok_or("Invalid release directory identifier")?;
    let parsed = Version::parse(version).map_err(err)?;
    if !parsed.pre.is_empty()
        || !parsed.build.is_empty()
        || revision.len() != 12
        || !lower_hex(revision)
    {
        return Err("Invalid release directory identifier".into());
    }
    Ok(())
}
fn validate_path(path: &str) -> Result<(), String> {
    if path.len() > 220
        || !path.is_ascii()
        || path.split('/').count() > 12
        || path.split('/').any(|p| {
            p.is_empty()
                || p == "."
                || p == ".."
                || p.starts_with('.')
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    {
        return Err("Invalid system archive path".into());
    }
    if [
        "cartridge",
        "game-library.py",
        "registry.json",
        "assets/gamecontrollerdb.txt",
    ]
    .contains(&path)
    {
        return Ok(());
    }
    if [
        "assets/fonts/",
        "assets/icons/",
        "assets/overlays/",
        "assets/brand/",
        "lua_cartridges/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
    {
        return Ok(());
    }
    Err(format!("File is outside the system update scope: {path}"))
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn reject_link(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => {
            Err(format!("Refusing symbolic link: {}", path.display()))
        }
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(err(e)),
    }
}
fn ensure_directory(path: &Path) -> Result<(), String> {
    reject_link(path)?;
    if !path.exists() {
        fs::create_dir(path).map_err(err)?;
        sync_dir(path.parent().ok_or("Missing parent")?)?;
    }
    if !path.is_dir() {
        return Err("Expected an update directory".into());
    }
    Ok(())
}
fn state_lock(dir: &Path) -> Result<File, String> {
    let path = dir.join("system-update.lock");
    reject_link(&path)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(err)?;
    file.try_lock()
        .map_err(|_| "Another system update is in progress")?;
    Ok(file)
}
fn read_state(dir: &Path) -> Result<UpdateStatus, String> {
    let path = dir.join("system-update.json");
    reject_link(&path)?;
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(UpdateStatus::default()),
        Err(e) => return Err(err(e)),
    };
    let mut bytes = Vec::new();
    file.take(16385).read_to_end(&mut bytes).map_err(err)?;
    if bytes.len() > 16384 {
        return Err("System update state is too large".into());
    }
    let object: serde_json::Value = serde_json::from_slice(&bytes).map_err(err)?;
    let keys = [
        "schema",
        "active",
        "previous",
        "pending",
        "trial",
        "last_result",
    ];
    if !object
        .as_object()
        .is_some_and(|o| o.len() == keys.len() && keys.iter().all(|key| o.contains_key(*key)))
    {
        return Err("System update state fields are missing or unsupported".into());
    }
    let state: UpdateStatus = serde_json::from_slice(&bytes)
        .map_err(|_| "System update state is damaged; recovery is required before updating")?;
    if state.schema != 1 || state.last_result.len() > 8000 {
        return Err("Unsupported system update state".into());
    }
    for id in [&state.active, &state.previous, &state.pending, &state.trial]
        .into_iter()
        .flatten()
    {
        validate_id(id)?;
    }
    Ok(state)
}
fn write_state(dir: &Path, state: &UpdateStatus) -> Result<(), String> {
    let temporary = dir.join("system-update.json.new");
    reject_link(&temporary)?;
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(err)?;
    }
    write_new_synced(&temporary, &serde_json::to_vec(state).map_err(err)?)?;
    fs::rename(temporary, dir.join("system-update.json")).map_err(err)?;
    sync_dir(dir)
}
fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(err)?;
    f.write_all(bytes).map_err(err)?;
    f.sync_all().map_err(err)
}
fn sync_dir(path: &Path) -> Result<(), String> {
    File::open(path).and_then(|f| f.sync_all()).map_err(err)
}
fn sync_tree(path: &Path) -> Result<(), String> {
    for e in fs::read_dir(path).map_err(err)? {
        let e = e.map_err(err)?;
        if e.file_type().map_err(err)?.is_dir() {
            sync_tree(&e.path())?;
        }
    }
    sync_dir(path)
}
fn remove_staging(path: &Path) -> Result<(), String> {
    reject_link(path)?;
    if path.exists() {
        fs::remove_dir_all(path).map_err(err)?;
    }
    Ok(())
}
fn power_check(battery: Option<u8>, charging: bool) -> Result<(), String> {
    if !charging && battery.is_none_or(|b| b < 30) {
        return Err(
            "Connect the charger or charge to at least 30% before downloading a system update"
                .into(),
        );
    }
    Ok(())
}
fn space_check(available: u64, required: u64) -> Result<(), String> {
    if available < required {
        return Err(format!(
            "Not enough free space: need {} MiB, available {} MiB",
            required.div_ceil(1024 * 1024),
            available / (1024 * 1024)
        ));
    }
    Ok(())
}
fn available_space(path: &Path) -> Result<u64, String> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(err)?;
        let mut data = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // statvfs initializes the output on success; path is NUL-terminated.
        if unsafe { libc::statvfs(path.as_ptr(), data.as_mut_ptr()) } != 0 {
            return Err(err(std::io::Error::last_os_error()));
        }
        let data = unsafe { data.assume_init() };
        Ok((data.f_bavail as u64).saturating_mul(data.f_frsize as u64))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("System updater requires a Unix filesystem".into())
    }
}
fn ensure_state_filesystem(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(err)?;
        let mut fs = std::mem::MaybeUninit::<libc::statfs>::uninit();
        if unsafe { libc::statfs(name.as_ptr(), fs.as_mut_ptr()) } != 0 {
            return Err(err(std::io::Error::last_os_error()));
        }
        let kind = unsafe { fs.assume_init() }.f_type as u64;
        // FAT/exFAT cannot provide the Linux-root state durability contract.
        if kind == 0x4d44 || kind == 0x2011bab0 {
            return Err("System update state must live on the Linux filesystem".into());
        }
    }
    let _ = path;
    Ok(())
}
fn hash_reader(mut input: impl Read, limit: u64) -> Result<(u64, String), String> {
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut bytes = [0; 65536];
    loop {
        let n = input.read(&mut bytes).map_err(err)?;
        if n == 0 {
            break;
        }
        size = size.checked_add(n as u64).ok_or("Size overflow")?;
        if size > limit {
            return Err("File exceeds its signed size".into());
        }
        hasher.update(&bytes[..n]);
    }
    Ok((size, format!("{:x}", hasher.finalize())))
}
fn download(
    agent: &ureq::Agent,
    info: &ArchiveInfo,
    path: &Path,
    progress: &mut (impl FnMut(String) + ?Sized),
) -> Result<(), String> {
    let response = agent
        .get(&info.url)
        .header("Accept-Encoding", "identity")
        .call()
        .map_err(|e| format!("System download failed: {e}"))?;
    if response.status().as_u16() != 200 {
        return Err(format!("System download returned {}", response.status()));
    }
    if let Some(encoding) = response.headers().get("content-encoding") {
        if encoding.to_str().ok() != Some("identity") {
            return Err("Unexpected archive encoding".into());
        }
    }
    if let Some(length) = response.headers().get("content-length") {
        if length.to_str().ok().and_then(|s| s.parse::<u64>().ok()) != Some(info.size) {
            return Err("System download size does not match the signed release".into());
        }
    }
    copy_download(response.into_body().into_reader(), info, path, progress)
}
fn copy_download(
    mut reader: impl Read,
    info: &ArchiveInfo,
    path: &Path,
    progress: &mut (impl FnMut(String) + ?Sized),
) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(err)?;
    let result = (|| {
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let mut buf = [0; 65536];
        let mut last = 0;
        loop {
            let n = reader.read(&mut buf).map_err(err)?;
            if n == 0 {
                break;
            }
            size += n as u64;
            if size > info.size {
                return Err("System download exceeds its signed size".into());
            }
            file.write_all(&buf[..n]).map_err(err)?;
            hash.update(&buf[..n]);
            let percent = size * 100 / info.size;
            if percent >= last + 5 {
                progress(format!("Downloading… {percent}%"));
                last = percent;
            }
        }
        if size != info.size || format!("{:x}", hash.finalize()) != info.sha256.to_ascii_lowercase()
        {
            return Err("System download is incomplete or its checksum does not match".into());
        }
        file.sync_all().map_err(err)
    })();
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}
fn extract(archive: &Path, dest: &Path, release: &Release) -> Result<(), String> {
    let file = File::open(archive).map_err(err)?;
    let decoded = flate2::read::GzDecoder::new(file).take(MAX_UNPACKED + 16 * 1024 * 1024);
    let mut archive = tar::Archive::new(decoded);
    let mut seen = HashSet::new();
    let expected: BTreeMap<&str, &ReleaseFile> =
        release.files.iter().map(|f| (f.path.as_str(), f)).collect();
    for entry in archive.entries().map_err(err)?.raw(true) {
        let mut entry = entry.map_err(err)?;
        if entry.header().as_ustar().is_none() || !entry.header().entry_type().is_file() {
            return Err("System archive must contain only USTAR regular files".into());
        }
        let bytes = entry.path_bytes();
        let name = std::str::from_utf8(&bytes).map_err(err)?.to_string();
        validate_path(&name)?;
        let metadata = expected
            .get(name.as_str())
            .ok_or("System archive contains a file not in its signed manifest")?;
        if !seen.insert(name.clone()) || entry.size() != metadata.size {
            return Err("Duplicate system file or incorrect signed size".into());
        }
        let path = dest.join(&name);
        fs::create_dir_all(path.parent().ok_or("Missing file parent")?).map_err(err)?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(err)?;
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut buf = [0; 65536];
        loop {
            let n = entry.read(&mut buf).map_err(err)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            output.write_all(&buf[..n]).map_err(err)?;
            hash.update(&buf[..n]);
        }
        if count != metadata.size
            || format!("{:x}", hash.finalize()) != metadata.sha256.to_ascii_lowercase()
        {
            return Err(format!("System file checksum mismatch: {name}"));
        }
        output.sync_all().map_err(err)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(if name == "cartridge" { 0o755 } else { 0o644 }),
            )
            .map_err(err)?;
        }
    }
    if seen.len() != expected.len() {
        return Err("System archive is missing signed files".into());
    }
    check_elf(&dest.join("cartridge"))?;
    Ok(())
}
/// Called by the immutable base binary before any staged code is executed.
pub fn verify_release_dir(dir: &Path) -> Result<Release, String> {
    verify_dir_with_key(dir, &decode_hex::<32>(PUBLIC_KEY.trim())?)
}
fn verify_dir_with_key(dir: &Path, key: &[u8; 32]) -> Result<Release, String> {
    reject_link(dir)?;
    let metadata = dir.join("system-release.json");
    reject_link(&metadata)?;
    let mut body = String::new();
    File::open(metadata)
        .map_err(err)?
        .take(MAX_MANIFEST + 1)
        .read_to_string(&mut body)
        .map_err(err)?;
    let release = verify_with_key(&body, key)?;
    if dir.file_name().and_then(|s| s.to_str()) != Some(release.id().as_str()) {
        return Err("Release directory does not match its signed identity".into());
    }
    let mut seen = HashSet::new();
    walk_regular(dir, dir, &mut seen)?;
    seen.remove("system-release.json");
    let expected: HashSet<String> = release.files.iter().map(|f| f.path.clone()).collect();
    if seen != expected {
        return Err("System release file list has changed since download".into());
    }
    for f in &release.files {
        let path = dir.join(&f.path);
        let (size, sha) = hash_reader(File::open(&path).map_err(err)?, f.size)?;
        if size != f.size || sha != f.sha256.to_ascii_lowercase() {
            return Err(format!("System release file is damaged: {}", f.path));
        }
    }
    check_elf(&dir.join("cartridge"))?;
    Ok(release)
}
fn walk_regular(root: &Path, dir: &Path, files: &mut HashSet<String>) -> Result<(), String> {
    for e in fs::read_dir(dir).map_err(err)? {
        let e = e.map_err(err)?;
        let kind = e.file_type().map_err(err)?;
        if kind.is_dir() {
            let depth = e
                .path()
                .strip_prefix(root)
                .map_err(err)?
                .components()
                .count();
            if depth > 12 {
                return Err("System release directory is too deep".into());
            }
            walk_regular(root, &e.path(), files)?;
        } else if kind.is_file() {
            if files.len() >= MAX_FILES + 1 {
                return Err("Too many system release files".into());
            }
            files.insert(
                e.path()
                    .strip_prefix(root)
                    .map_err(err)?
                    .to_str()
                    .ok_or("Non-UTF8 system path")?
                    .to_string(),
            );
        } else {
            return Err("System releases cannot contain links or special files".into());
        }
    }
    Ok(())
}
fn check_elf(path: &Path) -> Result<(), String> {
    let mut bytes = [0u8; 20];
    File::open(path)
        .map_err(err)?
        .read_exact(&mut bytes)
        .map_err(err)?;
    if &bytes[..4] != b"\x7fELF"
        || bytes[4] != 2
        || bytes[5] != 1
        || u16::from_le_bytes([bytes[18], bytes[19]]) != 183
    {
        return Err("System executable is not Linux ARM64 ELF".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture {
        dir: PathBuf,
        updater: SystemUpdater,
        key: SigningKey,
        release: Release,
        archive: Vec<u8>,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "cartridge-system-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir(&dir).unwrap();
            let root = dir.join("Cartridge");
            let state = dir.join("state");
            fs::create_dir(&root).unwrap();
            fs::create_dir(&state).unwrap();
            fs::write(root.join("cartridge"), b"stable base").unwrap();
            fs::write(dir.join("game.srm"), b"precious save").unwrap();
            let updater = SystemUpdater::at_paths(root, state).unwrap();
            let key = SigningKey::from_bytes(&[76; 32]);
            let mut elf = vec![0; 64];
            elf[..4].copy_from_slice(b"\x7fELF");
            elf[4] = 2;
            elf[5] = 1;
            elf[18] = 183;
            let files: Vec<(&str, Vec<u8>)> = vec![
                ("cartridge", elf),
                ("game-library.py", b"print('fixture')".to_vec()),
                ("registry.json", b"{}".to_vec()),
                ("assets/fonts/test.ttf", b"test font".to_vec()),
            ];
            let mut builder = tar::Builder::new(Vec::new());
            let mut records = Vec::new();
            let mut total = 0;
            for (name, data) in &files {
                let mut h = tar::Header::new_ustar();
                h.set_path(name).unwrap();
                h.set_size(data.len() as u64);
                h.set_mode(0o644);
                h.set_cksum();
                builder.append(&h, data.as_slice()).unwrap();
                records.push(ReleaseFile {
                    path: name.to_string(),
                    size: data.len() as u64,
                    sha256: format!("{:x}", Sha256::digest(data)),
                });
                total += data.len() as u64;
            }
            builder.finish().unwrap();
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            gz.write_all(&builder.into_inner().unwrap()).unwrap();
            let archive = gz.finish().unwrap();
            let release=Release{schema:1,channel:"stable".into(),version:"99.0.0".into(),revision:"ab".repeat(20),target:TARGET.into(),min_runtime:"0.6.1".into(),min_supervisor:1,notes:"A fixture".into(),
                archive:ArchiveInfo{url:"https://github.com/Strizzo/Cartridge/releases/download/v99.0.0/cartridgeos-r36s-plus-99.0.0.tar.gz".into(),size:archive.len() as u64,sha256:format!("{:x}",Sha256::digest(&archive)),unpacked_size:total},files:records,envelope:String::new()};
            let mut f = Self {
                dir,
                updater,
                key,
                release,
                archive,
            };
            f.resign();
            f
        }
        fn resign(&mut self) {
            let payload = serde_json::to_string(&self.release).unwrap();
            let signature = self
                .key
                .sign(payload.as_bytes())
                .to_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            self.release.envelope =
                serde_json::json!({"key_id":KEY_ID,"payload":payload,"signature":signature})
                    .to_string();
        }
        fn stage(&self) -> Result<(), String> {
            self.updater.stage_inner(
                &self.release,
                Some(80),
                false,
                &self.key.verifying_key().to_bytes(),
                &mut |_| {},
                |info, path, progress| copy_download(self.archive.as_slice(), info, path, progress),
            )
        }
        fn release_dir(&self) -> PathBuf {
            self.updater.root.join("releases").join(self.release.id())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
    #[test]
    fn signature_and_target_must_verify_before_install() {
        let mut f = Fixture::new();
        assert!(verify_with_key(&f.release.envelope, &f.key.verifying_key().to_bytes()).is_ok());
        assert!(verify_manifest(&f.release.envelope).is_err()); // No production trust in fixture key.
        let mut envelope: serde_json::Value = serde_json::from_str(&f.release.envelope).unwrap();
        envelope["payload"] = serde_json::Value::String(
            envelope["payload"]
                .as_str()
                .unwrap()
                .replace("99.0.0", "98.0.0"),
        );
        assert!(verify_with_key(&envelope.to_string(), &f.key.verifying_key().to_bytes()).is_err());
        f.release.target = "other-board".into();
        f.resign();
        assert!(verify_with_key(&f.release.envelope, &f.key.verifying_key().to_bytes()).is_err());
    }
    #[test]
    fn rejects_unknown_schema_urls_paths_and_missing_files() {
        let f = Fixture::new();
        for name in [
            "../kernel",
            "/kernel",
            "assets/fonts/../../kernel",
            "assets/.ssh/key",
            "deploy/cartridge-session.py",
            "cartridge-boot",
            "assets/logo.bmp",
            "lua_cartridges/a/.env",
            "assets/fonts/a\\b",
        ] {
            let mut r = f.release.clone();
            r.files[1].path = name.into();
            assert!(validate_release(&r).is_err(), "{name}");
        }
        let mut r = f.release.clone();
        r.schema = 2;
        assert!(validate_release(&r).is_err());
        r = f.release.clone();
        r.archive.url = "https://evil.example/release.tgz".into();
        assert!(validate_release(&r).is_err());
        r = f.release.clone();
        r.files.pop();
        assert!(validate_release(&r).is_err());
        r = f.release.clone();
        r.files.push(r.files[0].clone());
        assert!(validate_release(&r).is_err());
    }
    #[test]
    fn download_rejects_truncated_oversized_corrupt_and_interrupted_body() {
        let f = Fixture::new();
        let path = f.dir.join("download");
        for bytes in [
            &f.archive[..f.archive.len() - 1],
            [f.archive.as_slice(), b"extra"].concat().as_slice(),
            vec![0; f.archive.len()].as_slice(),
        ] {
            assert!(copy_download(bytes, &f.release.archive, &path, &mut |_| {}).is_err());
            assert!(!path.exists());
        }
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "network interrupted",
                ))
            }
        }
        assert!(copy_download(Broken, &f.release.archive, &path, &mut |_| {}).is_err());
        assert!(!path.exists());
    }
    #[test]
    fn stage_preserves_working_files_and_only_sets_pending() {
        let f = Fixture::new();
        let old = UpdateStatus {
            active: Some("0.6.1-123456abcdef".into()),
            previous: Some("0.6.0-123456abcdef".into()),
            ..Default::default()
        };
        write_state(&f.updater.state, &old).unwrap();
        f.stage().unwrap();
        let state = f.updater.status().unwrap();
        assert_eq!(state.active, old.active);
        assert_eq!(state.previous, old.previous);
        assert_eq!(state.pending, Some(f.release.id()));
        assert!(state.trial.is_none());
        assert_eq!(
            fs::read(f.updater.root.join("cartridge")).unwrap(),
            b"stable base"
        );
        assert_eq!(fs::read(f.dir.join("game.srm")).unwrap(), b"precious save");
        assert!(!f.updater.root.join("releases/.incoming").exists());
        assert!(verify_dir_with_key(&f.release_dir(), &f.key.verifying_key().to_bytes()).is_ok());
        assert!(f.stage().unwrap_err().contains("already ready"));
    }
    #[test]
    fn damaged_extracted_file_fails_before_pending_is_set() {
        let mut f = Fixture::new();
        f.release.files[1].sha256 = "00".repeat(32);
        f.resign();
        assert!(f.stage().is_err());
        assert!(f.updater.status().unwrap().pending.is_none());
        assert!(!f.release_dir().exists());
    }
    #[test]
    fn readback_detects_changed_extra_symlink_and_missing_files() {
        let f = Fixture::new();
        f.stage().unwrap();
        let dir = f.release_dir();
        fs::write(dir.join("game-library.py"), b"tampered").unwrap();
        assert!(verify_dir_with_key(&dir, &f.key.verifying_key().to_bytes()).is_err());
        let f = Fixture::new();
        f.stage().unwrap();
        fs::write(f.release_dir().join("extra"), b"extra").unwrap();
        assert!(verify_dir_with_key(&f.release_dir(), &f.key.verifying_key().to_bytes()).is_err());
        let f = Fixture::new();
        f.stage().unwrap();
        fs::remove_file(f.release_dir().join("registry.json")).unwrap();
        assert!(verify_dir_with_key(&f.release_dir(), &f.key.verifying_key().to_bytes()).is_err());
        #[cfg(unix)]
        {
            let f = Fixture::new();
            f.stage().unwrap();
            std::os::unix::fs::symlink(f.dir.join("game.srm"), f.release_dir().join("link"))
                .unwrap();
            assert!(
                verify_dir_with_key(&f.release_dir(), &f.key.verifying_key().to_bytes()).is_err()
            );
        }
    }
    #[test]
    fn orphan_release_after_interrupted_state_commit_is_reused() {
        let f = Fixture::new();
        f.stage().unwrap();
        write_state(&f.updater.state, &UpdateStatus::default()).unwrap();
        f.stage().unwrap();
        assert_eq!(f.updater.status().unwrap().pending, Some(f.release.id()));
    }
    #[test]
    fn abandoned_partial_download_is_cleaned_and_retried() {
        let f = Fixture::new();
        fs::create_dir_all(f.updater.root.join("releases/.incoming/payload")).unwrap();
        fs::write(
            f.updater.root.join("releases/.incoming/download.tar.gz"),
            b"partial",
        )
        .unwrap();
        f.stage().unwrap();
        assert!(f.updater.status().unwrap().pending.is_some());
    }
    #[test]
    fn conflicting_existing_release_never_overwritten() {
        let f = Fixture::new();
        fs::create_dir_all(f.release_dir()).unwrap();
        fs::write(f.release_dir().join("owner.txt"), b"keep").unwrap();
        assert!(f.stage().is_err());
        assert_eq!(
            fs::read(f.release_dir().join("owner.txt")).unwrap(),
            b"keep"
        );
        assert!(f.updater.status().unwrap().pending.is_none());
    }
    #[test]
    fn transaction_is_exclusive_and_trial_cannot_be_replaced() {
        let f = Fixture::new();
        let lock = state_lock(&f.updater.state).unwrap();
        assert!(f.stage().unwrap_err().contains("in progress"));
        drop(lock);
        let state = UpdateStatus {
            trial: Some("0.6.2-123456abcdef".into()),
            ..Default::default()
        };
        write_state(&f.updater.state, &state).unwrap();
        assert!(f.stage().unwrap_err().contains("startup check"));
    }
    #[test]
    fn bad_state_cannot_be_reset_by_update() {
        let f = Fixture::new();
        for state in [b"broken".as_slice(),br#"{"schema":2,"active":null,"previous":null,"pending":null,"trial":null,"last_result":""}"#.as_slice(),br#"{"schema":1,"active":"../../etc","previous":null,"pending":null,"trial":null,"last_result":""}"#.as_slice()]{fs::write(f.updater.state.join("system-update.json"),state).unwrap();assert!(f.stage().is_err());assert_eq!(fs::read(f.updater.state.join("system-update.json")).unwrap(),state);}
    }
    #[test]
    fn insufficient_power_and_space_do_not_authorize_install() {
        assert!(power_check(Some(29), false).is_err());
        assert!(power_check(None, false).is_err());
        assert!(power_check(Some(30), false).is_ok());
        assert!(power_check(None, true).is_ok());
        assert!(space_check(1023, 1024).is_err());
        assert!(space_check(1024, 1024).is_ok());
    }
    #[test]
    fn no_links_in_transaction_directories() {
        #[cfg(unix)]
        {
            let f = Fixture::new();
            std::os::unix::fs::symlink(&f.dir, f.updater.root.join("releases")).unwrap();
            assert!(f.stage().is_err());
            assert_eq!(fs::read(f.dir.join("game.srm")).unwrap(), b"precious save");
        }
    }
    #[test]
    fn architecture_is_checked_even_with_valid_file_hash() {
        let mut f = Fixture::new();
        let dir = f.dir.join("extract");
        fs::create_dir(&dir).unwrap();
        let path = f.dir.join("test.tar.gz");
        fs::write(&path, &f.archive).unwrap();
        extract(&path, &dir, &f.release).unwrap();
        let binary = dir.join("cartridge");
        fs::write(&binary, b"#!/bin/sh\necho not an ARM executable").unwrap();
        assert!(check_elf(&binary).is_err());
        f.release.version = "0.1.0".into();
        assert!(
            f.updater
                .stage_inner(
                    &f.release,
                    Some(80),
                    false,
                    &f.key.verifying_key().to_bytes(),
                    &mut |_| {},
                    |_, _, _| panic!("download must not run")
                )
                .is_err()
        );
    }
    #[test]
    fn tar_links_extensions_and_unlisted_entries_are_rejected() {
        let f = Fixture::new();
        for kind in [
            tar::EntryType::Symlink,
            tar::EntryType::Link,
            tar::EntryType::GNULongName,
            tar::EntryType::Directory,
        ] {
            let mut tar = tar::Builder::new(Vec::new());
            let mut h = tar::Header::new_ustar();
            h.set_path("cartridge").unwrap();
            h.set_size(0);
            h.set_mode(0o644);
            h.set_entry_type(kind);
            h.set_cksum();
            tar.append(&h, &[][..]).unwrap();
            tar.finish().unwrap();
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            gz.write_all(&tar.into_inner().unwrap()).unwrap();
            let archive = f.dir.join("bad.tgz");
            fs::write(&archive, gz.finish().unwrap()).unwrap();
            let dest = f.dir.join("unpack");
            fs::create_dir_all(&dest).unwrap();
            assert!(extract(&archive, &dest, &f.release).is_err());
        }
    }
}
