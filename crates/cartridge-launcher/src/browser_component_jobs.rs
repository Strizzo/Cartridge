//! Browser component work lives with the launcher, never with a screen or frame.
use crate::store_jobs::{Notice, is_newer};
use cartridge_net::browser_component::{
    self, BrowserComponents, ComponentStatus, VerifiedComponent,
};
use std::sync::{
    Arc,
    mpsc::{self, Receiver, TryRecvError},
};
use std::time::Instant;

/// Fixed official channel. Custom registries and environment URLs cannot replace
/// this endpoint or the production verifier's pinned signing key.
pub(crate) const MANIFEST_URL: &str = "https://github.com/Strizzo/Cartridge/releases/download/browser-engine-stable/browser-component.json";

#[derive(Clone, Debug)]
pub(crate) struct EngineRelease {
    pub id: String,
    pub version: String,
    pub chromium: String,
    pub min_runtime: String,
    pub download_bytes: u64,
}
impl EngineRelease {
    fn verified(value: &VerifiedComponent) -> Self {
        let release = value.release();
        Self {
            id: value.id().into(),
            version: release.version.clone(),
            chromium: release.chromium_version.clone(),
            min_runtime: release.min_runtime.clone(),
            download_bytes: release.archive.size,
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct LocalStatus {
    pub supported: bool,
    pub state: ComponentStatus,
    pub current: Option<EngineRelease>,
    pub problem: Option<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Status,
    Check,
    Install,
    Rollback,
}

enum Event {
    Progress(String),
    Complete(Outcome),
}
enum Outcome {
    Status(Result<LocalStatus, String>),
    Checked(Result<EngineRelease, String>),
    Changed {
        release: EngineRelease,
        state: Result<ComponentStatus, String>,
    },
    Failed {
        error: String,
        local: Result<LocalStatus, String>,
    },
}
trait Backend: Send {
    fn status(&self) -> Result<LocalStatus, String>;
    fn check(&self) -> Result<EngineRelease, String>;
    fn install(&self, progress: &mut dyn FnMut(String)) -> Result<EngineRelease, String>;
    fn rollback(&self) -> Result<EngineRelease, String>;
    fn pointers(&self) -> Result<ComponentStatus, String>;
}
struct Production;
impl Backend for Production {
    fn status(&self) -> Result<LocalStatus, String> {
        if !cfg!(target_os = "linux") {
            return Ok(LocalStatus {
                supported: false,
                state: ComponentStatus::default(),
                current: None,
                problem: None,
            });
        }
        let store = BrowserComponents::from_environment()?;
        let state = store.status()?;
        let verified = if state.current.is_some() {
            Some(store.verify_current())
        } else {
            None
        };
        let (current, problem) = match verified {
            Some(Ok(component)) => (Some(EngineRelease::verified(&component)), None),
            Some(Err(error)) => (
                None,
                Some(format!("Installed engine verification failed: {error}")),
            ),
            None => (None, None),
        };
        Ok(LocalStatus {
            supported: true,
            state,
            current,
            problem,
        })
    }
    fn check(&self) -> Result<EngineRelease, String> {
        browser_component::inspect_https(MANIFEST_URL).map(|c| EngineRelease::verified(&c))
    }
    fn install(&self, progress: &mut dyn FnMut(String)) -> Result<EngineRelease, String> {
        BrowserComponents::from_environment()?
            .install_https(MANIFEST_URL, progress)
            .map(|c| EngineRelease::verified(&c))
    }
    fn rollback(&self) -> Result<EngineRelease, String> {
        BrowserComponents::from_environment()?
            .rollback()
            .map(|c| EngineRelease::verified(&c))
    }
    fn pointers(&self) -> Result<ComponentStatus, String> {
        BrowserComponents::from_environment()?.status()
    }
}
type Factory = Arc<dyn Fn() -> Box<dyn Backend> + Send + Sync>;

pub(crate) struct BrowserComponentJobs {
    factory: Factory,
    receiver: Option<Receiver<Event>>,
    operation: Option<Operation>,
    initialized: bool,
    pub local: Option<LocalStatus>,
    pub release: Option<Arc<EngineRelease>>,
    pub progress: Option<String>,
    pub message: String,
    pub is_error: bool,
    pub revision: u64,
    started: Option<Instant>,
}
impl Default for BrowserComponentJobs {
    fn default() -> Self {
        Self::with_factory(Arc::new(|| Box::new(Production)))
    }
}
impl BrowserComponentJobs {
    fn with_factory(factory: Factory) -> Self {
        Self {
            factory,
            receiver: None,
            operation: None,
            initialized: false,
            local: None,
            release: None,
            progress: None,
            message: "Read installed engine status, or check the official signed channel.".into(),
            is_error: false,
            revision: 0,
            started: None,
        }
    }
    pub fn is_busy(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn is_changing(&self) -> bool {
        matches!(
            self.operation,
            Some(Operation::Install | Operation::Rollback)
        )
    }
    pub fn elapsed(&self) -> u64 {
        self.started.map_or(0, |t| t.elapsed().as_secs())
    }
    pub fn ensure_status(&mut self) {
        if !self.initialized && !self.is_busy() {
            self.refresh();
        }
    }
    pub fn refresh(&mut self) -> bool {
        self.start(Operation::Status)
    }
    pub fn check(&mut self) -> bool {
        self.start(Operation::Check)
    }
    pub fn can_install(&self) -> bool {
        let Some(release) = &self.release else {
            return false;
        };
        let Some(local) = &self.local else {
            return false;
        };
        local.supported
            && !self.is_busy()
            && !is_newer(&release.min_runtime, env!("CARGO_PKG_VERSION"))
            && local.current.as_ref().is_none_or(|current| {
                release.id != current.id && is_newer(&release.version, &current.version)
            })
    }
    pub fn can_rollback(&self) -> bool {
        !self.is_busy()
            && self
                .local
                .as_ref()
                .is_some_and(|s| s.supported && s.state.previous.is_some())
    }
    pub fn install(&mut self, preview: Arc<EngineRelease>) -> bool {
        if !self.can_install()
            || !self
                .release
                .as_ref()
                .is_some_and(|r| Arc::ptr_eq(r, &preview))
        {
            return false;
        }
        self.start(Operation::Install)
    }
    pub fn rollback(&mut self, previous: &str) -> bool {
        if !self.can_rollback()
            || !self
                .local
                .as_ref()
                .is_some_and(|s| s.state.previous.as_deref() == Some(previous))
        {
            return false;
        }
        self.start(Operation::Rollback)
    }
    pub fn error(&mut self, message: impl Into<String>) {
        // A preflight refusal is a persistent result too. Reopening Tools must
        // not replace it with an automatically scheduled status verification.
        self.initialized = true;
        self.message = bounded(message.into());
        self.is_error = true;
        self.revision = self.revision.wrapping_add(1);
    }
    fn start(&mut self, operation: Operation) -> bool {
        if self.is_busy() {
            return false;
        }
        let label = match operation {
            Operation::Status => {
                "Verifying installed browser engine files. Large components may take several minutes..."
            }
            Operation::Check => {
                "Checking the official HTTPS channel and verifying its signature..."
            }
            Operation::Install => {
                "Fetching the latest signed manifest, then streaming and verifying the engine..."
            }
            Operation::Rollback => {
                "Verifying the previous engine before switching back. This may take several minutes..."
            }
        };
        let factory = self.factory.clone();
        let (tx, rx) = mpsc::sync_channel(16);
        match std::thread::Builder::new()
            .name("cartridge-browser-component".into())
            .spawn(move || {
                let backend = factory();
                let result = match operation {
                    Operation::Status => Outcome::Status(backend.status()),
                    Operation::Check => Outcome::Checked(backend.check()),
                    Operation::Install | Operation::Rollback => {
                        let result = if operation == Operation::Install {
                            backend.install(&mut |text| {
                                let _ = tx.try_send(Event::Progress(bounded(text)));
                            })
                        } else {
                            backend.rollback()
                        };
                        match result {
                            Ok(release) => Outcome::Changed {
                                release,
                                state: backend.pointers(),
                            },
                            // Activation can precede an fsync/status failure. Re-read
                            // actual state; never promise that an error kept it old.
                            Err(error) => Outcome::Failed {
                                error,
                                local: backend.status(),
                            },
                        }
                    }
                };
                let _ = tx.send(Event::Complete(result));
            }) {
            Ok(_) => {
                self.receiver = Some(rx);
                self.operation = Some(operation);
                self.initialized = true;
                self.started = Some(Instant::now());
                self.message = label.into();
                self.progress = Some(label.into());
                self.is_error = false;
                if operation == Operation::Check {
                    self.release = None;
                }
                self.revision = self.revision.wrapping_add(1);
                true
            }
            Err(error) => {
                self.error(format!("Cannot start browser component worker: {error}"));
                false
            }
        }
    }
    pub fn poll(&mut self) -> (bool, Option<Notice>) {
        let mut changed = false;
        for _ in 0..17 {
            let Some(receiver) = &self.receiver else {
                break;
            };
            match receiver.try_recv() {
                Ok(Event::Progress(text)) => {
                    self.progress = Some(text.clone());
                    self.message = text;
                    changed = true;
                }
                Ok(Event::Complete(outcome)) => {
                    self.operation = None;
                    self.receiver = None;
                    self.progress = None;
                    self.started = None;
                    self.is_error = false;
                    match outcome {
                        Outcome::Status(Ok(local)) => {
                            self.message = if let Some(error) = &local.problem {
                                self.is_error = true;
                                error.clone()
                            } else if !local.supported {
                                "The desktop simulator uses local Chromium. Component installation and rollback are available on Linux handhelds.".into()
                            } else if local.current.is_some() {
                                "Installed engine files verified. A checks for a signed release."
                                    .into()
                            } else {
                                "No signed browser engine is installed. A checks the official channel.".into()
                            };
                            self.local = Some(local);
                            if !self.is_error && self.can_install() {
                                self.message = "Installed status checked. A reviews installation of the latest signed engine.".into();
                            }
                        }
                        Outcome::Status(Err(error)) => {
                            self.local = None;
                            self.error(error);
                        }
                        Outcome::Checked(Ok(release)) => {
                            self.release = Some(Arc::new(release));
                            self.message = if self.release.as_ref().is_some_and(|r| is_newer(&r.min_runtime, env!("CARGO_PKG_VERSION"))) {
                                "This engine requires a newer CartridgeOS. Use Settings > System Update first."
                            } else if self.local.is_none() {
                                "Signed release checked. R1 must read installed status before installation."
                            } else if self.can_install() {
                                "A signed browser engine is available. A reviews installation of the latest signed release."
                            } else if self.local.as_ref().is_some_and(|s| !s.supported) {
                                "Signed release checked. Installation is available on Linux handhelds."
                            } else {
                                "The installed engine is already at this version or newer. Y checks again."
                            }.into();
                        }
                        Outcome::Checked(Err(error)) => {
                            self.release = None;
                            if error.starts_with("Component server returned 404") {
                                self.message="No signed browser engine release is published on the official channel yet. Y checks again later.".into();
                            } else {
                                self.error(format!("Engine check failed: {error}. Y retries; R1 refreshes installed status."));
                            }
                        }
                        Outcome::Changed { release, state } => {
                            self.release = None;
                            match state {
                                Ok(state) if state.current.as_deref() != Some(&release.id) => {
                                    self.local=Some(LocalStatus{supported:true,state,current:None,problem:Some("Component pointers changed after activation. R1 verifies installed status.".into())});
                                    self.error("The engine was activated, but its pointers changed before status was read. R1 verifies actual installed status.");
                                }
                                Ok(state) => {
                                    self.message = format!(
                                        "Browser engine {} activated. Open Browser from your apps.",
                                        release.version
                                    );
                                    self.local = Some(LocalStatus {
                                        supported: true,
                                        state,
                                        current: Some(release),
                                        problem: None,
                                    });
                                }
                                Err(error) => {
                                    self.local = None;
                                    self.error(format!("Engine {} was activated, but status could not be read: {error}. R1 verifies installed status.",release.version));
                                }
                            }
                        }
                        Outcome::Failed { error, local } => {
                            self.release = None;
                            match local {
                                Ok(local) => {
                                    self.local = Some(local);
                                    self.error(format!("Engine operation failed: {error}. Installed status was re-read; review it below before retrying."));
                                }
                                Err(status_error) => {
                                    self.local = None;
                                    self.error(format!("Engine operation failed: {error}. Status is also unavailable: {status_error}. R1 retries verification."));
                                }
                            }
                        }
                    }
                    self.message = bounded(std::mem::take(&mut self.message));
                    self.revision = self.revision.wrapping_add(1);
                    return (
                        true,
                        Some(Notice {
                            message: format!("Browser Tools: {}", self.message),
                            is_error: self.is_error,
                        }),
                    );
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.operation = None;
                    self.progress = None;
                    self.started = None;
                    self.release = None;
                    self.error("Browser component worker stopped unexpectedly. R1 refreshes actual installed status.");
                    return (
                        true,
                        Some(Notice {
                            message: self.message.clone(),
                            is_error: true,
                        }),
                    );
                }
            }
        }
        (changed, None)
    }
}
fn bounded(text: String) -> String {
    text.chars().take(4096).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Blocked {
        wait: Arc<std::sync::Mutex<Option<Receiver<()>>>>,
    }
    impl Backend for Blocked {
        fn status(&self) -> Result<LocalStatus, String> {
            if let Some(wait) = self.wait.lock().unwrap().take() {
                let _ = wait.recv();
            }
            Err("Test status failure".into())
        }
        fn check(&self) -> Result<EngineRelease, String> {
            Err("Component server returned 404 Not Found".into())
        }
        fn install(&self, _: &mut dyn FnMut(String)) -> Result<EngineRelease, String> {
            Err("Test install refused".into())
        }
        fn rollback(&self) -> Result<EngineRelease, String> {
            Err("Test rollback refused".into())
        }
        fn pointers(&self) -> Result<ComponentStatus, String> {
            Ok(ComponentStatus::default())
        }
    }
    pub(crate) fn blocked_for_test(changing: bool) -> (BrowserComponentJobs, mpsc::SyncSender<()>) {
        let (done, wait) = mpsc::sync_channel(1);
        let slot = Arc::new(std::sync::Mutex::new(Some(wait)));
        let mut jobs = BrowserComponentJobs::with_factory(Arc::new(move || {
            Box::new(Blocked { wait: slot.clone() })
        }));
        jobs.refresh();
        if changing {
            jobs.operation = Some(Operation::Install);
        }
        (jobs, done)
    }
    fn wait_done(jobs: &mut BrowserComponentJobs) -> Notice {
        let until = Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let (_, notice) = jobs.poll();
            if let Some(notice) = notice {
                return notice;
            }
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
    }
    #[test]
    fn verification_runs_off_thread_and_poll_and_drop_never_join_it() {
        let (mut jobs, done) = blocked_for_test(false);
        let now = Instant::now();
        assert!(jobs.is_busy());
        assert!(!jobs.check());
        assert!(!jobs.poll().0);
        assert!(now.elapsed() < std::time::Duration::from_millis(100));
        done.send(()).unwrap();
        let notice = wait_done(&mut jobs);
        assert!(notice.is_error);
        assert!(notice.message.contains("Test status failure"));
        assert!(!jobs.is_busy());
        let (jobs, done) = blocked_for_test(false);
        let now = Instant::now();
        drop(jobs);
        assert!(now.elapsed() < std::time::Duration::from_millis(100));
        done.send(()).unwrap();
    }
    #[test]
    fn unavailable_channel_is_neither_an_installed_engine_nor_up_to_date() {
        let (mut jobs, done) = blocked_for_test(false);
        done.send(()).unwrap();
        wait_done(&mut jobs);
        assert!(jobs.check());
        let notice = wait_done(&mut jobs);
        assert!(!notice.is_error);
        assert!(
            jobs.message
                .contains("No signed browser engine release is published")
        );
        assert!(jobs.release.is_none());
        assert!(!jobs.can_install());
    }
    #[test]
    fn production_verifier_rejects_unsigned_component_before_network_or_install() {
        assert!(
            browser_component::verify_manifest(
                r#"{"version":"1.0.0","url":"https://example.org/engine"}"#
            )
            .is_err()
        );
        assert_eq!(
            MANIFEST_URL,
            "https://github.com/Strizzo/Cartridge/releases/download/browser-engine-stable/browser-component.json"
        );
    }
    struct RecordingBackend {
        calls: mpsc::SyncSender<Operation>,
    }
    impl Backend for RecordingBackend {
        fn status(&self) -> Result<LocalStatus, String> {
            Err("Test backend has no installed engine".into())
        }
        fn check(&self) -> Result<EngineRelease, String> {
            Err("Test backend has no release".into())
        }
        fn install(&self, _: &mut dyn FnMut(String)) -> Result<EngineRelease, String> {
            self.calls.send(Operation::Install).unwrap();
            Err("Test backend refused installation".into())
        }
        fn rollback(&self) -> Result<EngineRelease, String> {
            self.calls.send(Operation::Rollback).unwrap();
            Err("Test backend refused rollback".into())
        }
        fn pointers(&self) -> Result<ComponentStatus, String> {
            Ok(ComponentStatus::default())
        }
    }
    pub(crate) fn recording_for_test() -> (BrowserComponentJobs, Receiver<Operation>) {
        let (calls, receiver) = mpsc::sync_channel(4);
        let mut jobs = ready_for_test();
        jobs.factory = Arc::new(move || {
            Box::new(RecordingBackend {
                calls: calls.clone(),
            })
        });
        (jobs, receiver)
    }
    pub(crate) fn ready_for_test() -> BrowserComponentJobs {
        let mut jobs = BrowserComponentJobs::default();
        jobs.initialized = true;
        jobs.local = Some(LocalStatus {
            supported: true,
            state: ComponentStatus {
                current: Some("a".repeat(64)),
                previous: Some("b".repeat(64)),
            },
            current: Some(EngineRelease {
                id: "a".repeat(64),
                version: "1.0.0".into(),
                chromium: "147.0.0 fixture".into(),
                min_runtime: "0.8.0".into(),
                download_bytes: 1,
            }),
            problem: None,
        });
        jobs.release = Some(Arc::new(EngineRelease {
            id: "c".repeat(64),
            version: "1.1.0".into(),
            chromium: "147.0.1 fixture".into(),
            min_runtime: "0.8.0".into(),
            download_bytes: 500_000_000,
        }));
        jobs
    }
    #[test]
    fn compatibility_and_stale_confirmation_cannot_start_an_install() {
        let mut jobs = ready_for_test();
        assert!(jobs.can_install());
        let stale = jobs.release.as_ref().unwrap().clone();
        jobs.release = Some(Arc::new((**jobs.release.as_ref().unwrap()).clone()));
        assert!(!jobs.install(stale));
        assert!(!jobs.is_busy());
        Arc::get_mut(jobs.release.as_mut().unwrap())
            .unwrap()
            .min_runtime = "999.0.0".into();
        assert!(!jobs.can_install());
        assert!(!jobs.rollback(&"c".repeat(64)));
        assert!(!jobs.is_busy());
    }
}
#[cfg(test)]
pub(crate) use tests::{blocked_for_test, ready_for_test, recording_for_test};
