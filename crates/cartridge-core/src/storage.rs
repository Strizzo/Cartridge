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
        let base = dirs_home().join(".cartridges");
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
        let path = self.data_dir.join(format!("{key}.json"));
        if let Ok(content) = serde_json::to_string_pretty(data) {
            fs::write(path, content).ok();
        }
    }

    pub fn load(&self, key: &str) -> Option<serde_json::Value> {
        let path = self.data_dir.join(format!("{key}.json"));
        let content = fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn delete(&self, key: &str) {
        let path = self.data_dir.join(format!("{key}.json"));
        fs::remove_file(path).ok();
    }

    pub fn list_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.data_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json")
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
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}
