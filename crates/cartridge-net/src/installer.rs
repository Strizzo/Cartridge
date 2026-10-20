use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::client::{HttpClient, validate_artifact_url};
use crate::registry::RegistryApp;

const MAX_PACKAGE_SIZE: u64 = 32 * 1024 * 1024;
const MAX_EXTRACTED_SIZE: u64 = 128 * 1024 * 1024;
const MAX_TAR_SIZE: u64 = MAX_EXTRACTED_SIZE + 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_MANIFEST_SIZE: u64 = 64 * 1024;

/// Verified app code lives in `~/.cartridges/apps/{id}`. Mutable app storage
/// and global settings live elsewhere and are never part of an installation.
#[derive(Clone)]
pub struct AppInstaller {
    http: HttpClient,
    install_dir: PathBuf,
}

#[derive(Deserialize)]
struct Manifest {
    id: String,
    version: String,
    name: String,
    author: String,
    entry: String,
    #[serde(default)]
    min_runtime: Option<String>,
    #[serde(default)]
    permissions: Vec<String>,
}

impl AppInstaller {
    pub fn new(http: HttpClient) -> Self {
        let installer = Self {
            http,
            install_dir: cartridge_core::paths::installed_apps_dir(),
        };
        // Recover before the launcher's directory discovery runs. Query methods
        // also recover, so an interrupted swap is repairable without a restart.
        installer.recover_all();
        installer
    }

    /// Install only the explicit, integrity-protected package in the registry.
    /// Legacy local/bundled records without a package remain readable.
    pub fn install(&self, app: &RegistryApp) -> Result<(), String> {
        validate_id(&app.id)?;
        let package = app
            .package
            .as_ref()
            .ok_or("App has no verified installable package")?;
        validate_artifact_url(&package.url)?;
        if package.size == 0 || package.size > MAX_PACKAGE_SIZE {
            return Err("Package size must be between 1 byte and 32 MiB".into());
        }
        if package.sha256.len() != 64 || !package.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Package SHA256 must contain exactly 64 hexadecimal digits".into());
        }
        Version::parse(&app.version).map_err(|e| format!("Invalid app version: {e}"))?;
        package.check_runtime()?;
        let _lock = self.lock_app(&app.id)?;
        self.recover(&app.id)?;
        let work = self.work_path(&app.id);
        let stage = work.join("stage");
        let archive = work.join("package.tar.gz");
        fs::create_dir(&stage).map_err(|e| format!("Create staging directory: {e}"))?;
        let result = (|| {
            let size = self
                .http
                .download_bounded(&package.url, &archive, package.size)?;
            if size != package.size {
                return Err(format!(
                    "Package size mismatch: expected {}, received {size}",
                    package.size
                ));
            }
            let mut input = File::open(&archive).map_err(|e| e.to_string())?;
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 16 * 1024];
            loop {
                let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            let actual = format!("{:x}", hash.finalize());
            if !actual.eq_ignore_ascii_case(&package.sha256) {
                return Err("Package SHA256 mismatch".into());
            }
            extract_package(&archive, &stage)?;
            validate_manifest(&stage, app)?;
            sync_tree(&stage)?;
            self.commit(&app.id)
        })();
        // Never clean up previous/rollback here: they may be the sole working
        // copy after an I/O failure. Recovery owns those transitions.
        let _ = remove_directory(&stage);
        let _ = remove_regular_file(&archive);
        if result.is_ok() {
            log::info!("Installed {} v{}", app.id, app.version);
        }
        result
    }

    /// Remove installed code and its rollback version, retaining app data/settings.
    pub fn remove(&self, app_id: &str) -> Result<(), String> {
        validate_id(app_id)?;
        let _lock = self.lock_app(app_id)?;
        self.recover(app_id)?;
        let live = self.install_dir.join(app_id);
        if !directory_exists(&live)? {
            return Err(format!("App {app_id} is not installed"));
        }
        // Backups must be deleted first: an interrupted removal must not cause
        // recovery to resurrect an app whose live directory was removed.
        remove_directory(&self.work_path(app_id).join("rollback"))?;
        remove_directory(&live)?;
        sync_directory(&self.install_dir)?;
        Ok(())
    }

    /// Swap the current code with the immediately preceding installed version.
    /// Rolling back again restores the version that was just replaced.
    pub fn rollback(&self, app_id: &str) -> Result<(), String> {
        validate_id(app_id)?;
        let _lock = self.lock_app(app_id)?;
        self.recover(app_id)?;
        let work = self.work_path(app_id);
        let live = self.install_dir.join(app_id);
        let backup = work.join("rollback");
        read_manifest(&live, app_id)?;
        read_manifest(&backup, app_id).map_err(|_| "No valid rollback version available")?;
        let swap = work.join("rollback-swap");
        if let Err(error) = rename_synced(&live, &swap) {
            let _ = self.recover(app_id);
            return Err(error);
        }
        if let Err(error) = rename_synced(&backup, &live) {
            let _ = self.recover(app_id);
            return Err(error);
        }
        // The rollback is committed. An interrupted cleanup is finished by
        // recovery; it does not turn a successful swap into a failed operation.
        if let Err(error) = self.recover(app_id) {
            log::warn!("Rollback cleanup pending: {error}");
        }
        Ok(())
    }

    pub fn can_rollback(&self, app_id: &str) -> bool {
        if validate_id(app_id).is_err() {
            return false;
        }
        self.try_recover(app_id);
        self.is_installed(app_id)
            && directory_exists(&self.install_dir.join(".installer")).unwrap_or(false)
            && directory_exists(&self.work_path(app_id)).unwrap_or(false)
            && read_manifest(&self.work_path(app_id).join("rollback"), app_id).is_ok()
    }

    /// Compare the available version with installed code, or the bundled
    /// version when no installed override exists. Build metadata is ignored.
    pub fn has_update(&self, app: &RegistryApp) -> bool {
        self.has_update_in(app, &cartridge_core::paths::bundled_cartridges_dir())
    }

    fn has_update_in(&self, app: &RegistryApp, bundled: &Path) -> bool {
        if app.package.is_none() || validate_id(&app.id).is_err() {
            return false;
        }
        self.try_recover(&app.id);
        let Some(path) =
            cartridge_core::paths::resolve_cartridge_dir_in(&app.id, &self.install_dir, bundled)
        else {
            return false;
        };
        let Ok(installed) = read_manifest(&path, &app.id) else {
            return false;
        };
        match (
            Version::parse(&installed.version),
            Version::parse(&app.version),
        ) {
            (Ok(installed), Ok(available)) => available.cmp_precedence(&installed).is_gt(),
            _ => false,
        }
    }

    pub fn is_installed(&self, app_id: &str) -> bool {
        if validate_id(app_id).is_err() || !directory_exists(&self.install_dir).unwrap_or(false) {
            return false;
        }
        read_manifest(&self.app_path(app_id), app_id).is_ok()
    }

    pub fn list_installed(&self) -> Vec<String> {
        self.recover_all();
        let mut ids = Vec::new();
        if !directory_exists(&self.install_dir).unwrap_or(false) {
            return ids;
        }
        if let Ok(entries) = fs::read_dir(&self.install_dir) {
            for entry in entries.flatten() {
                if let Some(id) = entry.file_name().to_str() {
                    if self.is_installed(id) {
                        ids.push(id.to_owned());
                    }
                }
            }
        }
        ids.sort();
        ids
    }

    pub fn installed_version(&self, app_id: &str) -> Option<String> {
        validate_id(app_id).ok()?;
        if !directory_exists(&self.install_dir).ok()? {
            return None;
        }
        read_manifest(&self.app_path(app_id), app_id)
            .ok()
            .map(|m| m.version)
    }

    /// Invalid IDs resolve to a reserved, unusable location inside the install
    /// root, preserving the infallible legacy API without joining unsafe input.
    pub fn app_path(&self, app_id: &str) -> PathBuf {
        let invalid = self.install_dir.join(".invalid-app-id");
        if validate_id(app_id).is_err() {
            return invalid;
        }
        self.try_recover(app_id);
        let live = self.install_dir.join(app_id);
        if directory_exists(&self.install_dir).is_err() || directory_exists(&live).is_err() {
            return invalid;
        }
        live
    }

    fn work_path(&self, id: &str) -> PathBuf {
        self.install_dir.join(".installer").join(id)
    }

    /// Kernel advisory locks are released on process death, unlike lockfiles.
    /// They serialize cloned installers and separate launcher processes.
    fn lock_app(&self, id: &str) -> Result<File, String> {
        validate_id(id)?;
        ensure_directory(&self.install_dir)?;
        ensure_directory(&self.install_dir.join(".installer"))?;
        let work = self.work_path(id);
        ensure_directory(&work)?;
        let path = work.join("lock");
        regular_file_exists(&path)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|e| format!("Open installer lock: {e}"))?;
        file.try_lock()
            .map_err(|e| format!("App installation is busy: {e}"))?;
        Ok(file)
    }

    fn commit(&self, id: &str) -> Result<(), String> {
        let work = self.work_path(id);
        let live = self.install_dir.join(id);
        let previous = work.join("previous");
        if directory_exists(&live)? {
            if let Err(error) = rename_synced(&live, &previous) {
                let _ = self.recover(id);
                return Err(error);
            }
        }
        if let Err(error) = rename_synced(&work.join("stage"), &live) {
            let _ = self.recover(id);
            return Err(error);
        }
        if let Err(error) = self.recover(id) {
            log::warn!("Install cleanup pending: {error}");
        }
        Ok(())
    }

    /// A rename is the commit point. If live is absent, restore old code; if it
    /// is present, finish moving the old code to rollback. No unverified stage
    /// is ever promoted by recovery. The same rule applies to rollback-swap.
    fn recover(&self, id: &str) -> Result<(), String> {
        let work = self.work_path(id);
        let live = self.install_dir.join(id);
        let backup = work.join("rollback");
        // Check every path before any deletion or rename, including dangling links.
        for path in [
            &live,
            &backup,
            &work.join("previous"),
            &work.join("rollback-swap"),
            &work.join("stage"),
        ] {
            directory_exists(path)?;
        }
        for name in ["previous", "rollback-swap"] {
            let pending = work.join(name);
            if directory_exists(&pending)? {
                if directory_exists(&live)? {
                    remove_directory(&backup)?;
                    rename_synced(&pending, &backup)?;
                } else {
                    rename_synced(&pending, &live)?;
                }
            }
        }
        remove_directory(&work.join("stage"))?;
        remove_regular_file(&work.join("package.tar.gz"))?;
        Ok(())
    }

    fn try_recover(&self, id: &str) {
        if !directory_exists(&self.install_dir).unwrap_or(false)
            || !directory_exists(&self.install_dir.join(".installer")).unwrap_or(false)
            || !directory_exists(&self.work_path(id)).unwrap_or(false)
        {
            return;
        }
        if let Ok(_lock) = self.lock_app(id) {
            if let Err(error) = self.recover(id) {
                log::warn!("Cannot recover {id}: {error}");
            }
        }
    }

    fn recover_all(&self) {
        if !directory_exists(&self.install_dir).unwrap_or(false)
            || !directory_exists(&self.install_dir.join(".installer")).unwrap_or(false)
        {
            return;
        }
        if let Ok(entries) = fs::read_dir(self.install_dir.join(".installer")) {
            for entry in entries.flatten() {
                if let Some(id) = entry.file_name().to_str() {
                    if validate_id(id).is_ok() {
                        self.try_recover(id);
                    }
                }
            }
        }
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id.as_bytes()[0].is_ascii_alphanumeric()
        || !id.as_bytes()[id.len() - 1].is_ascii_alphanumeric()
        || id.contains("..")
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err("Invalid app ID".into());
    }
    Ok(())
}

fn directory_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => Ok(true),
        Ok(_) => Err(format!("Expected a real directory: {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("Inspect {}: {e}", path.display())),
    }
}

fn regular_file_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(true),
        Ok(_) => Err(format!("Expected a regular file: {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("Inspect {}: {e}", path.display())),
    }
}

fn ensure_directory(path: &Path) -> Result<(), String> {
    if !directory_exists(path)? {
        fs::create_dir_all(path).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            sync_directory(parent)?;
        }
    }
    Ok(())
}

fn remove_directory(path: &Path) -> Result<(), String> {
    if directory_exists(path)? {
        fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn remove_regular_file(path: &Path) -> Result<(), String> {
    if regular_file_exists(path)? {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("Sync {}: {e}", path.display()))
}

fn rename_synced(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to)
        .map_err(|e| format!("Rename {} to {}: {e}", from.display(), to.display()))?;
    sync_directory(from.parent().ok_or("Missing source parent")?)?;
    sync_directory(to.parent().ok_or("Missing destination parent")?)
}

fn sync_tree(path: &Path) -> Result<(), String> {
    for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            sync_tree(&entry.path())?;
        }
    }
    sync_directory(path)
}

fn read_manifest(path: &Path, id: &str) -> Result<Manifest, String> {
    if !directory_exists(path)? {
        return Err("App is not installed".into());
    }
    let manifest_path = path.join("cartridge.json");
    if !regular_file_exists(&manifest_path)? {
        return Err("Missing cartridge.json".into());
    }
    let mut text = String::new();
    File::open(manifest_path)
        .map_err(|e| e.to_string())?
        .take(MAX_MANIFEST_SIZE + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() as u64 > MAX_MANIFEST_SIZE {
        return Err("Manifest exceeds 64 KiB".into());
    }
    let manifest: Manifest =
        serde_json::from_str(&text).map_err(|e| format!("Invalid cartridge.json: {e}"))?;
    if manifest.id != id {
        return Err("Manifest app ID mismatch".into());
    }
    if manifest.entry != "main.lua" || !regular_file_exists(&path.join("main.lua"))? {
        return Err("Lua app must have entry main.lua and a regular main.lua file".into());
    }
    if manifest.name.trim().is_empty() || manifest.author.trim().is_empty() {
        return Err("Manifest needs a name and author".into());
    }
    Ok(manifest)
}

fn validate_manifest(path: &Path, app: &RegistryApp) -> Result<(), String> {
    let manifest = read_manifest(path, &app.id)?;
    if manifest.version != app.version {
        return Err("Manifest version mismatch".into());
    }
    if let Some(minimum) = manifest.min_runtime.as_deref() {
        let required = Version::parse(minimum)
            .map_err(|e| format!("Invalid manifest minimum runtime: {e}"))?;
        let runtime = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;
        if required.cmp_precedence(&runtime).is_gt() {
            return Err(format!(
                "Manifest requires runtime {required}; this runtime is {runtime}"
            ));
        }
        let signed = app.package.as_ref().ok_or("App has no verified package")?;
        if minimum != signed.min_runtime {
            return Err("Manifest minimum runtime differs from signed package".into());
        }
    }
    let actual: HashSet<_> = manifest.permissions.iter().collect();
    let expected: HashSet<_> = app.permissions.iter().collect();
    if actual != expected || actual.len() != manifest.permissions.len() {
        return Err("Manifest permissions mismatch".into());
    }
    Ok(())
}

fn archive_path(bytes: &[u8]) -> Result<PathBuf, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Non-UTF8 archive path")?;
    if text.len() > 1024
        || text.starts_with('/')
        || text.contains(['\\', ':'])
        || text.chars().any(char::is_control)
    {
        return Err("Unsafe archive path".into());
    }
    let mut path = PathBuf::new();
    let mut depth = 0;
    for part in text.split('/') {
        if part == ".." {
            return Err("Archive path traversal".into());
        }
        if part.is_empty() || part == "." {
            continue;
        }
        depth += 1;
        if depth > 16 {
            return Err("Archive path is too deep".into());
        }
        path.push(part);
    }
    Ok(path)
}

fn extract_package(archive_path_on_disk: &Path, stage: &Path) -> Result<(), String> {
    let input = File::open(archive_path_on_disk).map_err(|e| e.to_string())?;
    let decoded = flate2::read::MultiGzDecoder::new(input).take(MAX_TAR_SIZE + 1);
    let mut archive = tar::Archive::new(decoded);
    let mut seen = HashSet::new();
    let mut total = 0u64;
    // Raw entries prevent hidden PAX/GNU extensions from changing paths/sizes
    // or allocating unbounded metadata. Release archives use regular USTAR.
    for (index, entry) in archive
        .entries()
        .map_err(|e| e.to_string())?
        .raw(true)
        .enumerate()
    {
        if index >= MAX_ENTRIES {
            return Err("Archive has too many entries".into());
        }
        let mut entry = entry.map_err(|e| format!("Invalid tar entry: {e}"))?;
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            return Err("Archive contains a link or unsupported entry type".into());
        }
        let relative = archive_path(&entry.path_bytes())?;
        if relative.as_os_str().is_empty() {
            if kind.is_dir() {
                continue;
            }
            return Err("Empty archive filename".into());
        }
        // Also rejects case aliases on the default macOS filesystem.
        if !seen.insert(relative.to_string_lossy().to_lowercase()) {
            return Err("Duplicate archive path".into());
        }
        let size = entry.size();
        total = total.checked_add(size).ok_or("Extracted size overflow")?;
        if total > MAX_EXTRACTED_SIZE {
            return Err("Extracted package exceeds 128 MiB".into());
        }
        let dest = stage.join(&relative);
        if kind.is_dir() {
            if size != 0 {
                return Err("Directory entry has file contents".into());
            }
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        } else {
            fs::create_dir_all(dest.parent().ok_or("Missing archive parent")?)
                .map_err(|e| e.to_string())?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dest)
                .map_err(|e| format!("Create extracted file: {e}"))?;
            let written =
                std::io::copy(&mut entry, &mut output).map_err(|e| format!("Extract file: {e}"))?;
            if written != size {
                return Err("Truncated tar entry".into());
            }
            output
                .flush()
                .and_then(|_| output.sync_all())
                .map_err(|e| e.to_string())?;
        }
    }
    // Validate the gzip checksum/trailer too, and reject a second hidden tar
    // after the end marker. Zero padding is permitted but remains bounded.
    let mut remaining = archive.into_inner();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let n = remaining
            .read(&mut buffer)
            .map_err(|e| format!("Invalid gzip: {e}"))?;
        if n == 0 {
            break;
        }
        if buffer[..n].iter().any(|b| *b != 0) {
            return Err("Nonzero data after tar end marker".into());
        }
    }
    if remaining.limit() == 0 {
        return Err("Decompressed tar exceeds limit".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "installer_tests.rs"]
mod tests;
