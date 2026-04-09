use cartridge_core::atmosphere::Atmosphere;
use cartridge_core::input::InputEvent;
use cartridge_core::screen::Screen;
use cartridge_core::storage::AppStorage;
use cartridge_core::sysinfo::SystemInfo;

use crate::data::{InstalledApps, LauncherSettings, Registry};
use crate::screens::overlay::{BootOverlay, OverlayResult};
use crate::screens::{
    LauncherScreen, ScreenAction, ScreenContext, ScreenId,
    detail::DetailScreen,
    home::HomeScreen,
    settings::SettingsScreen,
    store::StoreScreen,
    wifi::WifiScreen,
};

use std::path::{Path, PathBuf};

/// The main launcher application managing a screen stack and shared state.
pub struct LauncherApp {
    screen_stack: Vec<Box<dyn LauncherScreen>>,
    ctx: ScreenContext,
    overlay: Option<BootOverlay>,
    /// Set when a screen requests launching an app; checked by the main loop.
    pub pending_launch: Option<String>,
}

impl LauncherApp {
    pub fn new(assets_dir: &Path) -> Self {
        let storage = AppStorage::new("cartridge-launcher");

        // Load settings first -- we need registry_url and cache_duration_mins
        let mut settings: LauncherSettings = storage
            .load("settings")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Migrate stale registry URL
        if settings.registry_url.contains("cartridge.dev") {
            settings.registry_url = LauncherSettings::default().registry_url;
            let json = serde_json::to_value(&settings).unwrap_or_default();
            storage.save("settings", &json);
        }

        // Set up network clients
        let cache_dir = home_dir().join(".cartridges/launcher/cache/http");
        let registry_http = cartridge_net::HttpClient::new(cache_dir.clone());
        let registry_client = cartridge_net::RegistryClient::new(
            registry_http,
            settings.registry_url.clone(),
        );

        let installer_http = cartridge_net::HttpClient::new(cache_dir);
        let installer = cartridge_net::AppInstaller::new(installer_http);

        // Try to load registry from network first, fall back to local file
        let registry = load_registry_from_net(&registry_client, assets_dir);

        // Load installed apps from storage, then sync with what's on disk
        let mut installed: InstalledApps = storage
            .load("installed")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Merge filesystem state into the in-memory list
        let on_disk = installer.list_installed();
        if !on_disk.is_empty() {
            log::info!("Found {} apps installed on disk", on_disk.len());
            for id in &on_disk {
                if !installed.is_installed(id) {
                    installed.install(id);
                }
            }
        }

        // Auto-discover bundled cartridges in lua_cartridges/
        discover_bundled_cartridges(&mut installed, &registry);

        // Load recents
        let recents = storage
            .load("recents")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        let mut sysinfo = SystemInfo::new();
        sysinfo.poll();

        let wifi_manager = cartridge_net::WifiManager::new();

        let ctx = ScreenContext {
            registry,
            installed,
            settings,
            recents,
            storage,
            registry_client: Some(registry_client),
            installer: Some(installer),
            sysinfo,
            wifi_manager,
        };

        let home = Box::new(HomeScreen::new()) as Box<dyn LauncherScreen>;

        Self {
            screen_stack: vec![home],
            ctx,
            overlay: None,
            pending_launch: None,
        }
    }

    /// Handle input events. Returns true if the app should quit.
    pub fn handle_input(&mut self, events: &[InputEvent]) -> bool {
        // If overlay is active, route input there
        if let Some(overlay) = &mut self.overlay {
            let result = overlay.handle_input(events);
            match result {
                OverlayResult::Active => return false,
                OverlayResult::Dismiss | OverlayResult::StayCartridge => {
                    self.overlay = None;
                    return false;
                }
                OverlayResult::SwitchToES => {
                    // Write flag file and signal quit
                    let _ = std::fs::write("/tmp/.cartridge_switch_to_es", "1");
                    return true;
                }
            }
        }

        // Route to current screen
        if let Some(current) = self.screen_stack.last_mut() {
            let action = current.handle_input(events, &mut self.ctx);
            match action {
                ScreenAction::None => {}
                ScreenAction::Push(screen_id) => {
                    let new_screen = create_screen(screen_id);
                    self.screen_stack.push(new_screen);
                }
                ScreenAction::Pop => {
                    if self.screen_stack.len() > 1 {
                        self.screen_stack.pop();
                    }
                }
                ScreenAction::ShowOverlay => {
                    self.overlay = Some(BootOverlay::new());
                }
                ScreenAction::LaunchApp(app_id) => {
                    self.pending_launch = Some(app_id);
                    return true;
                }
                ScreenAction::Quit => {
                    return true;
                }
            }
        }

        false
    }

    /// Poll system info (called ~once per second from main loop).
    pub fn poll_sysinfo(&mut self) {
        self.ctx.sysinfo.poll();
    }

    /// Returns the app_id that the user wants to launch, if any.
    pub fn pending_launch(&self) -> Option<&str> {
        self.pending_launch.as_deref()
    }

    /// Render the current screen (and overlay if active).
    pub fn render(&mut self, screen: &mut Screen, atmosphere: &Atmosphere) {
        // Draw atmospheric background instead of flat clear
        atmosphere.draw_background(screen);

        // Render current screen
        if let Some(current) = self.screen_stack.last_mut() {
            current.render(screen, &self.ctx);
        }

        // Render overlay on top if active
        if let Some(overlay) = &self.overlay {
            overlay.render(screen);
        }

        // Draw atmospheric overlays (scanlines, vignette, sweep line) on top
        atmosphere.draw_overlays(screen);
    }
}

fn create_screen(id: ScreenId) -> Box<dyn LauncherScreen> {
    match id {
        ScreenId::Home => Box::new(HomeScreen::new()),
        ScreenId::Store => Box::new(StoreScreen::new()),
        ScreenId::Detail(idx) => Box::new(DetailScreen::new(idx)),
        ScreenId::Settings => Box::new(SettingsScreen::new()),
        ScreenId::WiFi => Box::new(WifiScreen::new()),
    }
}

/// Try to load the registry from the network via RegistryClient, falling
/// back to a local registry.json file on disk if the network is unavailable.
fn load_registry_from_net(
    client: &cartridge_net::RegistryClient,
    assets_dir: &Path,
) -> Registry {
    log::info!("Attempting to fetch registry from network...");
    match client.fetch() {
        Ok(net_reg) => {
            log::info!(
                "Fetched registry v{} with {} apps from network",
                net_reg.version,
                net_reg.apps.len(),
            );
            return Registry::from_net(&net_reg);
        }
        Err(e) => {
            log::warn!("Network registry fetch failed: {e}");
            log::info!("Falling back to local registry file...");
        }
    }

    load_registry_from_file(assets_dir)
}

/// Load registry from a local JSON file on disk.
fn load_registry_from_file(assets_dir: &Path) -> Registry {
    let candidates = [
        assets_dir.join("../registry.json"),
        assets_dir.join("registry.json"),
        std::env::current_dir()
            .unwrap_or_default()
            .join("registry.json"),
    ];

    for path in &candidates {
        if let Ok(canonical) = std::fs::canonicalize(path)
            && canonical.exists() {
                match Registry::load(&canonical) {
                    Ok(reg) => {
                        log::info!("Loaded registry from {}", canonical.display());
                        return reg;
                    }
                    Err(e) => {
                        log::warn!("Failed to load registry from {}: {e}", canonical.display());
                    }
                }
            }
    }

    log::warn!("No registry.json found, using empty registry");
    Registry::empty()
}

/// Resolve the user's home directory.
fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Scan lua_cartridges/ for bundled apps and mark them as installed.
fn discover_bundled_cartridges(installed: &mut InstalledApps, registry: &Registry) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let lua_dir = cwd.join("lua_cartridges");
    let entries = match std::fs::read_dir(&lua_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let json_path = path.join("cartridge.json");
        if !json_path.exists() {
            continue;
        }
        // Read the cartridge.json to get the app_id
        if let Ok(content) = std::fs::read_to_string(&json_path) {
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(id) = meta.get("id").and_then(|v| v.as_str()) {
                    if registry.apps.iter().any(|a| a.id == id) && !installed.is_installed(id) {
                        log::info!("Auto-discovered bundled cartridge: {}", id);
                        installed.install(id);
                    }
                }
            }
        }
    }
}
