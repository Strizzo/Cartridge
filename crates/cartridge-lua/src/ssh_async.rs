//! Owned SSH connection state. Workers receive Rust options and channels only;
//! no Lua VM, table, callback, or RefCell crosses the thread boundary.
//!
//! One worker and a one-slot queue bound cancelled/retried opens. Cancellation
//! does not wait for the blocking opener: its receiver is dropped immediately,
//! so an already queued or late successful tunnel is closed by its own Drop.
//! Retain at most 32 deferred cancellation notifications, evicting the oldest
//! if a caller never polls. Poll also returns the current completion, which is
//! never evicted by this cap; normal cancellation ordering remains FIFO.

use cartridge_net::SshTunnel;
use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread;

const MAX_DEFERRED_COMPLETIONS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpenOptions {
    pub host: String,
    pub user: String,
    pub key_path: Option<String>,
    pub key_dir: Option<String>,
    pub remote_port: u16,
}

pub(crate) trait Tunnel: Send + 'static {
    fn local_port(&self) -> u16;
    fn is_alive(&mut self) -> bool;
}

pub(crate) enum TunnelHandle {
    Real(SshTunnel),
    Fixture(u16),
}

impl Tunnel for TunnelHandle {
    fn local_port(&self) -> u16 {
        match self {
            Self::Real(tunnel) => tunnel.local_port(),
            Self::Fixture(port) => *port,
        }
    }

    fn is_alive(&mut self) -> bool {
        match self {
            Self::Real(tunnel) => tunnel.is_alive(),
            Self::Fixture(_) => true,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ConnectError {
    AlreadyPending,
    Cancelled,
    WorkerBusy,
    WorkerDisconnected,
    TunnelExited,
    SpawnFailed(String),
    OpenFailed(String),
    IdExhausted,
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyPending => {
                f.write_str("SSH connection already pending; poll or close first")
            }
            Self::Cancelled => f.write_str("SSH connection cancelled"),
            Self::WorkerBusy => {
                f.write_str("SSH worker is finishing a cancelled connection; retry shortly")
            }
            Self::WorkerDisconnected => f.write_str("SSH connection worker disconnected; retry"),
            Self::TunnelExited => f.write_str("SSH tunnel exited before becoming active; retry"),
            Self::SpawnFailed(error) => write!(f, "Failed to start SSH worker: {error}"),
            Self::OpenFailed(error) => f.write_str(error),
            Self::IdExhausted => f.write_str("SSH request IDs exhausted"),
        }
    }
}

impl std::error::Error for ConnectError {}

pub(crate) struct Completion {
    pub id: u64,
    pub result: Result<u16, ConnectError>,
}

type OpenResult<T> = Result<T, ConnectError>;
type Opener<T> = Arc<dyn Fn(OpenOptions) -> Result<T, String> + Send + Sync>;

struct Job<T> {
    options: OpenOptions,
    cancelled: Arc<AtomicBool>,
    reply: mpsc::Sender<OpenResult<T>>,
}

struct Pending<T> {
    id: u64,
    cancelled: Arc<AtomicBool>,
    receiver: Receiver<OpenResult<T>>,
}

pub(crate) struct Connector<T: Tunnel> {
    opener: Opener<T>,
    immediate: bool,
    worker: Option<SyncSender<Job<T>>>,
    pending: Option<Pending<T>>,
    active: Option<T>,
    next_id: u64,
    completions: VecDeque<Completion>,
}

impl<T: Tunnel> Connector<T> {
    pub fn new(opener: Opener<T>) -> Self {
        Self {
            opener,
            immediate: false,
            worker: None,
            pending: None,
            active: None,
            next_id: 0,
            completions: VecDeque::with_capacity(MAX_DEFERRED_COMPLETIONS),
        }
    }

    /// Only the explicit simulator fixture uses this deterministic path.
    /// It still queues an owned result for poll; it never activates inline.
    pub fn immediate(opener: Opener<T>) -> Self {
        let mut connector = Self::new(opener);
        connector.immediate = true;
        connector
    }

    fn spawn_worker(&mut self) -> Result<(), ConnectError> {
        let (sender, receiver) = mpsc::sync_channel::<Job<T>>(1);
        let opener = self.opener.clone();
        thread::Builder::new()
            .name("cartridge-ssh".into())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    if job.cancelled.load(Ordering::Acquire) {
                        continue;
                    }
                    let result = opener(job.options).map_err(ConnectError::OpenFailed);
                    if job.cancelled.load(Ordering::Acquire) {
                        // A late success is owned here and must be dropped.
                        drop(result);
                    } else {
                        // Cancellation can race this send. SendError owns the
                        // rejected result; dropping it closes a late tunnel.
                        let _ = job.reply.send(result);
                    }
                }
            })
            .map_err(|error| ConnectError::SpawnFailed(error.to_string()))?;
        self.worker = Some(sender);
        Ok(())
    }

    pub fn start(&mut self, options: OpenOptions) -> Result<u64, ConnectError> {
        if self.pending.is_some() {
            return Err(ConnectError::AlreadyPending);
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or(ConnectError::IdExhausted)?;
        self.next_id = id;
        let (reply, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        if self.immediate {
            let _ = reply.send((self.opener)(options).map_err(ConnectError::OpenFailed));
        } else {
            let job = Job {
                options,
                cancelled: cancelled.clone(),
                reply,
            };
            let spawn = if self.worker.is_none() {
                self.spawn_worker()
            } else {
                Ok(())
            };
            if let Err(error) = spawn {
                let _ = job.reply.send(Err(error));
            } else {
                match self.worker.as_ref().unwrap().try_send(job) {
                    Ok(()) => {}
                    Err(TrySendError::Full(job)) => {
                        let _ = job.reply.send(Err(ConnectError::WorkerBusy));
                    }
                    Err(TrySendError::Disconnected(job)) => {
                        self.worker = None;
                        let _ = job.reply.send(Err(ConnectError::WorkerDisconnected));
                    }
                }
            }
        }
        self.pending = Some(Pending {
            id,
            cancelled,
            receiver,
        });
        Ok(id)
    }

    pub fn poll(&mut self) -> Vec<Completion> {
        let mut completions: Vec<_> = self.completions.drain(..).collect();
        if let Some(pending) = &self.pending {
            let result = match pending.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    // A panicking/disconnected worker must permit a fresh retry.
                    self.worker = None;
                    Some(Err(ConnectError::WorkerDisconnected))
                }
            };
            if let Some(result) = result {
                let pending = self.pending.take().unwrap();
                let result = match result {
                    Ok(mut tunnel) => {
                        if tunnel.is_alive() {
                            let port = tunnel.local_port();
                            self.active = Some(tunnel); // Drops/closes the previous tunnel.
                            Ok(port)
                        } else {
                            Err(ConnectError::TunnelExited)
                        }
                    }
                    Err(error) => Err(error),
                };
                // Deliver the current result even when deferred cancellations
                // filled the cap. No tunnel ownership lives in the FIFO.
                completions.push(Completion {
                    id: pending.id,
                    result,
                });
            }
        }
        completions
    }

    fn cancel_pending(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.cancelled.store(true, Ordering::Release);
            if self.completions.len() == MAX_DEFERRED_COMPLETIONS {
                self.completions.pop_front();
            }
            self.completions.push_back(Completion {
                id: pending.id,
                result: Err(ConnectError::Cancelled),
            });
            // Dropping the receiver also closes a success already waiting in it.
        }
    }

    /// Compatibility path: intentionally synchronous, using the same ownership.
    pub fn open_sync(&mut self, options: OpenOptions) -> Result<u16, ConnectError> {
        self.cancel_pending();
        let tunnel = (self.opener)(options).map_err(ConnectError::OpenFailed)?;
        let port = tunnel.local_port();
        self.active = Some(tunnel);
        Ok(port)
    }

    pub fn close(&mut self) {
        self.cancel_pending();
        self.active = None;
    }

    pub fn is_alive(&mut self) -> bool {
        if self.active.as_mut().is_some_and(Tunnel::is_alive) {
            true
        } else {
            self.active = None;
            false
        }
    }
}

impl<T: Tunnel> Drop for Connector<T> {
    fn drop(&mut self) {
        self.close();
        // No join: an in-progress open cannot hold up Lua VM destruction.
        self.worker = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    #[derive(Debug)]
    struct MockTunnel {
        port: u16,
        alive: Arc<AtomicBool>,
        drops: Arc<AtomicUsize>,
        dropped: Option<mpsc::Sender<u16>>,
    }

    impl Tunnel for MockTunnel {
        fn local_port(&self) -> u16 {
            self.port
        }
        fn is_alive(&mut self) -> bool {
            self.alive.load(Ordering::Acquire)
        }
    }

    impl Drop for MockTunnel {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::AcqRel);
            if let Some(sender) = &self.dropped {
                let _ = sender.send(self.port);
            }
        }
    }

    fn options(host: &str) -> OpenOptions {
        OpenOptions {
            host: host.into(),
            user: "fixture-user".into(),
            key_path: Some("/never-read/private-key".into()),
            key_dir: Some("/never-read/key-directory".into()),
            remote_port: 8766,
        }
    }

    fn tunnel(port: u16, drops: &Arc<AtomicUsize>) -> MockTunnel {
        MockTunnel {
            port,
            alive: Arc::new(AtomicBool::new(true)),
            drops: drops.clone(),
            dropped: None,
        }
    }

    fn controlled() -> (
        Connector<MockTunnel>,
        Receiver<OpenOptions>,
        mpsc::Sender<Result<MockTunnel, String>>,
    ) {
        let (entered, calls) = mpsc::channel();
        let (reply, responses) = mpsc::channel();
        let responses = Mutex::new(responses);
        let caller = thread::current().id();
        let connector = Connector::new(Arc::new(move |opts| {
            assert_ne!(
                thread::current().id(),
                caller,
                "opener ran on the Lua caller thread"
            );
            entered.send(opts).unwrap();
            responses
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(3))
                .expect("test must release its mock opener")
        }));
        (connector, calls, reply)
    }

    fn wait_poll(connector: &mut Connector<MockTunnel>) -> Vec<Completion> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let results = connector.poll();
            if !results.is_empty() {
                return results;
            }
            assert!(Instant::now() < deadline, "mock SSH completion timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn opener_runs_off_thread_and_poll_does_not_block_or_duplicate() {
        let drops = Arc::new(AtomicUsize::new(0));
        let (mut connector, calls, reply) = controlled();
        let id = connector.start(options("mock-host")).unwrap();
        assert_eq!(
            calls.recv_timeout(Duration::from_secs(3)).unwrap(),
            options("mock-host")
        );
        assert!(connector.poll().is_empty());
        assert!(!connector.is_alive());
        assert_eq!(
            connector.start(options("other-host")).unwrap_err(),
            ConnectError::AlreadyPending
        );
        let opened = tunnel(12345, &drops);
        let alive = opened.alive.clone();
        reply.send(Ok(opened)).unwrap();
        let results = wait_poll(&mut connector);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);
        assert_eq!(results[0].result, Ok(12345));
        assert!(connector.poll().is_empty());
        assert!(connector.is_alive());
        alive.store(false, Ordering::Release);
        assert!(
            !connector.is_alive(),
            "active liveness must reflect the child, not request state"
        );
        assert_eq!(drops.load(Ordering::Acquire), 1);
    }

    #[test]
    fn close_reports_cancellation_once_and_drops_a_late_success() {
        let drops = Arc::new(AtomicUsize::new(0));
        let (mut connector, calls, reply) = controlled();
        let id = connector.start(options("late-host")).unwrap();
        calls.recv_timeout(Duration::from_secs(3)).unwrap();
        connector.close();
        connector.close();
        let results = connector.poll();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);
        assert_eq!(results[0].result, Err(ConnectError::Cancelled));
        let (dropped, closed) = mpsc::channel();
        let mut late = tunnel(12345, &drops);
        late.dropped = Some(dropped);
        reply.send(Ok(late)).unwrap();
        assert_eq!(closed.recv_timeout(Duration::from_secs(3)).unwrap(), 12345);
        assert_eq!(drops.load(Ordering::Acquire), 1);
        assert!(connector.poll().is_empty());
        assert!(!connector.is_alive());
    }

    #[test]
    fn dropping_connection_state_does_not_wait_and_closes_late_tunnel() {
        let drops = Arc::new(AtomicUsize::new(0));
        let (mut connector, calls, reply) = controlled();
        connector.start(options("destroyed-vm")).unwrap();
        calls.recv_timeout(Duration::from_secs(3)).unwrap();
        drop(connector); // Would deadlock against the gate if Drop joined.
        let (dropped, closed) = mpsc::channel();
        let mut late = tunnel(54321, &drops);
        late.dropped = Some(dropped);
        reply.send(Ok(late)).unwrap();
        assert_eq!(closed.recv_timeout(Duration::from_secs(3)).unwrap(), 54321);
        assert_eq!(drops.load(Ordering::Acquire), 1);
    }

    #[test]
    fn cancellation_also_closes_success_already_queued_before_poll() {
        let drops = Arc::new(AtomicUsize::new(0));
        let counter = drops.clone();
        let mut connector = Connector::immediate(Arc::new(move |_| Ok(tunnel(8766, &counter))));
        let id = connector.start(options("queued-success")).unwrap();
        assert!(!connector.is_alive());
        connector.close();
        assert_eq!(drops.load(Ordering::Acquire), 1);
        let results = connector.poll();
        assert_eq!(results[0].id, id);
        assert_eq!(results[0].result, Err(ConnectError::Cancelled));
    }

    #[test]
    fn many_cancelled_opens_without_polling_bound_results_and_drop_every_tunnel() {
        let drops = Arc::new(AtomicUsize::new(0));
        let counter = drops.clone();
        let mut connector = Connector::immediate(Arc::new(move |options| {
            Ok(tunnel(options.remote_port, &counter))
        }));
        connector.open_sync(options("existing-active")).unwrap();
        assert!(connector.is_alive());
        let attempts = MAX_DEFERRED_COMPLETIONS * 128;
        for expected_id in 1..=attempts as u64 {
            assert_eq!(
                connector.start(options("cancel-without-poll")).unwrap(),
                expected_id
            );
            connector.close();
            assert!(connector.completions.len() <= MAX_DEFERRED_COMPLETIONS);
            assert!(connector.pending.is_none());
            assert!(!connector.is_alive());
            assert_eq!(drops.load(Ordering::Acquire), expected_id as usize + 1);
        }
        assert_eq!(connector.completions.len(), MAX_DEFERRED_COMPLETIONS);
        // A current success must survive even after the notification FIFO fills.
        let current_id = connector.start(options("current-success")).unwrap();
        let results = connector.poll();
        assert_eq!(results.len(), MAX_DEFERRED_COMPLETIONS + 1);
        let first_retained_id = attempts as u64 - MAX_DEFERRED_COMPLETIONS as u64 + 1;
        for (offset, result) in results[..MAX_DEFERRED_COMPLETIONS].iter().enumerate() {
            assert_eq!(result.id, first_retained_id + offset as u64);
            assert_eq!(result.result, Err(ConnectError::Cancelled));
        }
        assert_eq!(results.last().unwrap().id, current_id);
        assert_eq!(results.last().unwrap().result, Ok(8766));
        assert!(connector.completions.is_empty());
        assert!(
            connector.poll().is_empty(),
            "notifications must still drain once"
        );
        assert!(connector.is_alive());
        assert_eq!(drops.load(Ordering::Acquire), attempts + 1);
        connector.close();
        assert_eq!(drops.load(Ordering::Acquire), attempts + 2);
    }

    #[test]
    fn receiver_disconnection_is_typed_and_retry_spawns_a_fresh_worker() {
        let drops = Arc::new(AtomicUsize::new(0));
        let counter = drops.clone();
        let mut connector = Connector::new(Arc::new(move |_| Ok(tunnel(8766, &counter))));
        let (reply, receiver) = mpsc::channel();
        drop(reply);
        let (worker, jobs) = mpsc::sync_channel(1);
        drop(jobs);
        connector.worker = Some(worker);
        connector.next_id = 1;
        connector.pending = Some(Pending {
            id: 1,
            cancelled: Arc::new(AtomicBool::new(false)),
            receiver,
        });
        let results = connector.poll();
        assert_eq!(results[0].result, Err(ConnectError::WorkerDisconnected));
        assert!(connector.worker.is_none());
        assert_eq!(connector.start(options("retry")).unwrap(), 2);
        assert_eq!(wait_poll(&mut connector)[0].result, Ok(8766));
    }

    #[test]
    fn cancelled_retries_have_a_bounded_worker_queue() {
        let drops = Arc::new(AtomicUsize::new(0));
        let (mut connector, calls, reply) = controlled();
        let first = connector.start(options("blocked")).unwrap();
        calls.recv_timeout(Duration::from_secs(3)).unwrap();
        connector.close();
        let second = connector.start(options("queued-cancelled")).unwrap();
        connector.close();
        let third = connector.start(options("queue-full")).unwrap();
        let results = connector.poll();
        assert_eq!(results.len(), 3);
        assert_eq!(
            (results[0].id, &results[0].result),
            (first, &Err(ConnectError::Cancelled))
        );
        assert_eq!(
            (results[1].id, &results[1].result),
            (second, &Err(ConnectError::Cancelled))
        );
        assert_eq!(
            (results[2].id, &results[2].result),
            (third, &Err(ConnectError::WorkerBusy))
        );
        let (dropped, closed) = mpsc::channel();
        let mut late = tunnel(12345, &drops);
        late.dropped = Some(dropped);
        reply.send(Ok(late)).unwrap();
        closed.recv_timeout(Duration::from_secs(3)).unwrap();
        drop(connector);
        assert!(
            calls.recv_timeout(Duration::from_secs(3)).is_err(),
            "cancelled queued work called the opener"
        );
    }

    #[test]
    fn failures_preserve_existing_tunnel_and_successful_replacement_closes_it() {
        let drops = Arc::new(AtomicUsize::new(0));
        let counter = drops.clone();
        let mut connector = Connector::immediate(Arc::new(move |opts| {
            if opts.host == "fail" {
                Err("mock connection refused".into())
            } else {
                Ok(tunnel(opts.remote_port, &counter))
            }
        }));
        connector.open_sync(options("existing")).unwrap();
        connector.start(options("fail")).unwrap();
        assert_eq!(
            connector.poll()[0].result,
            Err(ConnectError::OpenFailed("mock connection refused".into()))
        );
        assert!(connector.is_alive());
        assert_eq!(drops.load(Ordering::Acquire), 0);
        let mut replacement = options("replacement");
        replacement.remote_port = 9000;
        connector.start(replacement).unwrap();
        assert_eq!(drops.load(Ordering::Acquire), 0, "poll owns promotion");
        assert_eq!(connector.poll()[0].result, Ok(9000));
        assert_eq!(drops.load(Ordering::Acquire), 1);
        connector.close();
        assert_eq!(drops.load(Ordering::Acquire), 2);
    }

    #[test]
    fn an_exited_result_is_not_promoted_and_legacy_open_cancels_pending() {
        let drops = Arc::new(AtomicUsize::new(0));
        let counter = drops.clone();
        let mut connector = Connector::immediate(Arc::new(move |opts| {
            let result = tunnel(opts.remote_port, &counter);
            if opts.host == "dead" {
                result.alive.store(false, Ordering::Release);
            }
            Ok(result)
        }));
        connector.start(options("dead")).unwrap();
        assert_eq!(connector.poll()[0].result, Err(ConnectError::TunnelExited));
        assert_eq!(drops.load(Ordering::Acquire), 1);
        assert!(!connector.is_alive());
        let id = connector.start(options("cancel-for-legacy")).unwrap();
        assert_eq!(connector.open_sync(options("legacy")).unwrap(), 8766);
        assert!(connector.is_alive());
        let results = connector.poll();
        assert_eq!(results[0].id, id);
        assert_eq!(results[0].result, Err(ConnectError::Cancelled));
        assert_eq!(drops.load(Ordering::Acquire), 2);
    }

    #[test]
    fn lua_state_drop_closes_an_active_tunnel() {
        let drops = Arc::new(AtomicUsize::new(0));
        let counter = drops.clone();
        let mut connector = Connector::immediate(Arc::new(move |_| Ok(tunnel(8766, &counter))));
        connector.start(options("active")).unwrap();
        connector.poll();
        drop(connector);
        assert_eq!(drops.load(Ordering::Acquire), 1);
    }

    #[test]
    fn disconnected_submission_is_a_poll_error_not_a_lost_id() {
        let mut connector =
            Connector::new(Arc::new(|_| Err::<MockTunnel, _>("unused mock".into())));
        let (worker, jobs) = mpsc::sync_channel(1);
        drop(jobs);
        connector.worker = Some(worker);
        let id = connector.start(options("disconnected")).unwrap();
        let results = connector.poll();
        assert_eq!(results[0].id, id);
        assert_eq!(results[0].result, Err(ConnectError::WorkerDisconnected));
        assert!(connector.worker.is_none());
        connector.next_id = u64::MAX;
        assert_eq!(
            connector.start(options("overflow")).unwrap_err(),
            ConnectError::IdExhausted
        );
    }
}
