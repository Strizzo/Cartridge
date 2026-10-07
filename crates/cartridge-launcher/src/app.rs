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
    system_update::SystemUpdateScreen,
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


        let mut ctx = ScreenContext {
            bundled_app_ids: registry.apps.iter().map(|app| app.id.clone()).collect(),
            registry,
            installed,
            local_apps: Default::default(),
            store_jobs: Default::default(),
            system_update_jobs: Default::default(),
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
        self.ctx.store_jobs.is_busy() || self.ctx.system_update_jobs.is_busy() || self.screen_stack.last().map(|s| s.is_loading()).unwrap_or(false)
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
                    if self.ctx.system_update_jobs.is_staging() {
                        self.ctx.store_jobs.error("Wait for system update staging before opening EmulationStation.");
                        self.overlay = None;
                        return false;
                    }
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

        // R2 pages/dismisses persistent background outcomes on every screen.
        let filtered_events: Vec<_> = events.iter().copied().filter(|event| {
            if event.button == cartridge_core::input::Button::R2 && self.ctx.system_update_jobs.progress.is_none() && self.ctx.store_jobs.progress.is_none() && !self.ctx.store_jobs.notices.is_empty() {
                if event.action == cartridge_core::input::InputAction::Press && self.ctx.store_jobs.can_page_notice() {
                    self.ctx.store_jobs.notice_page += 1;
                    if self.ctx.store_jobs.notice_page >= self.ctx.notice_pages.max(1) {
                        self.ctx.store_jobs.dismiss_notice();
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
                    if self.ctx.system_update_jobs.is_staging() {
                        self.ctx.store_jobs.error("Wait for system update staging before launching an app or game.");
                        return false;
                    }
                    if self.ctx.store_jobs.is_busy() { return false }
                    self.pending_exit = Some(crate::LauncherResult::LaunchGame(game));
                    return true;
                }
                ScreenAction::LaunchApp(app_id) => {
                    if self.ctx.system_update_jobs.is_staging() {
                        self.ctx.store_jobs.error("Wait for system update staging before launching an app or game.");
                        return false;
                    }
                    if self.ctx.store_jobs.is_busy() { return false }
                    if !self.ctx.installed.is_installed(&app_id) { return false }
                    self.pending_launch = Some(app_id);
                    return true;
                }
                ScreenAction::RestartForUpdate => {
                    return self.restart_for_update(self.ctx.sysinfo.battery_percent, self.ctx.sysinfo.battery_charging);
                }
                ScreenAction::Quit => {
                    return true;
                }
            }
        }

        false
    }

    fn restart_for_update(&mut self, battery: i32, charging: bool) -> bool {
        if self.ctx.store_jobs.is_busy() {
            self.ctx.store_jobs.error("Wait for the Store task to finish before restarting CartridgeOS.");
            return false;
        }
        if !self.ctx.system_update_jobs.can_restart() { return false; }
        if !crate::system_update_jobs::power_ready(battery, charging) {
            self.ctx.store_jobs.error("Connect the charger or charge to at least 30% before restarting for the update. An unknown battery level requires charging.");
            return false;
        }
        self.pending_exit = Some(crate::LauncherResult::RestartForUpdate);
        true
    }

    /// Drain pending sysinfo snapshots from the background poller.
    /// Cheap; safe to call every frame. Returns true if data updated
    /// (caller can use this to mark the UI dirty).
    pub fn refresh_sysinfo(&mut self) -> bool {
        let changed = self.ctx.sysinfo.refresh();
        let store_changed = self.ctx.poll_store_jobs();
        let update_changed = self.ctx.poll_system_update_jobs();
        let screen_changed = self.screen_stack.last_mut().map(|s| s.update(&mut self.ctx)).unwrap_or(false);
        changed || screen_changed || store_changed || update_changed
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
        if !self.ctx.has_background_notice() { return }
        let theme = screen.theme;
        let y = if crate::neo::is_neo(theme) { crate::neo::FOOTER_Y - 104 } else { 580 };
        screen.fill(Rect::new(8, y, 704, 100), theme.card_bg);
        screen.draw_outline(Rect::new(8, y, 704, 100), theme.border, 1);
        let update_progress = self.ctx.system_update_jobs.progress.as_deref();
        let title = if update_progress.is_some() { "System Update" } else { jobs.progress.as_deref().unwrap_or("Background result") };
        screen.draw_text(title, 18, y + 7, Some(theme.text), 12, true, Some(680));
        if let Some(progress) = update_progress {
            let lines = crate::screens::system_update::wrap_text(progress, 680, |s| screen.get_text_width(s, 12, false));
            for (index, line) in lines.iter().take(2).enumerate() {
                screen.draw_text(line, 18, y + 26 + index as i32 * 16, Some(theme.text), 12, false, Some(680));
            }
            screen.draw_text("You can browse while this finishes. Settings > System Update for details.",
                18, y + 79, Some(theme.text_dim), 11, false, Some(680));
        } else if jobs.progress.is_some() {
            screen.draw_text("You can continue browsing while this finishes.", 18, y + 31, Some(theme.text_dim), 12, false, None);
        } else if let Some(notice) = jobs.notices.front() {
            let lines = crate::screens::system_update::wrap_text(&notice.message, 680, |s| screen.get_text_width(s, 12, false));
            self.ctx.notice_pages = lines.len().div_ceil(3).max(1);
            let color = if notice.is_error { theme.negative } else { theme.positive };
            for (index, line) in lines.iter().skip(jobs.notice_page * 3).take(3).enumerate() {
                screen.draw_text(line, 18, y + 26 + index as i32 * 16, Some(color), 12, false, Some(680));
            }
            let label = if jobs.notice_page + 1 < self.ctx.notice_pages { "R2 More" } else { "R2 Dismiss" };
            screen.draw_text(label, 18, y + 79, Some(theme.text_dim), 11, false, None);
            self.ctx.store_jobs.mark_notice_rendered();
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
        let _ = std::process::Command::new("systemctl").arg(arg).spawn();
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
        ScreenId::SystemUpdate => Box::new(SystemUpdateScreen::new()),
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
        app.ctx.store_jobs.mark_notice_rendered(); // Unit fixture models the frame before acknowledgement.
        app.handle_input(&[InputEvent {button:Button::R2,action:InputAction::Press}]);
        assert!(app.ctx.store_jobs.notices.is_empty());
        std::fs::remove_dir_all(app.ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn r2_cannot_dismiss_a_new_error_using_an_older_notice_frame() {
        let mut ctx = crate::screens::test_context();
        ctx.store_jobs.notify(crate::store_jobs::Notice { message:"Catalog verified".into(), is_error:false });
        ctx.store_jobs.mark_notice_rendered();
        ctx.notice_pages = 1;
        ctx.store_jobs.error("Update failed; read the new error");
        let mut app = LauncherApp { screen_stack:vec![Box::new(HomeScreen::new())],
            ctx, overlay:None, pending_launch:None, pending_exit:None };
        let r2 = InputEvent {button:Button::R2,action:InputAction::Press};
        app.handle_input(&[r2]);
        assert_eq!(app.ctx.store_jobs.notices.len(), 2);
        assert_eq!(app.ctx.store_jobs.notices.front().unwrap().message, "Update failed; read the new error");
        app.ctx.store_jobs.mark_notice_rendered();
        app.handle_input(&[r2, r2]);
        assert_eq!(app.ctx.store_jobs.notices.len(), 1); // Second press cannot dismiss the next unseen notice.
        assert_eq!(app.ctx.store_jobs.notices.front().unwrap().message, "Catalog verified");
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

#[cfg(test)]
mod system_update_navigation_tests {
    use super::*;
    use cartridge_core::input::{Button, InputAction};
    use crate::system_update_jobs::SystemUpdateJobs;

    fn press(button: Button) -> InputEvent { InputEvent { button, action: InputAction::Press } }
    fn app(ctx: ScreenContext, screen: impl LauncherScreen + 'static) -> LauncherApp {
        LauncherApp { screen_stack: vec![Box::new(HomeScreen::new()), Box::new(screen)],
            ctx, overlay: None, pending_launch: None, pending_exit: None }
    }
    fn cleanup(app: &LauncherApp) {
        std::fs::remove_dir_all(app.ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn system_update_survives_back_and_publishes_failure_without_reopening_screen() {
        let mut ctx = crate::screens::test_context();
        let (jobs, done) = SystemUpdateJobs::blocked_for_test(true);
        ctx.system_update_jobs = jobs;
        let mut app = app(ctx, SystemUpdateScreen::new());
        assert!(app.is_loading());
        assert!(!app.handle_input(&[press(Button::B)]));
        assert_eq!(app.screen_stack.len(), 1);
        for _ in 0..20 {
            app.refresh_sysinfo();
            assert!(app.is_loading());
        }
        done.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while app.is_loading() {
            app.refresh_sysinfo();
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(app.ctx.system_update_jobs.message.contains("Test update failure"));
        assert!(app.ctx.store_jobs.notices.front().unwrap().is_error);
        app.screen_stack.push(Box::new(SystemUpdateScreen::new()));
        app.refresh_sysinfo();
        assert!(!app.is_loading()); // Reopening does not rerun the check or lose the result.
        app.ctx.store_jobs.mark_notice_rendered(); // Unit fixture models the frame before acknowledgement.
        app.handle_input(&[press(Button::R2)]);
        assert!(app.ctx.store_jobs.notices.is_empty());
        cleanup(&app);
    }

    struct LaunchScreen(bool);
    impl LauncherScreen for LaunchScreen {
        fn handle_input(&mut self, _: &[InputEvent], _: &mut ScreenContext) -> ScreenAction {
            if self.0 { ScreenAction::LaunchApp("test.app".into()) }
            else { ScreenAction::LaunchGame(crate::games::GameRequest::default()) }
        }
        fn render(&mut self, _: &mut Screen, _: &ScreenContext) {}
    }

    #[test]
    fn system_update_staging_blocks_app_game_and_store_operations() {
        for launch_app in [true, false] {
            let mut ctx = crate::screens::test_context();
            ctx.installed.install("test.app");
            let (jobs, done) = SystemUpdateJobs::blocked_for_test(true);
            ctx.system_update_jobs = jobs;
            ctx.start_store_job(crate::store_jobs::StoreOperation::Refresh { ttl_seconds: 0 });
            assert!(!ctx.store_jobs.is_busy());
            ctx.start_store_job(crate::store_jobs::StoreOperation::Remove("test.app".into()));
            assert!(!ctx.store_jobs.is_busy());
            let mut app = app(ctx, LaunchScreen(launch_app));
            assert!(!app.handle_input(&[press(Button::A)]));
            assert!(app.pending_launch.is_none());
            assert!(app.pending_exit.is_none());
            done.send(()).unwrap();
            cleanup(&app);
        }
    }

    #[test]
    fn system_update_check_does_not_block_launches() {
        let mut ctx = crate::screens::test_context();
        ctx.installed.install("test.app");
        let (jobs, done) = SystemUpdateJobs::blocked_for_test(false);
        ctx.system_update_jobs = jobs;
        let mut app = app(ctx, LaunchScreen(true));
        assert!(app.handle_input(&[press(Button::A)]));
        assert_eq!(app.pending_launch(), Some("test.app"));
        done.send(()).unwrap();
        cleanup(&app);
    }

    #[test]
    fn system_update_busy_store_prevents_staging_and_restart() {
        let mut ctx = crate::screens::test_context();
        ctx.system_update_jobs = SystemUpdateJobs::ready_for_test();
        let (tx, rx) = std::sync::mpsc::channel();
        ctx.store_jobs.simulate(rx);
        let release = std::sync::Arc::clone(ctx.system_update_jobs.release.as_ref().unwrap());
        assert!(!ctx.stage_system_update(release));
        assert!(!ctx.system_update_jobs.is_busy());
        ctx.system_update_jobs.status.as_mut().unwrap().pending = Some("0.6.2-0123456789ab".into());
        let mut app = app(ctx, SystemUpdateScreen::new());
        assert!(!app.restart_for_update(80, false));
        assert!(app.pending_exit.is_none());
        tx.send(crate::store_jobs::Completion { local: Default::default(), installer: None,
            registry: None, outcome: Ok(None) }).unwrap();
        cleanup(&app);
    }

    #[test]
    fn system_update_restart_requires_pending_release_and_current_safe_power() {
        let mut ctx = crate::screens::test_context();
        ctx.system_update_jobs = SystemUpdateJobs::ready_for_test();
        let mut app = app(ctx, SystemUpdateScreen::new());
        assert!(!app.restart_for_update(80, false));
        app.ctx.system_update_jobs.status.as_mut().unwrap().pending = Some("0.6.2-0123456789ab".into());
        for battery in [-1, 0, 29, 101] {
            assert!(!app.restart_for_update(battery, false));
            assert!(app.pending_exit.is_none());
        }
        for (battery, charging) in [(30, false), (100, false), (-1, true), (5, true)] {
            assert!(app.restart_for_update(battery, charging));
            assert!(matches!(app.pending_exit.take(), Some(crate::LauncherResult::RestartForUpdate)));
        }
        // AsyncSystemInfo polls the host/simulator on construction. Route using that
        // current reading; explicit low/unknown/charging cases are exercised above.
        let ready = app.ctx.system_update_power_ready();
        assert_eq!(app.handle_input(&[press(Button::A)]), ready);
        assert_eq!(matches!(app.pending_exit, Some(crate::LauncherResult::RestartForUpdate)), ready);
        cleanup(&app);
    }

    #[test]
    fn system_update_power_menu_stays_responsive_and_blocks_emulationstation_during_stage() {
        let mut ctx = crate::screens::test_context();
        let (jobs, done) = SystemUpdateJobs::blocked_for_test(true);
        ctx.system_update_jobs = jobs;
        let mut app = app(ctx, SystemUpdateScreen::new());
        assert!(!app.handle_input(&[press(Button::Select)]));
        assert!(app.overlay.is_some());
        app.handle_input(&[press(Button::DpadUp)]);
        assert!(!app.handle_input(&[press(Button::A)]));
        assert!(app.overlay.is_none());
        assert!(app.pending_exit.is_none());
        app.handle_input(&[press(Button::Select)]);
        app.handle_input(&[press(Button::B)]);
        assert!(app.overlay.is_none());
        assert!(app.is_loading());
        done.send(()).unwrap();
        cleanup(&app);
    }
}

#[cfg(test)]
mod system_update_visual_tests {
    use super::*;
    use cartridge_core::font::FontCache;
    use cartridge_core::image_cache::ImageCache;
    use cartridge_core::text_cache::TextCache;
    use cartridge_core::theme::Theme;
    use cartridge_core::input::{Button, InputAction};

    /// Test-only state: no production fixture switch, network access, or trust bypass.
    #[test]
    #[ignore = "visual QA: run with SDL_VIDEODRIVER=dummy and --ignored --nocapture"]
    fn system_update_visual_fixtures() {
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();
        let window = video.window("System Update visual fixtures", 720, 720).hidden().build().unwrap();
        let mut canvas = window.into_canvas().software().build().unwrap();
        let creator = canvas.texture_creator();
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let output = std::env::temp_dir().join(format!("cartridge-update-visual-{}", std::process::id()));
        std::fs::create_dir_all(&output).unwrap();
        for theme_id in ["neo", "midnight"] {
            let mut theme = Theme::by_id(theme_id);
            // The dummy video driver has no alpha render-target support. Inspect
            // static layout without its opaque atmospheric overlay texture.
            theme.atmosphere = false;
            let mut fonts = FontCache::new(&assets).unwrap();
            fonts.set_family(theme.font_regular, theme.font_bold);
            fonts.set_display(theme.font_display);
            let mut images = ImageCache::new(&creator).unwrap();
            let mut text_cache = TextCache::new(&creator);
            let mut atmosphere = Atmosphere::new();
            atmosphere.precompose(&mut canvas, &creator, &mut images, &theme);
            for fixture in ["available", "confirmation", "staged", "long_error", "downloading"] {
                let mut ctx = crate::screens::test_context();
                ctx.settings.theme_id = theme_id.into();
                ctx.settings.animations_enabled = false;
                ctx.system_update_jobs = crate::system_update_jobs::SystemUpdateJobs::ready_for_test();
                ctx.system_update_jobs.message = "A system update is available. Press A to review the download confirmation.".into();
                std::sync::Arc::get_mut(ctx.system_update_jobs.release.as_mut().unwrap()).unwrap().notes =
                    "A smoother CartridgeOS experience.\n\n- Keeps your apps, saves and settings.\n- Improves launcher navigation and startup recovery.\n- Verifies every system file before the next restart.\n\nRelease notes remain scrollable while a background result is visible.".into();
                let mut done = None;
                if fixture == "staged" {
                    ctx.system_update_jobs.release = None;
                    ctx.system_update_jobs.status.as_mut().unwrap().pending = Some("0.6.2-0123456789ab".into());
                    ctx.system_update_jobs.message = "System update staged. Press A to restart CartridgeOS and apply it.".into();
                } else if fixture == "long_error" {
                    ctx.system_update_jobs.release = None;
                    ctx.system_update_jobs.is_error = true;
                    ctx.system_update_jobs.message = format!("Download failed: {}\nYour current installation is unchanged. Press A to retry.", "long-error-path/".repeat(25));
                } else if fixture == "downloading" {
                    let (mut jobs, finish) = crate::system_update_jobs::SystemUpdateJobs::blocked_for_test(true);
                    jobs.message = "Downloading CartridgeOS 0.6.2: 12.5 MiB of 48.0 MiB".into();
                    jobs.progress = Some(jobs.message.clone());
                    ctx.system_update_jobs = jobs;
                    done = Some(finish);
                }
                ctx.store_jobs.notices.push_back(crate::store_jobs::Notice {
                    message: "System Update: a persistent result remains available after navigating away. R2 pages through long errors.".into(),
                    is_error: false,
                });
                let scratch = ctx.storage.data_dir.parent().unwrap().parent().unwrap().to_path_buf();
                let mut app = LauncherApp { screen_stack: vec![Box::new(SystemUpdateScreen::new())],
                    ctx, overlay: None, pending_launch: None, pending_exit: None };
                if fixture == "confirmation" {
                    app.handle_input(&[InputEvent { button: Button::A, action: InputAction::Press }]);
                }
                for capture in 0..2 {
                    if capture == 1 {
                        for _ in 0..40 {
                            app.handle_input(&[InputEvent { button: Button::DpadDown, action: InputAction::Press }]);
                        }
                    }
                    let mut screen = Screen { canvas: &mut canvas, theme: &theme, fonts: &mut fonts,
                        images: &mut images, text_cache: &mut text_cache, texture_creator: &creator };
                    app.render(&mut screen, &atmosphere);
                    crate::capture_frame_to_png(&canvas, &output.join(format!("{theme_id}-{fixture}-{capture}.png"))).unwrap();
                    canvas.present();
                }
                if let Some(done) = done { done.send(()).unwrap(); }
                std::fs::remove_dir_all(scratch).unwrap();
            }
        }
        println!("System Update fixtures: {}", output.display());
    }
}

#[cfg(test)]
mod store_feedback_visual_tests {
    use super::*;
    use cartridge_core::{font::FontCache, image_cache::ImageCache,
        text_cache::TextCache, theme::Theme};

    #[test]
    #[ignore = "visual QA: SDL_VIDEODRIVER=dummy, --ignored --nocapture"]
    fn store_update_feedback_visual_fixture() {
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();
        let window = video.window("Store update feedback", 720, 720).hidden().build().unwrap();
        let mut canvas = window.into_canvas().software().build().unwrap();
        let creator = canvas.texture_creator();
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let output = std::env::temp_dir().join(format!("cartridge-store-feedback-visual-{}", std::process::id()));
        std::fs::create_dir_all(&output).unwrap();
        let mut theme = Theme::by_id("neo");
        theme.atmosphere = false;
        let mut fonts = FontCache::new(&assets).unwrap();
        fonts.set_family(theme.font_regular, theme.font_bold);
        fonts.set_display(theme.font_display);
        let mut images = ImageCache::new(&creator).unwrap();
        let mut text_cache = TextCache::new(&creator);
        let atmosphere = Atmosphere::new();
        let mut ctx = crate::screens::test_context();
        ctx.settings.animations_enabled = false;
        let mut remote: crate::data::AppEntry = serde_json::from_value(serde_json::json!({
            "id":"dev.cartridge.frequency", "name":"Frequency", "version":"1.2.0",
            "description":"An atlas of live radio. Explore the world, collect stations, and listen.",
            "category":"media", "permissions":["network","audio","storage"],
            "repo_url":"https://github.com/Strizzo/frequency-cartridge"
        })).unwrap();
        let mut installed = remote.clone();
        installed.version = "1.0.0".into();
        ctx.local_apps.apps.push(installed);
        ctx.installed.install(&remote.id);
        // Future-runtime fixture tests the actual preflight without any download.
        remote.package = Some(cartridge_net::AppPackage {
            url:"https://example.org/must-not-download.tgz".into(), sha256:"aa".repeat(32),
            size:1, min_runtime:"999.0.0".into(),
        });
        ctx.registry.apps.push(remote.clone());
        ctx.store_jobs.notify(crate::store_jobs::Notice { message:"Catalog verified: 3 apps".into(), is_error:false });
        ctx.start_store_job(crate::store_jobs::StoreOperation::Update(remote.clone()));
        assert!(!ctx.store_jobs.is_busy());
        let scratch = ctx.storage.data_dir.parent().unwrap().parent().unwrap().to_path_buf();
        let mut app = LauncherApp { screen_stack:vec![Box::new(DetailScreen::new(remote.id))],
            ctx, overlay:None, pending_launch:None, pending_exit:None };
        for capture in 0..2 {
            let mut screen = Screen { canvas:&mut canvas, theme:&theme, fonts:&mut fonts,
                images:&mut images, text_cache:&mut text_cache, texture_creator:&creator };
            app.render(&mut screen, &atmosphere);
            crate::capture_frame_to_png(&canvas, &output.join(format!("error-{capture}.png"))).unwrap();
            canvas.present();
            assert!(app.ctx.store_jobs.notices.front().unwrap().is_error);
        }
        assert_eq!(std::fs::read(output.join("error-0.png")).unwrap(),
            std::fs::read(output.join("error-1.png")).unwrap());
        std::fs::remove_dir_all(scratch).unwrap();
        println!("Store feedback fixtures: {}", output.display());
    }
}
