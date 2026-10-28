use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use url::Url;

pub const VIEW_WIDTH: u32 = 720;
pub const VIEW_HEIGHT: u32 = 576;
pub const VIEW_Y: i32 = 100;
pub const MAX_URL: usize = 4096;
pub const ZOOM_LEVELS: &[u16] = &[60, 75, 90, 100, 110, 120, 130, 140, 150, 160, 175, 200];
pub const DEFAULT_ZOOM: u16 = 100;
const MAX_SITE_VIEWS: usize = 128;

pub fn site_key(address: &str) -> Option<String> {
    let url = Url::parse(address).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.origin().ascii_serialization())
}

pub fn address(input: &str) -> Result<String, String> {
    let input = input.trim();
    if input.is_empty() || input.len() > MAX_URL || input.chars().any(char::is_control) {
        return Err("Enter a web address or search terms (at most 4096 bytes).".into());
    }
    let candidate = if input.contains("://") {
        input.to_string()
    } else if input.starts_with("localhost:") || input.starts_with("127.0.0.1:") {
        format!("http://{input}")
    } else if !input.contains(' ') && input.contains('.') {
        format!("https://{input}")
    } else if input.contains(':') && !input.contains(' ') {
        return Err("Only http:// and https:// web addresses are supported.".into());
    } else {
        let mut url = Url::parse("https://duckduckgo.com/").unwrap();
        url.query_pairs_mut().append_pair("q", input);
        return Ok(url.to_string());
    };
    let url = Url::parse(&candidate).map_err(|_| "That web address is not valid.")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Use an http:// or https:// address without embedded credentials.".into());
    }
    Ok(url.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Library {
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<Bookmark>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub site_zoom: BTreeMap<String, u16>,
    #[serde(skip)]
    pub recovery_notice: Option<String>,
}
impl Library {
    pub fn load(path: &Path) -> Self {
        let mut library = match read_library(path) {
            Ok(library) => library,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => Self {
                recovery_notice: Some(format!(
                    "Saved library could not be read ({error}). The original will be preserved before saving a new library."
                )),
                ..Default::default()
            },
        };
        library.bookmarks.retain(|b| address(&b.url).is_ok());
        library.history.retain(|b| address(&b.url).is_ok());
        library.bookmarks.truncate(64);
        library.history.truncate(100);
        library.site_zoom.retain(|origin, zoom| {
            site_key(origin).as_deref() == Some(origin.as_str()) && ZOOM_LEVELS.contains(zoom)
        });
        while library.site_zoom.len() > MAX_SITE_VIEWS {
            library.site_zoom.pop_first();
        }
        library
    }
    pub fn zoom_for(&self, url: &str) -> u16 {
        site_key(url)
            .and_then(|key| self.site_zoom.get(&key).copied())
            .unwrap_or(DEFAULT_ZOOM)
    }
    pub fn remember_zoom(&mut self, url: &str, zoom: u16) -> bool {
        let Some(key) = site_key(url).filter(|_| ZOOM_LEVELS.contains(&zoom)) else {
            return false;
        };
        if self.site_zoom.get(&key) == Some(&zoom) {
            return false;
        }
        if self.site_zoom.len() >= MAX_SITE_VIEWS && !self.site_zoom.contains_key(&key) {
            self.site_zoom.pop_first();
        }
        self.site_zoom.insert(key, zoom);
        true
    }
    pub fn remember(&mut self, title: &str, url: &str) {
        if address(url).is_err() {
            return;
        }
        let base = url.split('#').next().unwrap_or(url);
        self.history
            .retain(|b| b.url.split('#').next().unwrap_or(&b.url) != base);
        self.history.insert(
            0,
            Bookmark {
                title: title.chars().take(160).collect(),
                url: url.into(),
            },
        );
        self.history.truncate(100);
    }
    pub fn toggle_bookmark(&mut self, title: &str, url: &str) {
        if let Some(i) = self.bookmarks.iter().position(|b| b.url == url) {
            self.bookmarks.remove(i);
        } else if address(url).is_ok() && self.bookmarks.len() < 64 {
            self.bookmarks.push(Bookmark {
                title: title.chars().take(160).collect(),
                url: url.into(),
            });
        }
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let parent = path.parent().ok_or("Missing browser data directory")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let (temporary, mut file) = unique_file(parent, "library-save")?;
        let result = (|| -> Result<(), String> {
            let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
            if bytes.len() > 1024 * 1024 {
                return Err("Browser library exceeded its 1 MiB limit.".into());
            }
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            // Read failures are not permission to overwrite. Malformed regular
            // files are first copied, synced and retained under a unique name.
            match read_library(path) {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                    let original = OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open(path)
                        .map_err(|e| e.to_string())?;
                    let meta = original.metadata().map_err(|e| e.to_string())?;
                    if !meta.is_file() || meta.len() > 4 * 1024 * 1024 {
                        return Err("Library is not a regular file within the recovery limit; original preserved. Move it aside to recover.".into());
                    }
                    let backups = std::fs::read_dir(parent)
                        .map_err(|e| e.to_string())?
                        .take(512)
                        .filter_map(Result::ok)
                        .filter(|e| {
                            e.file_name()
                                .to_string_lossy()
                                .starts_with(".library-corrupt-")
                        })
                        .take(4)
                        .count();
                    if backups >= 4 {
                        return Err("Four corrupt library backups already exist. Original preserved; move backups aside before saving.".into());
                    }
                    let (backup, mut output) = unique_file(parent, "library-corrupt")?;
                    let copied =
                        std::io::copy(&mut original.take(4 * 1024 * 1024 + 1), &mut output)
                            .and_then(|n| {
                                output.sync_all()?;
                                Ok(n)
                            })
                            .map_err(|e| e.to_string())?;
                    if copied != meta.len() {
                        return Err(format!(
                            "Library changed during recovery; original and {} preserved.",
                            backup.display()
                        ));
                    }
                    sync_directory(parent).map_err(|e| e.to_string())?;
                    log::warn!("Preserved corrupt browser library at {}", backup.display());
                }
                Err(error) => {
                    return Err(format!("Cannot safely replace browser library: {error}"));
                }
            }
            std::fs::rename(&temporary, path).map_err(|e| e.to_string())?;
            sync_directory(parent).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}

fn read_library(path: &Path) -> std::io::Result<Library> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Library is not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Library is too large",
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}
fn unique_file(parent: &Path, label: &str) -> Result<(std::path::PathBuf, File), String> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for _ in 0..8 {
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".{label}-{}-{stamp}-{serial}.json",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("Could not reserve a unique library temporary file.".into())
}
fn sync_directory(parent: &Path) -> std::io::Result<()> {
    // Linux4.4 exFAT may not implement directory fsync. Always attempt it, and
    // accept only that explicit unsupported result after syncing file contents.
    match File::open(parent)?.sync_all() {
        Err(e) if matches!(e.raw_os_error(), Some(libc::EINVAL) | Some(libc::ENOTSUP)) => {
            log::warn!("Browser data filesystem does not support directory fsync");
            Ok(())
        }
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_root(label: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("browser-library-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn corrupt_library_is_preserved_byte_for_byte_before_replacement() {
        let root = test_root("corrupt");
        let path = root.join("library.json");
        let broken = b"{broken JSON with only copy of saved data";
        std::fs::write(&path, broken).unwrap();
        let mut library = Library::load(&path);
        assert!(library.recovery_notice.is_some());
        library.toggle_bookmark("new", "https://example.org/");
        library.save(&path).unwrap();
        let backups = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".library-corrupt-")
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        assert_eq!(std::fs::read(backups[0].path()).unwrap(), broken);
        assert_eq!(Library::load(&path).bookmarks.len(), 1);
        assert!(Library::load(&path).recovery_notice.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn save_uses_unique_private_temporaries_and_refuses_symlink_destination() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = test_root("unique");
        let sentinel = root.join("sentinel");
        std::fs::write(&sentinel, "keep").unwrap();
        let path = root.join("library.json");
        symlink(&sentinel, root.join("library.tmp")).unwrap();
        let library = Library::default();
        library.save(&path).unwrap();
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::remove_file(&path).unwrap();
        symlink(&sentinel, &path).unwrap();
        assert!(library.save(&path).is_err());
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
        assert!(
            !std::fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .any(|e| e
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".library-save-"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn concurrent_saves_leave_a_complete_json_library() {
        let root = test_root("concurrent");
        let path = root.join("library.json");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let threads = (0..2)
            .map(|i| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut library = Library::default();
                    library.toggle_bookmark(&format!("{i}"), "https://example.org/");
                    barrier.wait();
                    library.save(&path).unwrap();
                })
            })
            .collect::<Vec<_>>();
        for t in threads {
            t.join().unwrap();
        }
        let saved = Library::load(&path);
        assert_eq!(saved.bookmarks.len(), 1);
        assert!(saved.recovery_notice.is_none());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn zoom_is_per_origin_and_old_libraries_keep_bookmarks() {
        let mut library: Library = serde_json::from_str(
            r#"{"bookmarks":[{"title":"saved","url":"https://example.org/a"}],"history":[]}"#,
        )
        .unwrap();
        assert_eq!(library.bookmarks.len(), 1);
        assert_eq!(library.zoom_for("https://example.org/a"), 100);
        assert!(library.remember_zoom("https://example.org/a?q=x#y", 130));
        assert_eq!(library.zoom_for("https://example.org/other"), 130);
        assert_eq!(library.zoom_for("http://example.org/"), 100);
        assert_eq!(library.zoom_for("https://example.org:444/"), 100);
        assert!(!library.remember_zoom("file:///private", 130));
        assert!(!library.remember_zoom("https://example.org/", 999));
        let saved = serde_json::to_string(&library).unwrap();
        let restored: Library = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.zoom_for("https://example.org/"), 130);
        for index in 0..200 {
            library.remember_zoom(&format!("https://s{index}.example.org/"), 110);
        }
        assert_eq!(library.site_zoom.len(), MAX_SITE_VIEWS);
    }
    #[test]
    fn url_and_search_are_distinct() {
        assert_eq!(
            address("example.com/a?q=x").unwrap(),
            "https://example.com/a?q=x"
        );
        assert_eq!(
            address("http://localhost:1234/a").unwrap(),
            "http://localhost:1234/a"
        );
        assert_eq!(
            address("localhost:1234/a").unwrap(),
            "http://localhost:1234/a"
        );
        assert_eq!(
            address("127.0.0.1:8765/").unwrap(),
            "http://127.0.0.1:8765/"
        );
        assert!(
            address("game consoles")
                .unwrap()
                .contains("q=game+consoles")
        );
    }
    #[test]
    fn privileged_schemes_and_credentials_are_not_addresses() {
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "chrome://settings",
            "data:text/html,x",
            "https://u:p@example.org",
            "http://",
            "",
            "https://example.org/\nfoo",
        ] {
            assert!(address(url).is_err(), "accepted {url}");
        }
    }
    #[test]
    fn bookmarks_and_history_are_bounded_and_deduplicated() {
        let mut library = Library::default();
        for i in 0..120 {
            library.remember("A", &format!("https://example.org/{i}"));
        }
        assert_eq!(library.history.len(), 100);
        library.remember("New title", "https://example.org/119");
        assert_eq!(library.history.len(), 100);
        library.toggle_bookmark("A", "https://example.org/");
        assert_eq!(library.bookmarks.len(), 1);
        library.toggle_bookmark("A", "https://example.org/");
        assert!(library.bookmarks.is_empty());
    }
}
