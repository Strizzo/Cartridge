use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// Well-known SSH key filenames to scan for, in priority order.
const KEY_NAMES: &[&str] = &["id_ed25519", "id_rsa", "id_ecdsa"];

/// An SSH tunnel wrapping a child `ssh` process.
///
/// Opens an SSH port-forward (`-L`) from an ephemeral local port to a remote
/// port, using the system `ssh` binary. The tunnel is killed on `Drop`.
pub struct SshTunnel {
    child: Option<Child>,
    local_port: u16,
}

impl SshTunnel {
    /// Open a new SSH tunnel.
    ///
    /// Finds an available local port, spawns `ssh -N -L ...`, then polls the
    /// local port for up to 5 seconds to confirm the tunnel is ready.
    ///
    /// If `key_path` is provided, it is used directly. Otherwise, if `key_dir`
    /// is provided, the directory is scanned for well-known key files
    /// (`id_ed25519`, `id_rsa`, `id_ecdsa`). As a final fallback, `~/.ssh/`
    /// is scanned.
    pub fn open(
        host: &str,
        user: &str,
        key_path: Option<&str>,
        key_dir: Option<&str>,
        remote_port: u16,
    ) -> Result<Self, String> {
        // Find an available local port
        let local_port = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("Failed to find free port: {e}"))?
            .local_addr()
            .map_err(|e| format!("Failed to get local addr: {e}"))?
            .port();

        let forward = format!("{local_port}:localhost:{remote_port}");
        let destination = if user.is_empty() {
            host.to_string()
        } else {
            format!("{user}@{host}")
        };

        // Resolve key: explicit path > scan key_dir > scan ~/.ssh/
        let resolved_key = Self::resolve_key(key_path, key_dir);

        let mut args: Vec<&str> = vec![
            "-N",
            "-o", "BatchMode=yes",
            "-o", "StrictHostKeyChecking=accept-new",
            "-o", "ConnectTimeout=10",
            "-L", &forward,
        ];

        let key_string;
        if let Some(ref key) = resolved_key {
            key_string = key.to_string_lossy().to_string();
            args.push("-i");
            args.push(&key_string);
        }

        args.push(&destination);

        let child = Command::new("ssh")
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn ssh: {e}"))?;

        let mut tunnel = SshTunnel {
            child: Some(child),
            local_port,
        };

        // Poll local port for up to 5 seconds to confirm tunnel is ready
        let start = Instant::now();
        let timeout = Duration::from_secs(5);

        loop {
            // Check if ssh process has exited (meaning it failed)
            if let Some(ref mut child) = tunnel.child {
                if let Ok(Some(status)) = child.try_wait() {
                    // Process exited — read stderr for error message
                    let stderr = child
                        .stderr
                        .take()
                        .and_then(|mut s| {
                            use std::io::Read;
                            let mut buf = String::new();
                            s.read_to_string(&mut buf).ok().map(|_| buf)
                        })
                        .unwrap_or_default();
                    let msg = stderr.trim();
                    tunnel.child = None;
                    return Err(format!(
                        "SSH exited with {status}: {msg}"
                    ));
                }
            }

            // Try to connect to the local forwarded port
            if std::net::TcpStream::connect_timeout(
                &format!("127.0.0.1:{local_port}").parse().unwrap(),
                Duration::from_millis(200),
            )
            .is_ok()
            {
                return Ok(tunnel);
            }

            if start.elapsed() >= timeout {
                tunnel.close();
                return Err("SSH tunnel timed out waiting for port forward".to_string());
            }

            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Resolve which SSH key to use.
    fn resolve_key(key_path: Option<&str>, key_dir: Option<&str>) -> Option<PathBuf> {
        // 1. Explicit key path
        if let Some(kp) = key_path {
            let p = Path::new(kp);
            if p.exists() {
                return Some(p.to_path_buf());
            }
        }

        // 2. Scan provided key directory
        if let Some(dir) = key_dir {
            if let Some(found) = Self::find_key_in_dir(Path::new(dir)) {
                return Some(found);
            }
        }

        // 3. Fallback: scan ~/.ssh/
        if let Ok(home) = std::env::var("HOME") {
            let ssh_dir = Path::new(&home).join(".ssh");
            if let Some(found) = Self::find_key_in_dir(&ssh_dir) {
                return Some(found);
            }
        }

        None
    }

    /// Scan a directory for well-known SSH key files.
    fn find_key_in_dir(dir: &Path) -> Option<PathBuf> {
        for name in KEY_NAMES {
            let candidate = dir.join(name);
            if candidate.exists() {
                log::info!("Found SSH key: {}", candidate.display());
                return Some(candidate);
            }
        }
        None
    }

    /// The local port that forwards to the remote port.
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// Check whether the SSH process is still running.
    pub fn is_alive(&mut self) -> bool {
        match self.child {
            Some(ref mut child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Kill the SSH process.
    pub fn close(&mut self) {
        if let Some(ref mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for SshTunnel {
    fn drop(&mut self) {
        self.close();
    }
}
