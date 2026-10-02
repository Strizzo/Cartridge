//! Store I/O belongs to the shared launcher context, never to a screen's lifetime.
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use crate::data::{AppEntry, InstalledApps, Registry};

#[derive(Default, Debug)]
pub struct LocalApps {
    pub apps: Vec<AppEntry>,
    pub overrides: HashSet<String>,
    pub rollbacks: HashSet<String>,
}

impl LocalApps {
    pub fn installed(&self) -> InstalledApps {
        InstalledApps {
            app_ids: self.apps.iter().map(|a| a.id.clone()).collect(),
        }
    }

    /// Read both trees and use exactly the same precedence as launch/icon lookup.
    fn scan(
        installed: &Path,
        bundled: &Path,
        installer: Option<&cartridge_net::AppInstaller>,
        bundled_app_ids: &[String],
    ) -> Self {
        let mut candidates = BTreeMap::new();
        for root in [bundled, installed] {
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(bytes) = std::fs::read(entry.path().join("cartridge.json")) else {
                    continue;
                };
                if let Ok(app) = serde_json::from_slice::<AppEntry>(&bytes) {
                    // The shipped catalog is the allowlist for bundles; sample
                    // and benchmark cartridges must not appear on Home.
                    if root == bundled && !bundled_app_ids.contains(&app.id) {
                        continue;
                    }
                    candidates.insert(app.id.clone(), app);
                }
            }
        }
        let mut local = Self::default();
        for id in candidates.keys() {
            let Some(dir) = cartridge_core::paths::resolve_cartridge_dir_in(id, installed, bundled)
            else {
                continue;
            };
            if dir.starts_with(bundled) && !bundled_app_ids.contains(id) {
                continue;
            }
            let Ok(bytes) = std::fs::read(dir.join("cartridge.json")) else {
                continue;
            };
            let Ok(mut app) = serde_json::from_slice::<AppEntry>(&bytes) else {
                continue;
            };
            // Only verified remote catalog records authorize remote installs.
            app.package = None;
            if dir.starts_with(installed) {
                local.overrides.insert(id.clone());
                if installer.is_some_and(|i| i.can_rollback(id)) {
                    local.rollbacks.insert(id.clone());
                }
            }
            local.apps.push(app);
        }
        local
    }
}

#[derive(Clone)]
pub enum StoreOperation {
    Sync,
    Refresh { ttl_seconds: u64 },
    Install(AppEntry),
    Update(AppEntry),
    Remove(String),
    Rollback(String),
}

impl StoreOperation {
    fn label(&self) -> String {
        match self {
            Self::Sync => "Reading installed cartridges…".into(),
            Self::Refresh { .. } => "Fetching and verifying the signed catalog…".into(),
            Self::Install(app) => format!(
                "Installing {} v{}: downloading and verifying…",
                app.name, app.version
            ),
            Self::Update(app) => format!(
                "Updating {} to v{}: downloading and verifying…",
                app.name, app.version
            ),
            Self::Remove(id) => format!("Removing {id} override…"),
            Self::Rollback(id) => format!("Restoring the previous version of {id}…"),
        }
    }
}

pub struct Completion {
    pub local: LocalApps,
    pub installer: Option<cartridge_net::AppInstaller>,
    pub registry: Option<Registry>,
    pub outcome: Result<Option<String>, String>,
}

enum JobEvent {
    Progress(String),
    Complete(Completion),
}

pub struct Notice {
    pub message: String,
    pub is_error: bool,
}

#[derive(Default)]
pub struct StoreJobs {
    receiver: Option<Receiver<JobEvent>>,
    pub progress: Option<String>,
    pub notices: VecDeque<Notice>,
    pub notice_page: usize,
}

impl StoreJobs {
    pub fn is_busy(&self) -> bool {
        self.receiver.is_some()
    }

    pub fn error(&mut self, message: impl Into<String>) {
        let message = message.into();
        log::warn!("Store: {message}");
        if !self.notices.iter().any(|n| n.message == message) {
            self.notices.push_back(Notice {
                message,
                is_error: true,
            });
        }
    }

    fn spawn(
        &mut self,
        label: String,
        work: impl FnOnce(Sender<JobEvent>) -> Completion + Send + 'static,
    ) -> bool {
        if self.is_busy() {
            return false;
        }
        let (tx, rx) = mpsc::channel();
        match std::thread::Builder::new()
            .name("cartridge-store".into())
            .spawn(move || {
                let completion = work(tx.clone());
                let _ = tx.send(JobEvent::Complete(completion));
            }) {
            Ok(_) => {
                self.receiver = Some(rx);
                self.progress = Some(label);
                true
            }
            Err(e) => {
                self.error(format!("Cannot start Store job: {e}"));
                false
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn simulate(&mut self, completion: Receiver<Completion>) {
        self.spawn("Test Store operation".into(), move |_| {
            completion.recv().unwrap()
        });
    }

    pub fn start(
        &mut self,
        operation: StoreOperation,
        client: Option<cartridge_net::RegistryClient>,
        installer: Option<cartridge_net::AppInstaller>,
        bundled_app_ids: Vec<String>,
        storage: cartridge_core::storage::AppStorage,
    ) -> bool {
        let installed_root = cartridge_core::paths::installed_apps_dir();
        let bundled_root = cartridge_core::paths::bundled_cartridges_dir();
        self.spawn(operation.label(), move |tx| {
            // Recovery can inspect/rename directories, so initialize here too.
            let installer = installer.or_else(|| {
                Some(cartridge_net::AppInstaller::new(
                    cartridge_net::HttpClient::new(storage.cache_dir.join("http")),
                ))
            });
            let mut registry = None;
            let outcome = (|| -> Result<Option<String>, String> {
                match &operation {
                    StoreOperation::Sync => Ok(None),
                    StoreOperation::Refresh { ttl_seconds } => {
                        let fetched = client
                            .as_ref()
                            .ok_or("Store is unavailable: no registry client")?
                            .fetch_with_cache(*ttl_seconds)?;
                        let count = fetched.apps.len();
                        registry = Some(Registry::from_net(&fetched));
                        Ok(Some(format!(
                            "Catalog verified: {count} apps. Select an app to install or update."
                        )))
                    }
                    StoreOperation::Install(app) | StoreOperation::Update(app) => {
                        if app.package.is_none() {
                            return Err("No verified package is available for this app".into());
                        }
                        let installer =
                            installer.as_ref().ok_or("Store installer is unavailable")?;
                        installer.install(&app.to_net_app())?;
                        Ok(Some(format!(
                            "{} v{} installed. Your settings and data are preserved.",
                            app.name, app.version
                        )))
                    }
                    StoreOperation::Remove(id) => {
                        installer
                            .as_ref()
                            .ok_or("Store installer is unavailable")?
                            .remove(id)?;
                        Ok(Some(format!(
                            "{id}: installed override removed. Settings and data are preserved."
                        )))
                    }
                    StoreOperation::Rollback(id) => {
                        installer
                            .as_ref()
                            .ok_or("Store installer is unavailable")?
                            .rollback(id)?;
                        Ok(Some(format!(
                            "{id}: previous version restored. Settings and data are preserved."
                        )))
                    }
                }
            })();
            let outcome = outcome.map_err(|error| {
                format!(
                    "{} {error}",
                    match &operation {
                        StoreOperation::Sync => "Installed-app scan failed:".into(),
                        StoreOperation::Refresh { .. } => "Catalog refresh failed:".into(),
                        StoreOperation::Install(app) => format!("Install {} failed:", app.name),
                        StoreOperation::Update(app) => format!("Update {} failed:", app.name),
                        StoreOperation::Remove(id) => format!("Remove {id} failed:"),
                        StoreOperation::Rollback(id) => format!("Rollback {id} failed:"),
                    }
                )
            });
            let _ = tx.send(JobEvent::Progress("Updating the local app library…".into()));
            // Also reconcile after failure: a failed removal must never hide an app.
            let local = LocalApps::scan(
                &installed_root,
                &bundled_root,
                installer.as_ref(),
                &bundled_app_ids,
            );
            let outcome = outcome.map(|message| {
                if let StoreOperation::Remove(id) = &operation {
                    if local.apps.iter().any(|app| &app.id == id) {
                        return Some(format!(
                            "{id}: bundled version restored. Settings and data are preserved."
                        ));
                    }
                }
                message
            });
            let outcome = match storage.try_save(
                "installed",
                &serde_json::to_value(local.installed()).unwrap_or_default(),
            ) {
                Ok(()) => outcome,
                Err(e) => Err(format!(
                    "{} Could not save the installed-app list: {e}. It will be rebuilt on startup.",
                    match outcome {
                        Ok(message) => message.unwrap_or_default(),
                        Err(error) => error,
                    }
                )),
            };
            Completion {
                local,
                installer,
                registry,
                outcome,
            }
        })
    }

    /// Nonblocking; the launcher drains this even when Store/Detail is popped.
    pub fn poll(&mut self) -> (bool, Option<Completion>) {
        let mut changed = false;
        loop {
            let Some(receiver) = &self.receiver else {
                return (changed, None);
            };
            match receiver.try_recv() {
                Ok(JobEvent::Progress(message)) => {
                    self.progress = Some(message);
                    changed = true;
                }
                Ok(JobEvent::Complete(completion)) => {
                    self.receiver = None;
                    self.progress = None;
                    match &completion.outcome {
                        Ok(Some(message)) => self.notices.push_back(Notice {
                            message: message.clone(),
                            is_error: false,
                        }),
                        Err(error) => self.error(error.clone()),
                        Ok(None) => {}
                    }
                    return (true, Some(completion));
                }
                Err(TryRecvError::Empty) => return (changed, None),
                Err(TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.progress = None;
                    self.error("Store worker stopped before reporting a result. Refresh to reconcile installed apps.");
                    return (true, None);
                }
            }
        }
    }
}

/// SemVer precedence without an extra launcher dependency. Invalid/legacy
/// versions don't advertise an update; build metadata never changes precedence.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn identifiers(value: &str, prerelease: bool) -> bool {
        !value.is_empty()
            && value.split('.').all(|part| {
                !part.is_empty()
                    && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                    && !(prerelease
                        && part.bytes().all(|c| c.is_ascii_digit())
                        && part.len() > 1
                        && part.starts_with('0'))
            })
    }
    fn parse(value: &str) -> Option<([u64; 3], Option<&str>)> {
        let value = if let Some((base, build)) = value.split_once('+') {
            if !identifiers(build, false) {
                return None;
            }
            base
        } else {
            value
        };
        let (core, pre) = value
            .split_once('-')
            .map_or((value, None), |(a, b)| (a, Some(b)));
        if pre.is_some_and(|p| !identifiers(p, true)) {
            return None;
        }
        let parts: Vec<_> = core.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        let mut numbers = [0; 3];
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty()
                || !part.bytes().all(|c| c.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
            {
                return None;
            }
            numbers[i] = part.parse().ok()?;
        }
        Some((numbers, pre))
    }
    let (Some((a, ap)), Some((b, bp))) = (parse(candidate), parse(current)) else {
        return false;
    };
    if a != b {
        return a > b;
    }
    match (ap, bp) {
        (None, Some(_)) => true,
        (Some(_), None) | (None, None) => false,
        (Some(a), Some(b)) => {
            let mut a = a.split('.');
            let mut b = b.split('.');
            loop {
                match (a.next(), b.next()) {
                    (Some(a), Some(b)) if a != b => {
                        let an = a.bytes().all(|c| c.is_ascii_digit());
                        let bn = b.bytes().all(|c| c.is_ascii_digit());
                        return match (an, bn) {
                            (true, true) => (a.len(), a) > (b.len(), b),
                            (true, false) => false,
                            (false, true) => true,
                            (false, false) => a > b,
                        };
                    }
                    (Some(_), Some(_)) => {}
                    (Some(_), None) => return true,
                    _ => return false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn await_completion(jobs: &mut StoreJobs) -> Completion {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let (_, Some(result)) = jobs.poll() {
                return result;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn background_completion_and_failure_survive_screen_lifetimes() {
        let mut jobs = StoreJobs::default();
        let (release, wait) = mpsc::channel();
        assert!(jobs.spawn("Installing".into(), move |_| {
            wait.recv().unwrap();
            Completion {
                local: LocalApps::default(),
                installer: None,
                registry: None,
                outcome: Err("Download failed".into()),
            }
        }));
        // Poll and navigation stay responsive while work is blocked elsewhere.
        assert!(jobs.poll().1.is_none());
        assert!(jobs.is_busy());
        assert!(!jobs.spawn("Duplicate".into(), |_| unreachable!()));
        release.send(()).unwrap();
        assert!(await_completion(&mut jobs).outcome.is_err());
        assert!(!jobs.is_busy());
        assert!(jobs.progress.is_none());
        assert_eq!(jobs.notices.front().unwrap().message, "Download failed");
        jobs.spawn("Refresh".into(), |_| Completion {
            local: LocalApps::default(),
            installer: None,
            registry: Some(Registry::empty()),
            outcome: Ok(Some("Catalog verified".into())),
        });
        assert!(await_completion(&mut jobs).registry.is_some());
        assert_eq!(jobs.notices.len(), 2); // A later success cannot erase an error.
        assert!(jobs.notices.front().unwrap().is_error);
    }

    #[test]
    fn disconnected_worker_reports_failure_and_unblocks_controls() {
        let mut jobs = StoreJobs::default();
        let (tx, rx) = mpsc::channel();
        jobs.receiver = Some(rx);
        jobs.progress = Some("Installing".into());
        drop(tx);
        assert!(jobs.poll().0);
        assert!(!jobs.is_busy());
        assert!(jobs.progress.is_none());
        assert!(jobs.notices.front().unwrap().is_error);
    }

    #[test]
    fn semantic_updates_never_offer_downgrades_or_metadata_changes() {
        for (candidate, current, expected) in [
            ("1.10.0", "1.9.0", true),
            ("1.9.0", "1.10.0", false),
            ("1.0.0", "1.0.0-rc.2", true),
            ("1.0.0-rc.2", "1.0.0", false),
            ("1.0.0-rc.10", "1.0.0-rc.2", true),
            ("1.0.0-alpha", "1.0.0-1", true),
            ("1.0.0-alpha.1", "1.0.0-alpha", true),
            ("1.0.0+build.2", "1.0.0+build.1", false),
            ("2.0.0", "invalid", false),
            ("2.0", "1.0.0", false),
            ("2.0.0-01", "1.0.0", false),
            ("02.0.0", "1.0.0", false),
        ] {
            assert_eq!(
                is_newer(candidate, current),
                expected,
                "{candidate} vs {current}"
            );
        }
    }

    #[test]
    fn disk_library_keeps_bundles_and_overrides_manifests_then_reveals_bundle() {
        let root = std::env::temp_dir().join(format!("cartridge-library-{}", std::process::id()));
        let installed = root.join("apps");
        let bundled = root.join("bundled");
        let id = "dev.cartridge.frequency";
        let bundle = bundled.join("frequency");
        let local = installed.join(id);
        for (path, version, name) in [
            (&bundle, "1.0.0", "Bundled"),
            (&local, "2.0.0", "Installed"),
        ] {
            std::fs::create_dir_all(path).unwrap();
            std::fs::write(
                path.join("cartridge.json"),
                serde_json::json!({"id":id,"name":name,"version":version,"author":"Test","entry":"main.lua"}).to_string(),
            )
            .unwrap();
            std::fs::write(path.join("main.lua"), "return {}").unwrap();
        }
        let snapshot = LocalApps::scan(&installed, &bundled, None, &[id.into()]);
        assert_eq!(snapshot.apps.len(), 1);
        assert_eq!(snapshot.apps[0].name, "Installed");
        assert_eq!(snapshot.apps[0].version, "2.0.0");
        assert!(snapshot.overrides.contains(id));
        std::fs::remove_dir_all(local).unwrap();
        let snapshot = LocalApps::scan(&installed, &bundled, None, &[id.into()]);
        assert!(snapshot.installed().is_installed(id));
        assert_eq!(snapshot.apps[0].name, "Bundled");
        assert!(snapshot.overrides.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shipped_catalog_hides_example_bundles_but_not_custom_installs() {
        let root =
            std::env::temp_dir().join(format!("cartridge-bundle-filter-{}", std::process::id()));
        let installed = root.join("apps");
        let bundled = root.join("bundled");
        let shipped_id = "dev.cartridge.frequency";
        let custom_id = "custom.example";
        for (parent, id) in [
            (&bundled, shipped_id),
            (&bundled, "dev.cartridge.bench"),
            (&bundled, "dev.cartridge.hello-world"),
            (&installed, custom_id),
        ] {
            let dir = parent.join(id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("cartridge.json"),
                serde_json::json!({
                    "id":id,"name":id,"version":"1.0.0","author":"Test","entry":"main.lua"
                })
                .to_string(),
            )
            .unwrap();
            std::fs::write(dir.join("main.lua"), "return {}").unwrap();
        }
        let snapshot = LocalApps::scan(&installed, &bundled, None, &[shipped_id.into()]);
        let ids: Vec<_> = snapshot.apps.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec![custom_id, shipped_id]);
        assert!(snapshot.overrides.contains(custom_id));
        // A partial legacy install must not accidentally reveal a hidden bundle.
        let partial = installed.join("dev.cartridge.bench");
        std::fs::create_dir_all(&partial).unwrap();
        std::fs::copy(
            bundled.join("dev.cartridge.bench/cartridge.json"),
            partial.join("cartridge.json"),
        )
        .unwrap();
        assert_eq!(
            LocalApps::scan(&installed, &bundled, None, &[shipped_id.into()])
                .apps
                .len(),
            2
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
