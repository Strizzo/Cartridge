use serde::{Deserialize, Serialize};
use std::path::Path;

/// A single app entry from the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub repo_url: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// The registry file format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub apps: Vec<AppEntry>,
}

impl Registry {
    /// Load registry from a JSON file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read registry: {e}"))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse registry: {e}"))
    }

    /// Create an empty registry as fallback.
    pub fn empty() -> Self {
        Self {
            version: 1,
            apps: Vec::new(),
        }
    }

    /// Convert from the network registry type into the launcher's local type.
    pub fn from_net(net_reg: &cartridge_net::Registry) -> Self {
        Self {
            version: net_reg.version,
            apps: net_reg
                .apps
                .iter()
                .map(|a| AppEntry {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    description: a.description.clone(),
                    version: a.version.clone(),
                    author: a.author.clone(),
                    category: a.category.clone(),
                    tags: a.tags.clone(),
                    repo_url: a.repo_url.clone(),
                    permissions: a.permissions.clone(),
                })
                .collect(),
        }
    }
}

/// Tracks which apps are installed locally.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstalledApps {
    pub app_ids: Vec<String>,
}

impl InstalledApps {
    pub fn is_installed(&self, app_id: &str) -> bool {
        self.app_ids.iter().any(|id| id == app_id)
    }

    pub fn install(&mut self, app_id: &str) {
        if !self.is_installed(app_id) {
            self.app_ids.push(app_id.to_string());
        }
    }

    pub fn remove(&mut self, app_id: &str) {
        self.app_ids.retain(|id| id != app_id);
    }
}

/// Recent app launch record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentEntry {
    pub app_id: String,
    pub name: String,
    pub timestamp_secs: u64,
}

/// All categories including the "All" pseudo-category.
pub const CATEGORIES: &[&str] = &[
    "All",
    "News",
    "Finance",
    "Tools",
    "Productivity",
    "Games",
    "Social",
    "Media",
];

/// Launcher settings persisted via AppStorage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherSettings {
    pub registry_url: String,
    pub auto_refresh: bool,
    pub cache_duration_mins: u32,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            registry_url: "https://raw.githubusercontent.com/Strizzo/Cartridge/main/registry.json".to_string(),
            auto_refresh: true,
            cache_duration_mins: 60,
        }
    }
}
