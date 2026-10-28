use std::fs;
use std::path::PathBuf;

/// Scoped key-value storage for app data.
pub struct AppStorage {
    pub app_id: String,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl AppStorage {
    pub fn new(app_id: &str) -> Self {
        Self::at_root(app_id, dirs_home().join(".cartridges"))
    }

    /// Explicit root for isolated app scenarios; no process-wide HOME changes.
    pub fn at_root(app_id: &str, base: PathBuf) -> Self {
        let data_dir = base.join(app_id).join("data");
        let cache_dir = base.join(app_id).join("cache");
        fs::create_dir_all(&data_dir).ok();
        fs::create_dir_all(&cache_dir).ok();
        Self {
            app_id: app_id.to_string(),
            data_dir,
            cache_dir,
        }
    }

    pub fn save(&self, key: &str, data: &serde_json::Value) {
        if let Err(error) = self.try_save(key, data) {
            log::warn!("Could not save {} / {key}: {error}", self.app_id);
        }
    }

    fn valid_key(key: &str) -> bool {
        !key.is_empty() && key != "." && key != ".." && !key.contains(['/', '\\', '\0'])
    }

    /// Publish a complete JSON file and report write failures to callers.
    pub fn try_save(&self, key: &str, data: &serde_json::Value) -> std::io::Result<()> {
        use std::io::{Error, ErrorKind, Write};
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        if !Self::valid_key(key) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Storage key must be a filename",
            ));
        }
        let content = serde_json::to_vec_pretty(data)?;
        let path = self.data_dir.join(format!("{key}.json"));
        let temp = self.data_dir.join(format!(
            ".{key}.{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(&content)?;
            file.sync_all()?;
            fs::rename(&temp, &path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    pub fn load(&self, key: &str) -> Option<serde_json::Value> {
        if !Self::valid_key(key) {
            return None;
        }
        let path = self.data_dir.join(format!("{key}.json"));
        // Refuse a leaf symlink at open time, rather than checking then
        // following a replaced path. The VM's storage namespace is not an
        // OS-level boundary against another process owned by the same user.
        #[cfg(unix)]
        let file = {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&path)
                .ok()?
        };
        #[cfg(not(unix))]
        let file = {
            if !fs::symlink_metadata(&path).ok()?.file_type().is_file() {
                return None;
            }
            fs::File::open(&path).ok()?
        };
        if !file.metadata().ok()?.is_file() {
            return None;
        }
        serde_json::from_reader(file).ok()
    }

    pub fn delete(&self, key: &str) {
        if !Self::valid_key(key) {
            return;
        }
        let path = self.data_dir.join(format!("{key}.json"));
        fs::remove_file(path).ok();
    }

    pub fn list_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.data_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if entry.file_type().is_ok_and(|kind| kind.is_file())
                    && path.extension().and_then(|e| e.to_str()) == Some("json")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    keys.push(stem.to_string());
                }
            }
        }
        keys
    }
}

fn dirs_home() -> PathBuf {
    crate::paths::home_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn storage_reads_and_deletes_stay_inside_the_app_namespace() {
        let root =
            std::env::temp_dir().join(format!("cartridge-storage-scope-{}", std::process::id()));
        let storage = AppStorage::at_root("app.one", root.join("apps"));
        let other = AppStorage::at_root("app.two", root.join("apps"));
        let value = serde_json::json!({"fixture": "belongs to another app"});
        other.try_save("state", &value).unwrap();
        let outside = root.join("outside.json");
        fs::write(&outside, serde_json::to_vec(&value).unwrap()).unwrap();
        for key in [
            "../../app.two/data/state".to_string(),
            root.join("outside").to_string_lossy().into_owned(),
        ] {
            assert_eq!(storage.load(&key), None, "must not read another namespace");
            storage.delete(&key);
            assert_eq!(other.load("state"), Some(value.clone()));
            assert!(outside.is_file(), "must not delete outside its namespace");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, storage.data_dir.join("linked.json")).unwrap();
            assert_eq!(
                storage.load("linked"),
                None,
                "must not follow a file symlink"
            );
            assert!(!storage.list_keys().contains(&"linked".to_string()));
            storage.delete("linked");
            assert!(outside.is_file());
        }
        storage.try_save("normal.v1", &value).unwrap();
        assert_eq!(storage.load("normal.v1"), Some(value));
        storage.delete("normal.v1");
        assert!(storage.load("normal.v1").is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn saves_replace_complete_json_and_surface_unwritable_storage() {
        let root =
            std::env::temp_dir().join(format!("cartridge-storage-check-{}", std::process::id()));
        let storage = AppStorage::at_root("test", root.clone());
        let first = serde_json::json!({"favorites":["one"]});
        storage.try_save("frequency.v1", &first).unwrap();
        assert_eq!(storage.load("frequency.v1"), Some(first));
        let second = serde_json::json!({"favorites":["one","two"]});
        storage.try_save("frequency.v1", &second).unwrap();
        assert_eq!(storage.load("frequency.v1"), Some(second.clone()));
        assert!(storage.try_save("../escape", &second).is_err());
        fs::remove_dir_all(&storage.data_dir).unwrap();
        fs::write(&storage.data_dir, "blocked directory").unwrap();
        assert!(storage.try_save("frequency.v1", &second).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
