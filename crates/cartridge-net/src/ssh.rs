use std::net::TcpListener;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

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
    pub fn open(
        host: &str,
        user: &str,
        key_path: Option<&str>,
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

        let mut args: Vec<&str> = vec![
            "-N",
            "-o", "BatchMode=yes",
            "-o", "StrictHostKeyChecking=accept-new",
            "-o", "ConnectTimeout=10",
            "-L", &forward,
        ];

        let key_string;
        if let Some(key) = key_path {
            key_string = key.to_string();
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
