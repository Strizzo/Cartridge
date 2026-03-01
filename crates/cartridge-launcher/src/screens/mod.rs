pub mod home;
pub mod store;
pub mod detail;
pub mod settings;
pub mod overlay;
pub mod wifi;

use cartridge_core::input::InputEvent;
use cartridge_core::screen::Screen;
use cartridge_core::sysinfo::SystemInfo;

/// Result of handling input on a screen.
pub enum ScreenAction {
    /// Stay on current screen, no navigation change.
    None,
    /// Push a new screen onto the stack.
    Push(ScreenId),
    /// Pop the current screen (go back).
    Pop,
    /// Show the boot selector overlay.
    ShowOverlay,
    /// Quit the application.
    Quit,
    /// Launch an installed app by its id.
    LaunchApp(String),
}

/// Identifies which screen to push.
pub enum ScreenId {
    Home,
    Store,
    Detail(usize), // index into registry apps
    Settings,
    WiFi,
}

/// Common trait for all launcher screens.
pub trait LauncherScreen {
    fn handle_input(&mut self, events: &[InputEvent], ctx: &mut ScreenContext) -> ScreenAction;
    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext);
}

/// Shared context passed to all screens.
pub struct ScreenContext {
    pub registry: crate::data::Registry,
    pub installed: crate::data::InstalledApps,
    pub settings: crate::data::LauncherSettings,
    pub recents: Vec<crate::data::RecentEntry>,
    pub storage: cartridge_core::storage::AppStorage,
    pub registry_client: Option<cartridge_net::RegistryClient>,
    pub installer: Option<cartridge_net::AppInstaller>,
    pub sysinfo: SystemInfo,
    pub wifi_manager: cartridge_net::WifiManager,
}

impl ScreenContext {
    pub fn save_installed(&self) {
        let json = serde_json::to_value(&self.installed).unwrap_or_default();
        self.storage.save("installed", &json);
    }

    pub fn save_settings(&self) {
        let json = serde_json::to_value(&self.settings).unwrap_or_default();
        self.storage.save("settings", &json);
    }

    pub fn save_recents(&self) {
        let json = serde_json::to_value(&self.recents).unwrap_or_default();
        self.storage.save("recents", &json);
    }

    /// Get installed apps as AppEntry references from registry.
    pub fn installed_apps(&self) -> Vec<&crate::data::AppEntry> {
        self.registry
            .apps
            .iter()
            .filter(|app| self.installed.is_installed(&app.id))
            .collect()
    }

    /// Refresh the registry from the network. Falls back to the current
    /// registry if the fetch fails.
    pub fn refresh_registry(&mut self) {
        if let Some(client) = &self.registry_client {
            log::info!("Refreshing registry from network...");
            match client.fetch() {
                Ok(net_reg) => {
                    log::info!(
                        "Fetched registry v{} with {} apps",
                        net_reg.version,
                        net_reg.apps.len()
                    );
                    self.registry = crate::data::Registry::from_net(&net_reg);
                }
                Err(e) => {
                    log::warn!("Failed to fetch registry from network: {e}");
                }
            }
        } else {
            log::debug!("No registry client configured, skipping network refresh");
        }
    }

    /// Synchronize the in-memory installed apps list with what is actually
    /// present on disk (checked via AppInstaller).
    pub fn sync_installed_from_disk(&mut self) {
        if let Some(installer) = &self.installer {
            let on_disk = installer.list_installed();
            if on_disk != self.installed.app_ids {
                log::info!(
                    "Syncing installed apps from disk: {} apps found",
                    on_disk.len()
                );
                self.installed.app_ids = on_disk;
                self.save_installed();
            }
        }
    }
}
