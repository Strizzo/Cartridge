use cartridge_core::atmosphere::Atmosphere;
use cartridge_core::input::InputEvent;
use cartridge_core::screen::Screen;
use cartridge_core::storage::AppStorage;
use cartridge_core::sysinfo::AsyncSystemInfo;

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
    pub pending_exit: Option<crate::LauncherResult>,
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
        if settings.migrate_registry_url() {
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



        // Startup must work offline and present immediately. Store refresh is
        // explicit; the bundled registry already describes installed apps.
        let registry = load_registry_from_file(assets_dir);

        // Disk scanning and installed-manifest reconciliation run in a shared
        // background job. Startup renders immediately, even when offline.
        let installed = InstalledApps::default();

        // Load recents
        let recents = storage
            .load("recents")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Poll system info on a background thread (every 2s) so nmcli/ps/df
        // forks don't stall the render thread. AsyncSystemInfo Derefs to SystemInfo.
        let sysinfo = AsyncSystemInfo::new(std::time::Duration::from_secs(2));

        let wifi_manager = cartridge_net::WifiManager::new();

        let mut ctx = ScreenContext {
            bundled_app_ids: registry.apps.iter().map(|app| app.id.clone()).collect(),
            registry,
            installed,
            local_apps: Default::default(),
            store_jobs: Default::default(),
            registry_revision: 0,
            invalidated_textures: Vec::new(),
            notice_pages: 0,
            automatic_store_refresh: true,
            settings,
            recents,
            storage,
            registry_client: Some(registry_client),
            installer: None,
            sysinfo,
            wifi_manager,
        };

        ctx.sync_installed_from_disk();

        let home = Box::new(HomeScreen::new()) as Box<dyn LauncherScreen>;

        Self {
            screen_stack: vec![home],
            ctx,
            overlay: None,
            pending_launch: None,
            pending_exit: None,
        }
    }

    pub(crate) fn set_automatic_store_refresh(&mut self, enabled: bool) {
        self.ctx.automatic_store_refresh = enabled;
    }

    pub(crate) fn is_starting(&self) -> bool {
        self.ctx.registry_revision == 0 && self.ctx.store_jobs.is_busy()
    }

    pub fn is_loading(&self) -> bool {
        self.ctx.store_jobs.is_busy() || self.screen_stack.last().map(|s| s.is_loading()).unwrap_or(false)
    }

    pub fn show_games(&mut self, resume: Option<crate::games::GameRequest>) {
        self.screen_stack.push(Box::new(crate::screens::games::GamesScreen::new(resume)));
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
                    self.pending_exit = Some(crate::LauncherResult::EmulationStation);
                    return true;
                }
                OverlayResult::Reboot => {
                    request_power_action(PowerAction::Reboot);
                    self.pending_exit = Some(crate::LauncherResult::PowerRequested);
                    return true;
                }
                OverlayResult::Shutdown => {
                    request_power_action(PowerAction::Shutdown);
                    self.pending_exit = Some(crate::LauncherResult::PowerRequested);
                    return true;
                }
            }
        }

        // R2 pages/dismisses persistent Store outcomes on every screen.
        let filtered_events: Vec<_> = events.iter().copied().filter(|event| {
            if event.button == cartridge_core::input::Button::R2 && !self.ctx.store_jobs.notices.is_empty() {
                if event.action == cartridge_core::input::InputAction::Press {
                    self.ctx.store_jobs.notice_page += 1;
                    if self.ctx.store_jobs.notice_page >= self.ctx.notice_pages.max(1) {
                        self.ctx.store_jobs.notices.pop_front();
                        self.ctx.store_jobs.notice_page = 0;
                    }
                }
                false
            } else { true }
        }).collect();

        // Route to current screen
        if let Some(current) = self.screen_stack.last_mut() {
            let action = current.handle_input(&filtered_events, &mut self.ctx);
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
                ScreenAction::LaunchGame(game) => {
                    if self.ctx.store_jobs.is_busy() { return false }
                    self.pending_exit = Some(crate::LauncherResult::LaunchGame(game));
                    return true;
                }
                ScreenAction::LaunchApp(app_id) => {
                    if self.ctx.store_jobs.is_busy() { return false }
                    if !self.ctx.installed.is_installed(&app_id) { return false }
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

    /// Drain pending sysinfo snapshots from the background poller.
    /// Cheap; safe to call every frame. Returns true if data updated
    /// (caller can use this to mark the UI dirty).
    pub fn refresh_sysinfo(&mut self) -> bool {
        let changed = self.ctx.sysinfo.refresh();
        let store_changed = self.ctx.poll_store_jobs();
        let screen_changed = self.screen_stack.last_mut().map(|s| s.update(&mut self.ctx)).unwrap_or(false);
        changed || screen_changed || store_changed
    }

    /// Read the user's currently selected theme id.
    pub fn theme_id(&self) -> &str {
        &self.ctx.settings.theme_id
    }

    /// Whether the user has animated theme overlays enabled.
    pub fn animations_enabled(&self) -> bool {
        self.ctx.settings.animations_enabled
    }

    /// Whether the user has launcher sound feedback enabled.
    pub fn sounds_enabled(&self) -> bool {
        self.ctx.settings.sounds_enabled
    }

    /// Returns the app_id that the user wants to launch, if any.
    pub fn pending_launch(&self) -> Option<&str> {
        self.pending_launch.as_deref()
    }

    /// Render the current screen (and overlay if active).
    pub fn render(&mut self, screen: &mut Screen, atmosphere: &Atmosphere) {
        let animations_enabled = self.ctx.settings.animations_enabled;
        for path in self.ctx.invalidated_textures.drain(..) { screen.images.remove(&path); }
        // Draw atmospheric background instead of flat clear
        atmosphere.draw_background(screen);

        // Render current screen
        if let Some(current) = self.screen_stack.last_mut() {
            current.render(screen, &self.ctx);
        }

        self.render_store_status(screen);

        // Render overlay on top if active
        if let Some(overlay) = &self.overlay {
            overlay.render(screen);
        }

        // Draw atmospheric overlays (scanlines, vignette) on top
        atmosphere.draw_overlays(screen);

        // Animated effects (theme-dependent, gated by setting).
        // Drawn last so the sweep line sits above scanlines/vignette.
        atmosphere.draw_animated(screen, animations_enabled);
    }

    fn render_store_status(&mut self, screen: &mut Screen) {
        use sdl2::rect::Rect;
        let jobs = &self.ctx.store_jobs;
        if jobs.progress.is_none() && jobs.notices.is_empty() { return }
        let theme = screen.theme;
        let y = if crate::neo::is_neo(theme) { crate::neo::FOOTER_Y - 104 } else { 580 };
        screen.fill(Rect::new(8, y, 704, 100), theme.card_bg);
        screen.draw_outline(Rect::new(8, y, 704, 100), theme.border, 1);
        let title = jobs.progress.as_deref().unwrap_or("Store result");
        screen.draw_text(title, 18, y + 7, Some(theme.text), 12, true, Some(680));
        if let Some(notice) = jobs.notices.front() {
            let lines = crate::neo::wrap_lines(screen, &notice.message, 12, false, 680, usize::MAX);
            self.ctx.notice_pages = lines.len().div_ceil(3).max(1);
            let color = if notice.is_error { theme.negative } else { theme.positive };
            for (index, line) in lines.iter().skip(jobs.notice_page * 3).take(3).enumerate() {
                screen.draw_text(line, 18, y + 26 + index as i32 * 16, Some(color), 12, false, Some(680));
            }
            let label = if jobs.notice_page + 1 < self.ctx.notice_pages { "R2 More" } else { "R2 Dismiss" };
            screen.draw_text(label, 18, y + 79, Some(theme.text_dim), 11, false, None);
        } else {
            screen.draw_text("You can continue browsing while this finishes.", 18, y + 31, Some(theme.text_dim), 12, false, None);
        }
    }

}

/// Power actions that can be triggered from the BootOverlay.
#[derive(Clone, Copy)]
enum PowerAction {
    Reboot,
    Shutdown,
}

/// Execute a power action. On Linux this calls `systemctl reboot|poweroff`.
/// On other platforms (macOS dev) it just logs and exits cleanly so the
/// developer can iterate without rebooting their workstation.
fn request_power_action(action: PowerAction) {
    if cartridge_core::sim::is_sim() {
        log::info!("Simulator power action requested; host unchanged");
        return;
    }
    #[cfg(target_os = "linux")]
    {
        let arg = match action {
            PowerAction::Reboot => "reboot",
            PowerAction::Shutdown => "poweroff",
        };
        log::info!("Power action: systemctl {arg}");
        let _ = std::process::Command::new("systemctl").arg(arg).status();
    }
    #[cfg(not(target_os = "linux"))]
    {
        let label = match action {
            PowerAction::Reboot => "reboot",
            PowerAction::Shutdown => "shutdown",
        };
        log::warn!("Power action {label} requested -- ignored on non-Linux dev build");
    }
}

fn create_screen(id: ScreenId) -> Box<dyn LauncherScreen> {
    match id {
        ScreenId::Home => Box::new(HomeScreen::new()),
        ScreenId::Games => Box::new(crate::screens::games::GamesScreen::new(None)),
        ScreenId::Store => Box::new(StoreScreen::new()),
        ScreenId::Detail(id) => Box::new(DetailScreen::new(id)),
        ScreenId::Settings => Box::new(SettingsScreen::new()),
        ScreenId::WiFi => Box::new(WifiScreen::new()),
    }
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
    cartridge_core::paths::home_dir()
}


#[cfg(test)]
mod store_navigation_tests {
    use super::*;
    use crate::data::AppEntry;
    use crate::store_jobs::{Completion, LocalApps};
    use cartridge_core::input::{Button, InputAction};

    #[test]
    fn failed_job_completes_after_detail_is_popped_without_hiding_installed_app() {
        let entry: AppEntry = serde_json::from_value(serde_json::json!({"id":"dev.cartridge.test","name":"Test","version":"1.0.0"})).unwrap();
        let mut ctx = crate::screens::test_context();
        ctx.registry.apps.push(entry.clone());
        ctx.local_apps.apps.push(entry.clone());
        ctx.installed.install(&entry.id);
        let (tx, rx) = std::sync::mpsc::channel();
        ctx.store_jobs.simulate(rx);
        let mut app = LauncherApp { screen_stack:vec![Box::new(HomeScreen::new()),Box::new(DetailScreen::new(entry.id.clone()))],
            ctx, overlay:None, pending_launch:None, pending_exit:None };
        assert!(app.is_loading());
        app.handle_input(&[InputEvent {button:Button::B,action:InputAction::Press}]);
        assert_eq!(app.screen_stack.len(),1);
        assert!(app.is_loading());
        tx.send(Completion { local:LocalApps {apps:vec![entry.clone()],overrides:[entry.id.clone()].into(),..Default::default()},
            installer:None,registry:None,outcome:Err("Remove failed: permission denied".into()) }).unwrap();
        let deadline = std::time::Instant::now()+std::time::Duration::from_secs(2);
        while app.is_loading() {
            app.refresh_sysinfo();
            assert!(std::time::Instant::now()<deadline);
            std::thread::yield_now();
        }
        assert!(app.ctx.installed.is_installed(&entry.id));
        assert!(app.ctx.has_override(&entry.id));
        assert_eq!(app.ctx.store_jobs.notices.front().unwrap().message,"Remove failed: permission denied");
        assert!(app.ctx.store_jobs.notices.front().unwrap().is_error);
        app.handle_input(&[InputEvent {button:Button::R2,action:InputAction::Press}]);
        assert!(app.ctx.store_jobs.notices.is_empty());
        std::fs::remove_dir_all(app.ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn snapshot_startup_waits_for_published_sync_even_without_explicit_wait_flag() {
        let entry: AppEntry = serde_json::from_value(serde_json::json!({"id":"dev.cartridge.test","name":"Test","version":"1.0.0"})).unwrap();
        let mut ctx = crate::screens::test_context();
        ctx.registry.apps.push(entry.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        ctx.store_jobs.simulate(rx);
        let mut app = LauncherApp { screen_stack:vec![Box::new(HomeScreen::new())], ctx,
            overlay:None, pending_launch:None, pending_exit:None };
        let config = crate::LauncherConfig { max_frames:Some(40), capture_frames:vec![0,30], ..Default::default() };
        assert!(!config.script_wait_for_background);
        // A deliberately blocked Sync cannot advance scripts or frame captures.
        for _ in 0..20 {
            app.refresh_sysinfo();
            assert!(config.waits_for_background(&app));
            assert!(app.ctx.installed_apps().is_empty());
        }
        tx.send(Completion {local:LocalApps {apps:vec![entry.clone()],..Default::default()},
            installer:None,registry:None,outcome:Ok(None)}).unwrap();
        let deadline = std::time::Instant::now()+std::time::Duration::from_secs(2);
        while config.waits_for_background(&app) {
            app.refresh_sysinfo();
            assert!(std::time::Instant::now()<deadline);
            std::thread::yield_now();
        }
        assert!(app.ctx.installed.is_installed(&entry.id));
        assert!(!app.is_starting());
        assert!(app.ctx.store_jobs.notices.is_empty());
        std::fs::remove_dir_all(app.ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }

}
