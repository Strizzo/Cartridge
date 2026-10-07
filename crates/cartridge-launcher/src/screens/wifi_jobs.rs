//! One in-flight Wi-Fi job shared by all screen instances. Only the UI accesses
//! the coordinator; workers send owned results and never acquire its mutex.
use cartridge_net::wifi::{WifiManager, WifiNetwork, WifiStatus, merge_saved_connections};
use std::sync::{
    Arc, Mutex, OnceLock,
    mpsc::{self, Receiver, Sender, TryRecvError},
};

pub(super) type SharedJobs = Arc<Mutex<WifiJobs>>;
type Worker = dyn Fn(Operation, Sender<Event>) + Send + Sync;

pub(super) fn shared_jobs() -> SharedJobs {
    static JOBS: OnceLock<SharedJobs> = OnceLock::new();
    JOBS.get_or_init(|| Arc::new(Mutex::new(WifiJobs::new(Arc::new(run)))))
        .clone()
}

// Intentionally no Debug: credentials must never appear in diagnostics.
pub(super) enum Operation {
    Refresh,
    ConnectSaved { ssid: String, offer_password: bool },
    ConnectPassword { ssid: String, password: String },
    Disconnect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pending {
    Scan,
    Connect,
    Disconnect,
    Refresh,
}

impl Pending {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Scan => "Scanning Wi-Fi...",
            Self::Connect => "Connecting...",
            Self::Disconnect => "Disconnecting...",
            Self::Refresh => "Refreshing Wi-Fi...",
        }
    }
}

#[derive(Clone)]
pub(super) struct View {
    pub status: WifiStatus,
    pub networks: Vec<WifiNetwork>,
    pub scan_error: Option<String>,
    pub message: Option<String>,
    pub password_retry_ssid: Option<String>,
    pub pending: Option<Pending>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            status: WifiStatus::Unknown,
            networks: vec![],
            scan_error: None,
            message: None,
            password_retry_ssid: None,
            pending: None,
        }
    }
}

pub(super) enum Event {
    Outcome {
        result: Result<(), String>,
        success: String,
        password_retry_ssid: Option<String>,
    },
    Snapshot {
        status: WifiStatus,
        networks: Vec<WifiNetwork>,
        scan_error: Option<String>,
    },
}

pub(super) struct WifiJobs {
    pub view: View,
    pub revision: u64,
    receiver: Option<Receiver<Event>>,
    worker: Arc<Worker>,
}

impl WifiJobs {
    pub(super) fn new(worker: Arc<Worker>) -> Self {
        Self {
            view: View::default(),
            revision: 0,
            receiver: None,
            worker,
        }
    }

    pub(super) fn is_busy(&self) -> bool {
        self.receiver.is_some()
    }

    pub(super) fn start(&mut self, operation: Operation) -> bool {
        // A completed but unconsumed result still owns the slot. Closing a
        // screen cannot abandon that slot or start a competing nmcli command.
        if self.is_busy() {
            return false;
        }
        self.view.pending = Some(match &operation {
            Operation::Refresh => Pending::Scan,
            Operation::ConnectSaved { .. } | Operation::ConnectPassword { .. } => Pending::Connect,
            Operation::Disconnect => Pending::Disconnect,
        });
        if !matches!(operation, Operation::Refresh) {
            self.view.message = None;
            self.view.password_retry_ssid = None;
        }
        let (sender, receiver) = mpsc::channel();
        let worker = self.worker.clone();
        self.receiver = Some(receiver);
        if std::thread::Builder::new()
            .name("wifi-screen".into())
            .spawn(move || worker(operation, sender))
            .is_err()
        {
            self.receiver = None;
            self.view.pending = None;
            self.view.message = Some("Could not start Wi-Fi worker. Press Y to retry.".into());
        }
        self.revision += 1;
        true
    }

    pub(super) fn poll(&mut self) {
        while let Some(receiver) = &self.receiver {
            match receiver.try_recv() {
                Ok(Event::Outcome {
                    result,
                    success,
                    password_retry_ssid,
                }) => {
                    self.view.message = Some(match result {
                        Ok(()) => success,
                        Err(error) => {
                            self.view.password_retry_ssid = password_retry_ssid;
                            if self.view.password_retry_ssid.is_some() {
                                format!(
                                    "Error: {error}. Select this network and press A to enter a password."
                                )
                            } else {
                                format!("Error: {error}")
                            }
                        }
                    });
                    self.view.pending = Some(Pending::Refresh);
                }
                Ok(Event::Snapshot {
                    status,
                    networks,
                    scan_error,
                }) => {
                    self.view.status = status;
                    self.view.networks = networks;
                    self.view.scan_error = scan_error;
                    // Refresh never clears the outcome of a connection attempt.
                    self.view.pending = None;
                    self.receiver = None;
                }
                Err(TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.view.pending = None;
                    let error = "Wi-Fi worker stopped unexpectedly. Press Y to retry.";
                    self.view.message = Some(match self.view.message.take() {
                        Some(message) => format!("{message} {error}"),
                        None => error.into(),
                    });
                }
                Err(TryRecvError::Empty) => break,
            }
            self.revision += 1;
        }
    }
}

fn run(operation: Operation, sender: Sender<Event>) {
    // Construction and every WifiManager call stay on this worker. No changes
    // to the manager's driver, NetworkManager, or credential policy are needed.
    let manager = WifiManager::new();
    let outcome = match operation {
        Operation::Refresh => None,
        Operation::ConnectSaved {
            ssid,
            offer_password,
        } => Some(Event::Outcome {
            result: manager.connect(&ssid),
            success: format!("Connected to {ssid}"),
            password_retry_ssid: offer_password.then_some(ssid),
        }),
        Operation::ConnectPassword { ssid, password } => {
            // Do not retain credentials in shared state or echo them in errors.
            let result = manager
                .connect_with_password(&ssid, &password)
                .map_err(|error| {
                    if password.is_empty() {
                        error
                    } else {
                        error.replace(&password, "[redacted]")
                    }
                });
            Some(Event::Outcome {
                result,
                success: format!("Connected to {ssid}"),
                password_retry_ssid: None,
            })
        }
        Operation::Disconnect => Some(Event::Outcome {
            result: manager.disconnect(),
            success: "Disconnected".into(),
            password_retry_ssid: None,
        }),
    };
    if let Some(outcome) = outcome {
        // Report connection errors before potentially slow status/scan queries.
        let _ = sender.send(outcome);
    }
    let status = manager.status();
    let scanned = manager.scan_networks();
    let saved = manager.saved_connections();
    let (networks, scan_error) = match scanned {
        Ok(networks) => (merge_saved_connections(networks, &saved), None),
        Err(error) => (merge_saved_connections(vec![], &saved), Some(error)),
    };
    // Final event: all manager work has finished before the slot is released.
    let _ = sender.send(Event::Snapshot {
        status,
        networks,
        scan_error,
    });
}
