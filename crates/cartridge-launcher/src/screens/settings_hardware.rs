//! Bounded background controls. Each device has one persistent worker so closing
//! and reopening Settings cannot reorder writes. Locks protect only tiny mailbox
//! updates: hardware work always runs outside them. The screen owns its UI cache.

use cartridge_core::device;
use std::sync::{Arc, Mutex, OnceLock, mpsc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Read,
    Write(u8),
    // Used only when leaving before the initial level is known.
    Adjust(Adjustment),
}

struct Completion {
    revision: u64,
    result: Result<u8, String>,
}

type Reply = Arc<Mutex<Option<Completion>>>;

struct Request {
    revision: u64,
    operation: Operation,
    reply: Reply,
}

#[derive(Default)]
struct Mailbox {
    read: Option<Request>,
    write: Option<Request>,
}

impl Mailbox {
    fn put(&mut self, request: Request) {
        match request.operation {
            Operation::Read => self.read = Some(request),
            Operation::Write(_) | Operation::Adjust(_) => self.write = Some(request),
        }
    }

    fn take(&mut self) -> Option<Request> {
        // Finish accepted targets before a newly opened screen reads the device.
        self.write.take().or_else(|| self.read.take())
    }
}

#[derive(Clone)]
struct Worker {
    mailbox: Arc<Mutex<Mailbox>>,
    wake: mpsc::SyncSender<()>,
}

impl Worker {
    fn spawn(
        name: &str,
        mut execute: impl FnMut(Operation) -> Result<u8, String> + Send + 'static,
    ) -> Result<Self, String> {
        let mailbox = Arc::new(Mutex::new(Mailbox::default()));
        let worker_mailbox = mailbox.clone();
        // Only a wakeup travels through the channel; targets replace a single
        // pending slot instead of creating an unbounded queue of mixer processes.
        let (wake, wakeups) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                while wakeups.recv().is_ok() {
                    loop {
                        let request = worker_mailbox.lock().unwrap().take();
                        let Some(request) = request else { break };
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            execute(request.operation)
                        }))
                        .unwrap_or_else(|_| Err("Hardware worker failed; press A to retry".into()));
                        let mut slot = request.reply.lock().unwrap();
                        // A late initial read must not replace a newer write's
                        // completion even when the screen hasn't polled yet.
                        if slot
                            .as_ref()
                            .is_none_or(|old| old.revision <= request.revision)
                        {
                            *slot = Some(Completion {
                                revision: request.revision,
                                result,
                            });
                        }
                    }
                }
            })
            .map_err(|e| format!("Cannot start hardware worker: {e}"))?;
        Ok(Self { mailbox, wake })
    }

    fn submit(&self, request: Request) -> Result<(), String> {
        self.mailbox.lock().unwrap().put(request);
        match self.wake.try_send(()) {
            Ok(()) | Err(mpsc::TrySendError::Full(())) => Ok(()),
            Err(mpsc::TrySendError::Disconnected(())) => Err("Hardware worker stopped".into()),
        }
    }
}

/// A compact composition of clamped relative changes, used before the initial
/// read completes. It preserves direction changes at 0/100 without guessing the
/// hardware's starting value or retaining every held-Dpad event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Adjustment {
    shift: i16,
    low: u8,
    high: u8,
}

impl Default for Adjustment {
    fn default() -> Self {
        Self {
            shift: 0,
            low: 0,
            high: 100,
        }
    }
}

impl Adjustment {
    fn add(&mut self, delta: i16) {
        self.shift += delta;
        self.low = (i16::from(self.low) + delta).clamp(0, 100) as u8;
        self.high = (i16::from(self.high) + delta).clamp(0, 100) as u8;
        if self.low == self.high {
            self.shift = 0;
        }
    }

    fn apply(self, value: u8) -> u8 {
        (i16::from(value) + self.shift).clamp(i16::from(self.low), i16::from(self.high)) as u8
    }
}

fn execute_device(
    operation: Operation,
    read: impl FnOnce() -> Result<u8, String>,
    write: impl FnOnce(u8) -> Result<(), String>,
) -> Result<u8, String> {
    let target = match operation {
        Operation::Read => return read(),
        Operation::Write(value) => value,
        Operation::Adjust(adjustment) => adjustment.apply(read()?),
    };
    write(target)?;
    Ok(target)
}

pub(super) struct HardwareControl {
    worker: Result<Worker, String>,
    reply: Reply,
    value: Option<u8>,
    revision: u64,
    pending: bool,
    error: Option<String>,
    initial_adjustment: Option<Adjustment>,
}

impl HardwareControl {
    pub(super) fn brightness() -> Self {
        static WORKER: OnceLock<Result<Worker, String>> = OnceLock::new();
        Self::new(
            WORKER
                .get_or_init(|| {
                    Worker::spawn("settings-brightness", |op| {
                        execute_device(
                            op,
                            device::try_get_brightness_percent,
                            device::try_set_brightness_percent,
                        )
                    })
                })
                .clone(),
        )
    }

    pub(super) fn volume() -> Self {
        static WORKER: OnceLock<Result<Worker, String>> = OnceLock::new();
        Self::new(
            WORKER
                .get_or_init(|| {
                    Worker::spawn("settings-volume", |op| {
                        execute_device(
                            op,
                            device::try_get_volume_percent,
                            device::try_set_volume_percent,
                        )
                    })
                })
                .clone(),
        )
    }

    fn new(worker: Result<Worker, String>) -> Self {
        let mut control = Self {
            worker,
            reply: Arc::new(Mutex::new(None)),
            value: None,
            revision: 0,
            pending: false,
            error: None,
            initial_adjustment: None,
        };
        control.request(Operation::Read);
        control
    }

    fn request(&mut self, operation: Operation) {
        self.revision += 1;
        self.pending = true;
        self.error = None;
        let result = self
            .worker
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|worker| {
                worker.submit(Request {
                    revision: self.revision,
                    operation,
                    reply: self.reply.clone(),
                })
            });
        if let Err(error) = result {
            self.pending = false;
            self.error = Some(error);
        }
    }

    pub(super) fn poll(&mut self) -> bool {
        // Polling never waits on the worker, including its completion mailbox.
        let completion = self.reply.try_lock().ok().and_then(|mut slot| slot.take());
        let Some(completion) = completion else {
            return false;
        };
        if completion.revision != self.revision {
            return false;
        }
        self.pending = false;
        match completion.result {
            Ok(value) => {
                self.value = Some(value);
                self.error = None;
                if let Some(adjustment) = self.initial_adjustment.take() {
                    let target = adjustment.apply(value);
                    self.value = Some(target);
                    if target != value {
                        self.request(Operation::Write(target));
                    }
                }
            }
            Err(error) => {
                self.error = Some(error);
                self.initial_adjustment = None;
            }
        }
        true
    }

    pub(super) fn adjust(&mut self, delta: i16) {
        if let Some(value) = self.value {
            let target = (i16::from(value) + delta).clamp(0, 100) as u8;
            if target != value || self.error.is_some() {
                self.value = Some(target);
                self.request(Operation::Write(target));
            }
        } else {
            self.initial_adjustment
                .get_or_insert_with(Adjustment::default)
                .add(delta);
            if !self.pending {
                self.request(Operation::Read);
            }
        }
    }

    pub(super) fn retry(&mut self) {
        self.request(self.value.map_or(Operation::Read, Operation::Write));
    }

    pub(super) fn value(&self) -> Option<u8> {
        self.value
    }
    pub(super) fn is_pending(&self) -> bool {
        self.pending
    }
    pub(super) fn has_error(&self) -> bool {
        self.error.is_some()
    }

    pub(super) fn status(&self) -> String {
        if let Some(error) = &self.error {
            let prefix = if self.value.is_some() {
                "Not saved"
            } else {
                "Unavailable"
            };
            return format!("{prefix} · A: retry · {error}");
        }
        if self.pending {
            return if self.value.is_some() {
                "Saving…"
            } else if self.initial_adjustment.is_some() {
                "Loading… adjustment queued"
            } else {
                "Loading…"
            }
            .into();
        }
        "Left / right to adjust".into()
    }
}

impl Drop for HardwareControl {
    fn drop(&mut self) {
        if let Some(adjustment) = self.initial_adjustment.take() {
            // No waiting on close, including when input preceded the first read.
            // The persistent worker applies this before any reopening read.
            self.request(Operation::Adjust(adjustment));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    type Call = (Operation, mpsc::Sender<Result<u8, String>>);

    fn fake_worker() -> (Worker, mpsc::Receiver<Call>) {
        let (tx, rx) = mpsc::channel();
        let worker = Worker::spawn("fake-settings", move |operation| {
            let (reply, answer) = mpsc::channel();
            tx.send((operation, reply)).unwrap();
            answer.recv_timeout(Duration::from_secs(2)).unwrap()
        })
        .unwrap();
        (worker, rx)
    }

    fn next(calls: &mpsc::Receiver<Call>, expected: Operation) -> mpsc::Sender<Result<u8, String>> {
        let (actual, reply) = calls.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(actual, expected);
        reply
    }

    fn poll_until(control: &mut HardwareControl, predicate: impl Fn(&HardwareControl) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            control.poll();
            if predicate(control) {
                break;
            }
            assert!(Instant::now() < deadline, "worker did not complete");
            std::thread::yield_now();
        }
    }

    #[test]
    fn held_input_coalesces_and_stale_completion_cannot_replace_target() {
        let (worker, calls) = fake_worker();
        let mut control = HardwareControl::new(Ok(worker));
        next(&calls, Operation::Read).send(Ok(50)).unwrap();
        poll_until(&mut control, |c| !c.is_pending());
        control.adjust(10);
        let first = next(&calls, Operation::Write(60));
        for _ in 0..1000 {
            control.adjust(10);
        }
        control.adjust(-10);
        assert_eq!(control.value(), Some(90));
        assert!(control.is_pending());
        first.send(Ok(60)).unwrap();
        let latest = next(&calls, Operation::Write(90));
        control.poll();
        assert_eq!(control.value(), Some(90));
        assert!(control.is_pending());
        latest.send(Ok(90)).unwrap();
        poll_until(&mut control, |c| !c.is_pending());
        assert!(!control.has_error());
        assert!(calls.try_recv().is_err());
    }

    #[test]
    fn initial_input_waits_for_real_level_and_preserves_saturation() {
        let (worker, calls) = fake_worker();
        let mut control = HardwareControl::new(Ok(worker));
        let initial = next(&calls, Operation::Read);
        control.adjust(10);
        control.adjust(-10);
        assert_eq!(control.value(), None);
        assert!(control.status().contains("queued"));
        initial.send(Ok(95)).unwrap();
        poll_until(&mut control, |c| c.value().is_some());
        assert_eq!(control.value(), Some(90));
        next(&calls, Operation::Write(90)).send(Ok(90)).unwrap();
        poll_until(&mut control, |c| !c.is_pending());
    }

    #[test]
    fn read_and_write_errors_are_visible_and_retry_keeps_failed_target() {
        let (worker, calls) = fake_worker();
        let mut control = HardwareControl::new(Ok(worker));
        next(&calls, Operation::Read)
            .send(Err("No mixer".into()))
            .unwrap();
        poll_until(&mut control, |c| c.has_error());
        assert_eq!(control.value(), None);
        assert!(control.status().contains("No mixer"));
        control.retry();
        next(&calls, Operation::Read).send(Ok(90)).unwrap();
        poll_until(&mut control, |c| !c.is_pending());
        control.adjust(10);
        next(&calls, Operation::Write(100))
            .send(Err("Permission denied".into()))
            .unwrap();
        poll_until(&mut control, |c| c.has_error());
        assert_eq!(control.value(), Some(100));
        assert!(control.status().contains("Not saved"));
        control.retry();
        next(&calls, Operation::Write(100)).send(Ok(100)).unwrap();
        poll_until(&mut control, |c| !c.is_pending());
        assert!(!control.has_error());
    }

    #[test]
    fn closing_settings_preserves_latest_write_before_reopening_read() {
        let (worker, calls) = fake_worker();
        let mut control = HardwareControl::new(Ok(worker.clone()));
        next(&calls, Operation::Read).send(Ok(50)).unwrap();
        poll_until(&mut control, |c| !c.is_pending());
        control.adjust(10);
        let first = next(&calls, Operation::Write(60));
        control.adjust(10);
        drop(control);
        let mut reopened = HardwareControl::new(Ok(worker));
        first.send(Ok(60)).unwrap();
        next(&calls, Operation::Write(70)).send(Ok(70)).unwrap();
        next(&calls, Operation::Read).send(Ok(70)).unwrap();
        poll_until(&mut reopened, |c| !c.is_pending());
        assert_eq!(reopened.value(), Some(70));
    }

    #[test]
    fn closing_during_initial_read_still_applies_queued_adjustments() {
        let (worker, calls) = fake_worker();
        let mut control = HardwareControl::new(Ok(worker.clone()));
        let initial = next(&calls, Operation::Read);
        control.adjust(10);
        control.adjust(-10);
        let mut adjustment = Adjustment::default();
        adjustment.add(10);
        adjustment.add(-10);
        drop(control);
        let mut reopened = HardwareControl::new(Ok(worker));
        initial.send(Ok(95)).unwrap();
        next(&calls, Operation::Adjust(adjustment))
            .send(Ok(90))
            .unwrap();
        next(&calls, Operation::Read).send(Ok(90)).unwrap();
        poll_until(&mut reopened, |c| !c.is_pending());
        assert_eq!(reopened.value(), Some(90));
    }

    #[test]
    fn deferred_changes_use_real_level_and_report_write_failures() {
        let mut adjustment = Adjustment::default();
        adjustment.add(10);
        adjustment.add(-10);
        let result = execute_device(
            Operation::Adjust(adjustment),
            || Ok(95),
            |target| {
                assert_eq!(target, 90);
                Err("Permission denied".into())
            },
        );
        assert_eq!(result, Err("Permission denied".into()));
        let result = execute_device(
            Operation::Write(70),
            || panic!("writes must not re-read"),
            |_| Ok(()),
        );
        assert_eq!(result, Ok(70));
    }

    #[test]
    fn stale_read_and_error_do_not_override_newer_target() {
        let (worker, calls) = fake_worker();
        let mut control = HardwareControl::new(Ok(worker));
        let initial = next(&calls, Operation::Read);
        // A cached target with a newer request, as with a delayed refresh.
        control.value = Some(50);
        control.adjust(10);
        initial.send(Err("Old read failed".into())).unwrap();
        let write = next(&calls, Operation::Write(60));
        control.poll();
        assert_eq!(control.value(), Some(60));
        assert!(!control.has_error());
        assert!(control.is_pending());
        write.send(Ok(60)).unwrap();
        poll_until(&mut control, |c| !c.is_pending());
    }

    #[test]
    fn queued_adjustments_match_each_step_for_all_starting_levels() {
        let mut adjustment = Adjustment::default();
        let mut expected: Vec<u8> = (0..=100).collect();
        for delta in [
            10, -10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, -10, -10, -10, -10, -10, -10, -10,
            -10, -10, -10, -10, -10, 10, 10, -10,
        ] {
            adjustment.add(delta);
            for (base, value) in expected.iter_mut().enumerate() {
                *value = (i16::from(*value) + delta).clamp(0, 100) as u8;
                assert_eq!(adjustment.apply(base as u8), *value);
            }
        }
    }

    #[test]
    fn worker_start_failure_is_visible_without_remaining_busy() {
        let control = HardwareControl::new(Err("Thread limit reached".into()));
        assert!(!control.is_pending());
        assert!(control.status().contains("Thread limit reached"));
    }
}
