pub mod home;
pub mod games;
pub mod store;
pub mod detail;
pub mod settings;
pub mod overlay;
pub mod wifi;

use cartridge_core::input::InputEvent;
use cartridge_core::screen::Screen;
use cartridge_core::sysinfo::AsyncSystemInfo;
use crate::store_jobs::{LocalApps, StoreJobs, StoreOperation, is_newer};

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
    LaunchGame(crate::games::GameRequest),
}

/// Identifies which screen to push.
pub enum ScreenId {
    Home,
    Games,
    Store,
    Detail(String), // stable app ID, independent of catalog ordering
    Settings,
    WiFi,
}

/// Common trait for all launcher screens.
pub trait LauncherScreen {
    /// Poll background work; true requests a redraw. Must never block.
    fn update(&mut self, _ctx: &mut ScreenContext) -> bool { false }
    fn is_loading(&self) -> bool { false }
    fn handle_input(&mut self, events: &[InputEvent], ctx: &mut ScreenContext) -> ScreenAction;
    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext);
}

/// Shared context passed to all screens.
pub struct ScreenContext {
    pub registry: crate::data::Registry,
    pub(crate) bundled_app_ids: Vec<String>,
    pub installed: crate::data::InstalledApps,
    pub(crate) local_apps: LocalApps,
    pub(crate) store_jobs: StoreJobs,
    pub registry_revision: u64,
    pub(crate) invalidated_textures: Vec<String>,
    pub(crate) notice_pages: usize,
    pub(crate) automatic_store_refresh: bool,
    pub settings: crate::data::LauncherSettings,
    pub recents: Vec<crate::data::RecentEntry>,
    pub storage: cartridge_core::storage::AppStorage,
    pub registry_client: Option<cartridge_net::RegistryClient>,
    pub installer: Option<cartridge_net::AppInstaller>,
    pub sysinfo: AsyncSystemInfo,
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

    /// Installed metadata comes from the same manifest that launch resolves.
    pub fn installed_apps(&self) -> Vec<&crate::data::AppEntry> {
        let mut apps: Vec<_> = self.registry.apps.iter().filter_map(|catalog| {
            self.local_apps.apps.iter().find(|local| local.id == catalog.id)
        }).collect();
        // Custom installations missing from the catalog remain discoverable.
        apps.extend(self.local_apps.apps.iter().filter(|local| !self.registry.apps.iter().any(|app| app.id == local.id)));
        apps
    }

    pub fn app(&self, id: &str) -> Option<&crate::data::AppEntry> {
        self.registry.apps.iter().find(|a| a.id == id)
            .or_else(|| self.local_apps.apps.iter().find(|a| a.id == id))
    }

    pub fn installed_version(&self, id: &str) -> Option<&str> {
        self.local_apps.apps.iter().find(|a| a.id == id).map(|a| a.version.as_str())
    }

    pub fn has_update(&self, id: &str) -> bool {
        match (self.app(id), self.installed_version(id)) {
            (Some(remote), Some(current)) if remote.package.is_some() => is_newer(&remote.version, current),
            _ => false,
        }
    }

    pub fn has_override(&self, id: &str) -> bool { self.local_apps.overrides.contains(id) }
    pub fn can_rollback(&self, id: &str) -> bool { self.local_apps.rollbacks.contains(id) }

    /// Explicit Refresh always checks online; opening Store can reuse its cache.
    pub fn refresh_registry(&mut self) { self.start_store_job(StoreOperation::Refresh { ttl_seconds: 0 }); }
    pub fn refresh_registry_cached(&mut self) {
        let ttl_seconds = u64::from(self.settings.cache_duration_mins.min(360)) * 60;
        self.start_store_job(StoreOperation::Refresh { ttl_seconds });
    }
    pub fn sync_installed_from_disk(&mut self) { self.start_store_job(StoreOperation::Sync); }

    pub(crate) fn start_store_job(&mut self, operation: StoreOperation) {
        if self.store_jobs.is_busy() { return }
        match &operation {
            StoreOperation::Install(app) | StoreOperation::Update(app) if app.package.is_none() => {
                self.store_jobs.error("This app has no verified package. Local and bundled copies remain launchable.");
                return;
            }
            StoreOperation::Update(app) if !self.has_update(&app.id) => return,
            StoreOperation::Remove(id) if !self.has_override(id) => {
                self.store_jobs.error("This cartridge is bundled. Only a Store-installed override can be removed.");
                return;
            }
            StoreOperation::Rollback(id) if !self.can_rollback(id) => return,
            _ => {}
        }
        // Move a storage handle into the worker so list persistence never fsyncs
        // on the render thread. This does not touch any app's settings/data.
        let storage = cartridge_core::storage::AppStorage {
            app_id: self.storage.app_id.clone(), data_dir: self.storage.data_dir.clone(), cache_dir: self.storage.cache_dir.clone(),
        };
        self.store_jobs.start(operation, self.registry_client.clone(), self.installer.clone(), self.bundled_app_ids.clone(), storage);
    }

    pub fn poll_store_jobs(&mut self) -> bool {
        let (changed, completion) = self.store_jobs.poll();
        if let Some(completion) = completion { self.apply_store_completion(completion); }
        changed
    }

    pub(crate) fn apply_store_completion(&mut self, completion: crate::store_jobs::Completion) {
        for app in self.local_apps.apps.iter().chain(completion.local.apps.iter()) {
            self.invalidated_textures.extend(crate::ui_constants::cached_icon_paths(&app.id));
            crate::ui_constants::invalidate_icon_path(&app.id);
        }
        if let Some(installer) = completion.installer { self.installer = Some(installer); }
        self.installed = completion.local.installed();
        self.local_apps = completion.local;
        if let Some(registry) = completion.registry { self.registry = registry; }
        // A signed refresh can omit bundled or locally installed apps. They
        // remain visible, with package=None unless present in the signed catalog.
        for local in &self.local_apps.apps {
            match self.registry.apps.iter_mut().find(|a| a.id == local.id) {
                Some(entry) if entry.package.is_none() => *entry = local.clone(),
                Some(_) => {},
                None => self.registry.apps.push(local.clone()),
            }
        }
        self.registry_revision = self.registry_revision.wrapping_add(1);
    }
}

/// Preserve identity across reorder/removal; clamp when the selected app vanishes.
pub(crate) fn preserve_selection(previous: &[String], current: &[String], index: i32) -> i32 {
    previous.get(index.max(0) as usize)
        .and_then(|id| current.iter().position(|candidate| candidate == id))
        .map(|i| i as i32)
        .unwrap_or_else(|| index.max(0).min(current.len().saturating_sub(1) as i32))
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    #[test]
    fn reordered_and_smaller_catalogs_keep_identity_or_clamp() {
        let previous = vec!["a".into(), "b".into(), "c".into()];
        assert_eq!(preserve_selection(&previous, &["c".into(),"a".into(),"b".into()], 1), 2);
        assert_eq!(preserve_selection(&previous, &["a".into()], 2), 0);
        assert_eq!(preserve_selection(&previous, &[], 2), 0);
    }
}

#[cfg(test)]
pub(crate) fn test_context() -> ScreenContext {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!("cartridge-context-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    ScreenContext {
        registry: crate::data::Registry::empty(), bundled_app_ids: vec![], installed: Default::default(), local_apps: Default::default(),
        store_jobs: Default::default(), registry_revision: 0, invalidated_textures: vec![], notice_pages: 0, automatic_store_refresh: true,
        settings: crate::data::LauncherSettings { auto_refresh: false, ..Default::default() }, recents: vec![],
        storage: cartridge_core::storage::AppStorage::at_root("launcher", root), registry_client: None, installer: None,
        sysinfo: Default::default(),
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;
    use crate::data::{AppEntry, Registry};
    use crate::store_jobs::{Completion, LocalApps};
    use cartridge_core::input::{Button, InputAction};

    fn entry(id: &str, version: &str) -> AppEntry {
        serde_json::from_value(serde_json::json!({"id":id,"name":id,"version":version})).unwrap()
    }

    #[test]
    fn completion_reconciles_library_and_detail_keeps_id_through_catalog_reorder() {
        let mut ctx = test_context();
        let a = entry("dev.cartridge.a", "1.0.0");
        let b = entry("dev.cartridge.b", "1.0.0");
        let mut remote = a.clone();
        remote.version = "1.1.0".into();
        remote.package = Some(cartridge_net::AppPackage {url:"https://example.org/a.tgz".into(),sha256:"aa".into(),size:1,min_runtime:"0.6.0".into()});
        ctx.registry.apps = vec![remote.clone(), b.clone()];
        let mut detail = detail::DetailScreen::new(a.id.clone());
        ctx.apply_store_completion(Completion {
            local: LocalApps { apps: vec![a.clone(), b.clone()], ..Default::default() }, installer: None,
            registry: Some(Registry { version:1, apps:vec![b.clone(),remote] }), outcome: Ok(None),
        });
        assert!(ctx.has_update(&a.id)); // Includes the bundled version.
        let events = [InputEvent {button:Button::A, action:InputAction::Press}];
        assert!(matches!(detail.handle_input(&events, &mut ctx), ScreenAction::LaunchApp(id) if id == a.id));
        let mut installed = a.clone(); installed.version = "2.0.0".into(); installed.name = "Installed name".into();
        ctx.apply_store_completion(Completion {
            local: LocalApps { apps:vec![installed, b.clone()], overrides:[a.id.clone()].into(), ..Default::default() },
            installer:None, registry:None, outcome:Ok(None),
        });
        assert!(!ctx.has_update(&a.id)); // Newer installed versions aren't downgraded.
        assert_eq!(ctx.installed_apps().iter().find(|app| app.id == a.id).unwrap().name, "Installed name");
        ctx.apply_store_completion(Completion {
            local: LocalApps { apps:vec![a.clone(), b], ..Default::default() }, installer:None,
            registry:None, outcome:Ok(None),
        });
        assert!(ctx.installed.is_installed(&a.id));
        assert!(!ctx.has_override(&a.id));
        assert_eq!(ctx.installed_apps().len(), 2);
        std::fs::remove_dir_all(ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn home_uses_catalog_order_then_custom_installations() {
        let mut ctx = test_context();
        let a = entry("dev.cartridge.a", "1.0.0");
        let b = entry("dev.cartridge.b", "1.0.0");
        let custom = entry("custom.app", "1.0.0");
        ctx.registry.apps = vec![b.clone(), a.clone()];
        ctx.local_apps.apps = vec![custom.clone(), a.clone(), b.clone()];
        let ids: Vec<_> = ctx.installed_apps().iter().map(|app| app.id.as_str()).collect();
        assert_eq!(ids, vec![b.id.as_str(), a.id.as_str(), custom.id.as_str()]);
        std::fs::remove_dir_all(ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }

}
