//! Shared, nonblocking system update work. No environment or disk access on the UI thread.
use cartridge_net::system_update::{Release, SystemUpdater, UpdateStatus};
use std::sync::{
    Arc,
    mpsc::{self, Receiver, SyncSender, TryRecvError},
};

use crate::store_jobs::Notice;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Status,
    Check,
    Stage,
}

enum Request {
    Status,
    Check,
    Stage {
        release: Arc<Release>,
        battery: Option<u8>,
        charging: bool,
    },
}

enum Outcome {
    Status(UpdateStatus),
    Checked(UpdateStatus, Option<Release>),
    // A failed status read after staging must not be reported as a failed download.
    Staged(Result<UpdateStatus, String>),
}

enum Event {
    Progress(String),
    Complete(Result<Outcome, String>),
}

trait Backend: Send {
    fn status(&self) -> Result<UpdateStatus, String>;
    fn check(&self) -> Result<Option<Release>, String>;
    fn stage(
        &self,
        release: &Release,
        battery: Option<u8>,
        charging: bool,
        progress: &mut dyn FnMut(String),
    ) -> Result<(), String>;
}

impl Backend for SystemUpdater {
    fn status(&self) -> Result<UpdateStatus, String> {
        self.status()
    }
    fn check(&self) -> Result<Option<Release>, String> {
        self.check()
    }
    fn stage(
        &self,
        release: &Release,
        battery: Option<u8>,
        charging: bool,
        progress: &mut dyn FnMut(String),
    ) -> Result<(), String> {
        self.stage(release, battery, charging, progress)
    }
}

pub(crate) fn power_ready(battery: i32, charging: bool) -> bool {
    charging || (30..=100).contains(&battery)
}

type Factory = Arc<dyn Fn() -> Result<Box<dyn Backend>, String> + Send + Sync>;

pub(crate) struct SystemUpdateJobs {
    factory: Factory,
    receiver: Option<Receiver<Event>>,
    operation: Option<Operation>,
    initialized: bool,
    pub status: Option<UpdateStatus>,
    pub release: Option<Arc<Release>>,
    pub progress: Option<String>,
    pub message: String,
    pub is_error: bool,
    pub revision: u64,
}

impl Default for SystemUpdateJobs {
    fn default() -> Self {
        Self::with_factory(Arc::new(|| {
            SystemUpdater::from_environment().map(|updater| Box::new(updater) as Box<dyn Backend>)
        }))
    }
}

impl SystemUpdateJobs {
    fn with_factory(factory: Factory) -> Self {
        Self {
            factory,
            receiver: None,
            operation: None,
            initialized: false,
            status: None,
            release: None,
            progress: None,
            message: "Read local update status; press A to check for a release.".into(),
            is_error: false,
            revision: 0,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.receiver.is_some()
    }
    pub fn is_staging(&self) -> bool {
        self.operation == Some(Operation::Stage)
    }
    pub fn can_restart(&self) -> bool {
        !self.is_busy() && self.status.as_ref().is_some_and(|s| s.pending.is_some())
    }

    pub fn ensure_status(&mut self) {
        if !self.initialized && !self.is_busy() {
            self.start(Request::Status);
        }
    }

    pub fn check(&mut self) -> bool {
        self.start(Request::Check)
    }

    pub fn stage(&mut self, release: Arc<Release>, battery: Option<u8>, charging: bool) -> bool {
        // Confirmation applies only to the exact release shown to the user.
        if !self
            .release
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &release))
        {
            return false;
        }
        self.start(Request::Stage {
            release,
            battery,
            charging,
        })
    }

    fn start(&mut self, request: Request) -> bool {
        if self.is_busy() {
            return false;
        }
        let operation = match &request {
            Request::Status => Operation::Status,
            Request::Check => Operation::Check,
            Request::Stage { .. } => Operation::Stage,
        };
        let label = match operation {
            Operation::Status => "Reading local update status...",
            Operation::Check => "Checking for system updates...",
            Operation::Stage => "Preparing system update...",
        };
        let factory = Arc::clone(&self.factory);
        // Bound progress memory and work per frame, even if a downloader is very chatty.
        let (tx, rx) = mpsc::sync_channel(16);
        match std::thread::Builder::new()
            .name("cartridge-system-update".into())
            .spawn(move || {
                let result = run(request, factory, &tx);
                // Only the worker can wait for channel space; dropping the launcher never joins it.
                let _ = tx.send(Event::Complete(result));
            }) {
            Ok(_) => {
                self.receiver = Some(rx);
                self.operation = Some(operation);
                self.initialized = true;
                if operation == Operation::Check {
                    self.release = None;
                }
                self.progress = Some(label.into());
                self.message = label.into();
                self.is_error = false;
                self.revision = self.revision.wrapping_add(1);
                true
            }
            Err(error) => {
                self.initialized = true;
                self.message =
                    format!("Cannot start system update worker: {error}. Press A to retry.");
                self.is_error = true;
                self.revision = self.revision.wrapping_add(1);
                false
            }
        }
    }

    /// Returns a persistent global notice for explicit check/stage outcomes.
    pub fn poll(&mut self) -> (bool, Option<Notice>) {
        let mut changed = false;
        for _ in 0..17 {
            let Some(receiver) = &self.receiver else {
                break;
            };
            match receiver.try_recv() {
                Ok(Event::Progress(message)) => {
                    self.progress = Some(message.clone());
                    self.message = message;
                    changed = true;
                }
                Ok(Event::Complete(result)) => {
                    let operation = self.operation.take();
                    self.receiver = None;
                    self.progress = None;
                    self.is_error = false;
                    match result {
                        Ok(Outcome::Status(status)) => {
                            self.message = if status.pending.is_some() {
                                "An update is staged. Press A to restart CartridgeOS and apply it."
                            } else {
                                "Press A to check for system updates."
                            }
                            .into();
                            self.status = Some(status);
                        }
                        Ok(Outcome::Checked(status, release)) => {
                            self.message = if status.pending.is_some() {
                                "An update is staged. Press A to restart CartridgeOS and apply it."
                            } else if release.is_some() {
                                "A system update is available. Press A to review the download confirmation."
                            } else { "You are up to date. Press A to check again." }.into();
                            self.status = Some(status);
                            self.release = release.map(Arc::new);
                        }
                        Ok(Outcome::Staged(status)) => {
                            self.release = None;
                            match status {
                                Ok(status) => {
                                    self.status = Some(status);
                                    self.message = "System update staged. Press A in System Update to restart CartridgeOS and apply it.".into();
                                }
                                Err(error) => {
                                    self.status = None;
                                    self.is_error = true;
                                    self.message = format!(
                                        "The update was staged, but its status could not be read: {error}\nPress A to check again before restarting."
                                    );
                                }
                            }
                        }
                        Err(error) => {
                            self.status = None;
                            self.release = None;
                            self.message = format!("{error}\nPress A to retry the update check.");
                            self.is_error = true;
                        }
                    }
                    self.revision = self.revision.wrapping_add(1);
                    let notice = (operation != Some(Operation::Status)).then(|| Notice {
                        message: format!("System Update: {}", self.message),
                        is_error: self.is_error,
                    });
                    return (true, notice);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.operation = None;
                    self.progress = None;
                    self.release = None;
                    self.status = None;
                    self.is_error = true;
                    self.message = "System update worker stopped unexpectedly. Press A to retry the update check.".into();
                    self.revision = self.revision.wrapping_add(1);
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

    #[cfg(test)]
    pub(crate) fn blocked_for_test(staging: bool) -> (Self, SyncSender<()>) {
        let (done, wait) = mpsc::sync_channel(1);
        let mut jobs = Self::default();
        let (tx, rx) = mpsc::sync_channel(16);
        jobs.initialized = true;
        jobs.receiver = Some(rx);
        jobs.operation = Some(if staging {
            Operation::Stage
        } else {
            Operation::Check
        });
        jobs.progress = Some("Test system update".into());
        std::thread::spawn(move || {
            let _ = wait.recv();
            let _ = tx.send(Event::Complete(Err("Test update failure".into())));
        });
        (jobs, done)
    }
}

fn run(request: Request, factory: Factory, tx: &SyncSender<Event>) -> Result<Outcome, String> {
    // Construction, configuration lookup and every updater call stay in this worker.
    let updater = factory()?;
    match request {
        Request::Status => updater.status().map(Outcome::Status),
        Request::Check => {
            let status = updater.status()?;
            let release = if status.pending.is_none() {
                updater.check()?
            } else {
                None
            };
            Ok(Outcome::Checked(status, release))
        }
        Request::Stage {
            release,
            battery,
            charging,
        } => {
            updater.stage(&release, battery, charging, &mut |message| {
                let _ = tx.try_send(Event::Progress(message));
            })?;
            Ok(Outcome::Staged(updater.status()))
        }
    }
}

#[cfg(test)]
pub(crate) fn test_release() -> Release {
    serde_json::from_value(serde_json::json!({
        "schema": 1, "channel": "stable", "version": "0.6.2",
        "revision": "0123456789abcdef0123456789abcdef01234567",
        "target": "r36s-plus-aarch64", "min_runtime": "0.6.1", "min_supervisor": 1,
        "notes": "A verified test release.",
        "archive": { "url": "https://github.com/Strizzo/Cartridge/releases/download/v0.6.2/system.tar.gz",
            "size": 1048576, "unpacked_size": 2097152, "sha256": "00".repeat(32) },
        "files": []
    })).unwrap()
}

#[cfg(test)]
impl SystemUpdateJobs {
    pub(crate) fn ready_for_test() -> Self {
        let mut jobs = Self::with_factory(Arc::new(|| Err("Test worker unavailable".into())));
        jobs.initialized = true;
        jobs.status = Some(UpdateStatus::default());
        jobs.release = Some(Arc::new(test_release()));
        jobs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct Recorded {
        status: UpdateStatus,
        checks: usize,
        stage_power: Vec<(Option<u8>, bool)>,
        threads: Vec<std::thread::ThreadId>,
        stage_failure: bool,
        status_failure: bool,
    }
    struct Fake(Arc<Mutex<Recorded>>);
    impl Backend for Fake {
        fn status(&self) -> Result<UpdateStatus, String> {
            let mut state = self.0.lock().unwrap();
            state.threads.push(std::thread::current().id());
            if state.status_failure {
                return Err("State read failed".into());
            }
            Ok(state.status.clone())
        }
        fn check(&self) -> Result<Option<Release>, String> {
            let mut state = self.0.lock().unwrap();
            state.threads.push(std::thread::current().id());
            state.checks += 1;
            Ok(Some(test_release()))
        }
        fn stage(
            &self,
            release: &Release,
            battery: Option<u8>,
            charging: bool,
            progress: &mut dyn FnMut(String),
        ) -> Result<(), String> {
            // Exceed the bounded queue without blocking the producer or the UI.
            for i in 0..1000 {
                progress(format!("Download block {i}"));
            }
            let mut state = self.0.lock().unwrap();
            state.threads.push(std::thread::current().id());
            state.stage_power.push((battery, charging));
            if state.stage_failure {
                return Err("Download disconnected".into());
            }
            state.status.pending = Some(release.id());
            Ok(())
        }
    }

    fn fake_jobs() -> (SystemUpdateJobs, Arc<Mutex<Recorded>>) {
        let state = Arc::new(Mutex::new(Recorded::default()));
        let worker_state = Arc::clone(&state);
        let jobs = SystemUpdateJobs::with_factory(Arc::new(move || {
            worker_state
                .lock()
                .unwrap()
                .threads
                .push(std::thread::current().id());
            Ok(Box::new(Fake(Arc::clone(&worker_state))))
        }));
        (jobs, state)
    }

    fn finish(jobs: &mut SystemUpdateJobs) -> Option<Notice> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut notice = None;
        while jobs.is_busy() {
            let (_, next) = jobs.poll();
            if next.is_some() {
                notice = next;
            }
            assert!(Instant::now() < deadline, "worker did not complete");
            std::thread::yield_now();
        }
        notice
    }

    #[test]
    fn system_update_status_is_local_and_all_io_runs_on_workers() {
        let (mut jobs, recorded) = fake_jobs();
        assert!(recorded.lock().unwrap().threads.is_empty());
        jobs.ensure_status();
        assert!(finish(&mut jobs).is_none());
        assert_eq!(recorded.lock().unwrap().checks, 0);
        jobs.ensure_status();
        assert!(!jobs.is_busy());
        assert!(jobs.check());
        assert!(!finish(&mut jobs).unwrap().is_error);
        assert_eq!(recorded.lock().unwrap().checks, 1);
        let release = Arc::clone(jobs.release.as_ref().unwrap());
        // A different object with the same fields cannot substitute for what was confirmed.
        assert!(!jobs.stage(Arc::new(test_release()), Some(70), false));
        assert!(jobs.stage(release, Some(70), false));
        assert!(jobs.is_staging());
        assert!(!jobs.check());
        assert!(!finish(&mut jobs).unwrap().is_error);
        assert!(jobs.can_restart());
        assert!(jobs.release.is_none());
        let status = jobs.status.as_ref().unwrap();
        assert_eq!(status.pending.as_deref(), Some("0.6.2-0123456789ab"));
        assert!(status.active.is_none());
        assert!(status.previous.is_none());
        let state = recorded.lock().unwrap();
        assert_eq!(state.stage_power, [(Some(70), false)]);
        assert!(
            state
                .threads
                .iter()
                .all(|id| *id != std::thread::current().id())
        );
    }

    #[test]
    fn system_update_failures_clear_busy_and_allow_an_explicit_retry() {
        let (mut jobs, state) = fake_jobs();
        jobs.check();
        finish(&mut jobs);
        state.lock().unwrap().stage_failure = true;
        assert!(jobs.stage(Arc::clone(jobs.release.as_ref().unwrap()), None, true));
        assert!(finish(&mut jobs).unwrap().is_error);
        assert!(jobs.is_error);
        assert!(!jobs.is_staging());
        assert!(!jobs.can_restart());
        assert!(jobs.message.contains("Download disconnected"));
        state.lock().unwrap().stage_failure = false;
        assert!(jobs.check());
        finish(&mut jobs);
        assert!(!jobs.is_error);
        assert!(jobs.release.is_some());
        assert!(jobs.stage(Arc::clone(jobs.release.as_ref().unwrap()), None, true));
        finish(&mut jobs);
        assert!(jobs.can_restart());
    }

    #[test]
    fn system_update_pending_check_never_fetches_another_release() {
        let (mut jobs, state) = fake_jobs();
        state.lock().unwrap().status.pending = Some(test_release().id());
        jobs.check();
        finish(&mut jobs);
        assert_eq!(state.lock().unwrap().checks, 0);
        assert!(jobs.can_restart());
        assert!(jobs.release.is_none());
    }

    #[test]
    fn system_update_unknown_state_fails_before_network_and_cannot_restart() {
        let (mut jobs, state) = fake_jobs();
        state.lock().unwrap().status_failure = true;
        jobs.check();
        assert!(finish(&mut jobs).unwrap().is_error);
        assert_eq!(state.lock().unwrap().checks, 0);
        assert!(!jobs.can_restart());
    }

    #[test]
    fn system_update_poll_and_drop_do_not_wait_for_a_blocked_download() {
        let (mut jobs, done) = SystemUpdateJobs::blocked_for_test(true);
        for _ in 0..100 {
            assert!(!jobs.poll().0);
        }
        let (dropped, rx) = mpsc::channel();
        std::thread::spawn(move || {
            drop(jobs);
            dropped.send(()).unwrap();
        });
        let result = rx.recv_timeout(Duration::from_secs(2));
        // Always unblock the fake worker, including if the assertion fails.
        done.send(()).unwrap();
        result.expect("dropping launcher jobs must not join a blocked worker");
    }

    #[test]
    fn system_update_disconnected_worker_unlocks_retry_and_reports_error() {
        let mut jobs = SystemUpdateJobs::ready_for_test();
        let (tx, rx) = mpsc::sync_channel(1);
        jobs.receiver = Some(rx);
        jobs.operation = Some(Operation::Stage);
        drop(tx);
        let (changed, notice) = jobs.poll();
        assert!(changed && notice.unwrap().is_error);
        assert!(!jobs.is_busy());
        assert!(!jobs.can_restart());
        assert!(jobs.check());
        finish(&mut jobs);
    }
}
