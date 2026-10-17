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
    #[serde(default)]
    pub package: Option<cartridge_net::registry::AppPackage>,
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
        let mut registry: Self = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse registry: {e}"))?;
        // Local registry files describe launchable legacy apps. Only from_net,
        // after signature verification, may supply downloadable packages.
        for app in &mut registry.apps { app.package = None; }
        Ok(registry)
    }

    /// Create an empty registry as fallback.
    pub fn empty() -> Self {
        Self {
            version: 1,
            apps: Vec::new(),
        }
    }

    /// A store refresh must not hide locally installed cartridges merely
    /// because a remote catalogue predates the installed app bundle.
    pub fn retain_installed_from(&mut self, previous: &Registry, installed: &InstalledApps) {
        for app in &previous.apps {
            if installed.is_installed(&app.id) && !self.apps.iter().any(|a| a.id == app.id) {
                self.apps.push(app.clone());
            }
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
                    package: a.package.clone(),
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
    /// Show the htop-like process panel on the home screen.
    /// Disabling it cuts ~64 text draws per frame and frees 200px of screen.
    #[serde(default = "default_show_processes")]
    pub show_processes: bool,
    /// Visual theme preset id (see cartridge_core::theme::THEME_PRESETS).
    #[serde(default = "default_theme_id")]
    pub theme_id: String,
    /// Whether to render the per-theme animated overlays (sweep line).
    /// Off saves a few frames of constant redraws when idle.
    #[serde(default = "default_animations_enabled")]
    pub animations_enabled: bool,
    /// Play short synthesized sounds on navigation and app launch.
    #[serde(default = "default_sounds_enabled")]
    pub sounds_enabled: bool,
}

fn default_show_processes() -> bool {
    false
}

fn default_theme_id() -> String {
    cartridge_core::theme::DEFAULT_THEME_ID.to_string()
}

fn default_animations_enabled() -> bool {
    true
}

fn default_sounds_enabled() -> bool {
    true
}

pub const OFFICIAL_REGISTRY_URL: &str = cartridge_net::registry::OFFICIAL_CATALOG_URL;
const OLD_OFFICIAL_REGISTRY_URL: &str = "https://raw.githubusercontent.com/Strizzo/Cartridge/main/registry.json";

impl LauncherSettings {
    /// Only migrate known defaults. Explicit custom URLs remain untouched.
    pub fn migrate_registry_url(&mut self) -> bool {
        let host = self.registry_url.split_once("://")
            .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase());
        if self.registry_url == OLD_OFFICIAL_REGISTRY_URL
            || host.as_deref().is_some_and(|h| h == "cartridge.dev" || h.ends_with(".cartridge.dev"))
        {
            self.registry_url = OFFICIAL_REGISTRY_URL.to_string();
            true
        } else { false }
    }
}

impl AppEntry {
    pub fn to_net_app(&self) -> cartridge_net::RegistryApp {
        cartridge_net::RegistryApp {
            id: self.id.clone(), name: self.name.clone(), description: self.description.clone(),
            version: self.version.clone(), author: self.author.clone(), category: self.category.clone(),
            tags: self.tags.clone(), repo_url: self.repo_url.clone(), permissions: self.permissions.clone(),
            package: self.package.clone(),
        }
    }
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            registry_url: OFFICIAL_REGISTRY_URL.to_string(),
            auto_refresh: true,
            cache_duration_mins: 60,
            show_processes: false,
            theme_id: default_theme_id(),
            animations_enabled: default_animations_enabled(),
            sounds_enabled: default_sounds_enabled(),
        }
    }
}

#[cfg(test)]
mod connected_app_registry_tests {
    use super::*;
    #[test]
    fn old_catalogue_keeps_new_bundled_apps_and_remote_updates() {
        fn entry(id: &str, version: &str) -> AppEntry {
            serde_json::from_value(serde_json::json!({"id":id,"name":id,"version":version})).unwrap()
        }
        let previous = Registry { version: 1, apps: vec![entry("frequency","1"),entry("weather","1"),entry("uninstalled","1")] };
        let installed = InstalledApps { app_ids: vec!["frequency".into(),"weather".into()] };
        let mut remote = Registry { version:1, apps:vec![entry("weather","2")] };
        remote.retain_installed_from(&previous,&installed);
        assert_eq!(remote.apps.len(),2);
        assert_eq!(remote.apps[0].version,"2");
        assert_eq!(remote.apps[1].id,"frequency");
        remote.retain_installed_from(&previous,&installed);
        assert_eq!(remote.apps.len(),2);
    }
}

#[cfg(test)]
mod store_registry_tests {
    use super::*;
    #[test]
    fn migration_only_changes_retired_official_locations() {
        for url in [OLD_OFFICIAL_REGISTRY_URL, "https://cartridge.dev/registry.json", "https://api.cartridge.dev/catalog.json"] {
            let mut settings = LauncherSettings { registry_url: url.into(), ..Default::default() };
            assert!(settings.migrate_registry_url());
            assert_eq!(settings.registry_url, OFFICIAL_REGISTRY_URL);
        }
        for url in [OFFICIAL_REGISTRY_URL, "https://example.org/custom.json", "https://example.org/cartridge.dev/catalog.json", "https://cartridge.dev.example.org/catalog.json", "https://raw.githubusercontent.com/Strizzo/Cartridge/custom/registry.json"] {
            let mut settings = LauncherSettings { registry_url: url.into(), ..Default::default() };
            assert!(!settings.migrate_registry_url());
            assert_eq!(settings.registry_url, url);
        }
    }

    #[test]
    fn package_survives_verified_mapping_but_local_registry_is_not_installable() {
        let app: AppEntry = serde_json::from_value(serde_json::json!({
            "id":"dev.cartridge.test", "name":"Test", "version":"1.0.0",
            "package":{"url":"https://example.org/test.tar.gz","sha256":"abc","size":42,"min_runtime":"0.6.0"}
        })).unwrap();
        let net = cartridge_net::Registry { version: 1, apps: vec![app.to_net_app()] };
        let mapped = Registry::from_net(&net);
        assert_eq!(mapped.apps[0].package.as_ref().unwrap().size, 42);
        assert_eq!(mapped.apps[0].to_net_app().package.unwrap().min_runtime, "0.6.0");
        let path = std::env::temp_dir().join(format!("cartridge-local-registry-{}.json", std::process::id()));
        std::fs::write(&path, serde_json::to_vec(&mapped).unwrap()).unwrap();
        assert!(Registry::load(&path).unwrap().apps[0].package.is_none());
        std::fs::remove_file(path).unwrap();
    }
}
