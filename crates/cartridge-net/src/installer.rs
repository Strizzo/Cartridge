use std::fs;
use std::path::PathBuf;

use crate::client::HttpClient;
use crate::registry::RegistryApp;

/// Installs and manages Cartridge apps on the local filesystem.
///
/// Apps are stored under `~/.cartridges/apps/{app_id}/`.
pub struct AppInstaller {
    http: HttpClient,
    install_dir: PathBuf,
}

impl AppInstaller {
    /// Create a new installer. Apps will be placed under the default install
    /// directory (`~/.cartridges/apps`).
    pub fn new(http: HttpClient) -> Self {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));
        let install_dir = home.join(".cartridges").join("apps");
        Self { http, install_dir }
    }

    /// Install an app by downloading its release archive from the repo URL.
    ///
    /// The convention is: `{repo_url}/releases/download/v{version}/{app_id}.tar.gz`
    pub fn install(&self, app: &RegistryApp) -> Result<(), String> {
        let app_dir = self.app_path(&app.id);
        fs::create_dir_all(&app_dir)
            .map_err(|e| format!("Failed to create app directory {}: {e}", app_dir.display()))?;

        // Download the release tarball.
        let archive_url = format!(
            "{}/releases/download/v{}/{}.tar.gz",
            app.repo_url.trim_end_matches('/'),
            app.version,
            app.id,
        );
        let archive_path = app_dir.join("archive.tar.gz");
        self.http.download(&archive_url, &archive_path)?;

        // Write app metadata so we can list / query installed apps later.
        let meta = serde_json::json!({
            "id": app.id,
            "name": app.name,
            "version": app.version,
            "author": app.author,
            "category": app.category,
            "repo_url": app.repo_url,
        });
        let meta_path = app_dir.join("cartridge.json");
        let meta_json = serde_json::to_string_pretty(&meta)
            .map_err(|e| format!("Failed to serialize metadata: {e}"))?;
        fs::write(&meta_path, meta_json)
            .map_err(|e| format!("Failed to write metadata {}: {e}", meta_path.display()))?;

        log::info!("Installed {} v{}", app.id, app.version);
        Ok(())
    }

    /// Remove an installed app by deleting its directory.
    pub fn remove(&self, app_id: &str) -> Result<(), String> {
        let app_dir = self.app_path(app_id);
        if !app_dir.exists() {
            return Err(format!("App {app_id} is not installed"));
        }
        fs::remove_dir_all(&app_dir)
            .map_err(|e| format!("Failed to remove {}: {e}", app_dir.display()))?;
        log::info!("Removed {app_id}");
        Ok(())
    }

    /// Check whether an app is installed locally.
    pub fn is_installed(&self, app_id: &str) -> bool {
        self.app_path(app_id).join("cartridge.json").exists()
    }

    /// List the IDs of all installed apps.
    pub fn list_installed(&self) -> Vec<String> {
        let mut ids = Vec::new();
        let entries = match fs::read_dir(&self.install_dir) {
            Ok(e) => e,
            Err(_) => return ids,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("cartridge.json").exists()
                && let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    ids.push(name.to_string());
                }
        }
        ids.sort();
        ids
    }

    /// Return the on-disk path for a given app.
    pub fn app_path(&self, app_id: &str) -> PathBuf {
        self.install_dir.join(app_id)
    }
}
