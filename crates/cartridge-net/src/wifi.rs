/// A visible WiFi network from scanning.
#[derive(Debug, Clone)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: u8,
    pub security: String,
    pub is_saved: bool,
}

/// Current WiFi connection state.
#[derive(Debug, Clone)]
pub enum WifiStatus {
    Connected { ssid: String, signal: u8 },
    Disconnected,
    Unknown,
}

/// WiFi manager wrapping nmcli commands.
pub struct WifiManager;

impl WifiManager {
    pub fn new() -> Self {
        Self
    }

    pub fn status(&self) -> WifiStatus {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;

            // Primary: use nmcli to check active WiFi connection
            if let Ok(output) = Command::new("nmcli")
                .args(["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "dev", "status"])
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    let parts: Vec<&str> = line.splitn(4, ':').collect();
                    if parts.len() >= 4
                        && parts[1] == "wifi"
                        && parts[2] == "connected"
                        && !parts[3].is_empty()
                    {
                        let ssid = parts[3].to_string();
                        let signal = read_signal_strength();
                        return WifiStatus::Connected { ssid, signal };
                    }
                }
            }

            // Fallback: try iwgetid
            if let Ok(output) = Command::new("iwgetid").arg("-r").output() {
                let ssid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !ssid.is_empty() {
                    let signal = read_signal_strength();
                    return WifiStatus::Connected { ssid, signal };
                }
            }

            WifiStatus::Disconnected
        }
        #[cfg(not(target_os = "linux"))]
        {
            WifiStatus::Connected {
                ssid: "MockNetwork".to_string(),
                signal: 75,
            }
        }
    }

    pub fn scan_networks(&self) -> Vec<WifiNetwork> {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;

            // Force a fresh scan
            let _ = Command::new("nmcli").args(["device", "wifi", "rescan"]).output();
            std::thread::sleep(std::time::Duration::from_millis(500));

            let output = Command::new("nmcli")
                .args(["-t", "-f", "SSID,SIGNAL,SECURITY", "device", "wifi", "list"])
                .output()
                .ok();

            let saved = self.saved_connections();
            let mut networks = Vec::new();

            if let Some(output) = output {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    // nmcli -t uses : as separator. Parse from the right since
                    // SSID might contain colons. SECURITY is last, SIGNAL is second-to-last.
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() >= 3 {
                        // Last part is security, second-to-last is signal, rest is SSID
                        let security = parts[parts.len() - 1].to_string();
                        let signal: u8 = parts[parts.len() - 2].parse().unwrap_or(0);
                        let ssid = parts[..parts.len() - 2].join(":");
                        if ssid.is_empty() {
                            continue;
                        }
                        let is_saved = saved.iter().any(|s| s == &ssid);
                        networks.push(WifiNetwork {
                            ssid,
                            signal,
                            security,
                            is_saved,
                        });
                    }
                }
            }

            // Deduplicate by SSID, keeping highest signal
            networks.sort_by(|a, b| b.signal.cmp(&a.signal));
            networks.dedup_by(|a, b| a.ssid == b.ssid);
            networks
        }
        #[cfg(not(target_os = "linux"))]
        {
            vec![
                WifiNetwork {
                    ssid: "HomeNetwork".into(),
                    signal: 85,
                    security: "WPA2".into(),
                    is_saved: true,
                },
                WifiNetwork {
                    ssid: "Neighbor5G".into(),
                    signal: 45,
                    security: "WPA3".into(),
                    is_saved: false,
                },
                WifiNetwork {
                    ssid: "CoffeeShop".into(),
                    signal: 60,
                    security: "WPA2".into(),
                    is_saved: true,
                },
                WifiNetwork {
                    ssid: "OpenWifi".into(),
                    signal: 30,
                    security: "--".into(),
                    is_saved: false,
                },
            ]
        }
    }

    pub fn saved_connections(&self) -> Vec<String> {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let output = Command::new("nmcli")
                .args(["-t", "-f", "NAME,TYPE", "con", "show"])
                .output()
                .ok();
            let mut names = Vec::new();
            if let Some(output) = output {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    let parts: Vec<&str> = line.splitn(2, ':').collect();
                    if parts.len() == 2 && parts[1].contains("wireless") {
                        names.push(parts[0].to_string());
                    }
                }
            }
            names
        }
        #[cfg(not(target_os = "linux"))]
        {
            vec!["HomeNetwork".to_string(), "CoffeeShop".to_string()]
        }
    }

    /// Connect to a saved WiFi network.
    /// Reads saved PSK, writes NM connection file, restarts NM to apply.
    pub fn connect(&self, ssid: &str) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            let psk = Self::read_saved_psk(ssid);
            if let Some(password) = psk {
                self.connect_with_password(ssid, &password)
            } else {
                // Open network — write profile without PSK
                Self::write_nm_file(ssid, None)?;
                Self::restart_nm_and_wait(ssid)
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = ssid;
            Ok(())
        }
    }

    /// Connect to a WiFi network with a password.
    /// Writes the NM connection file with PSK, restarts NM to load it fresh.
    pub fn connect_with_password(&self, ssid: &str, password: &str) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            Self::save_psk(ssid, password);
            Self::write_nm_file(ssid, Some(password))?;
            Self::restart_nm_and_wait(ssid)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (ssid, password);
            Ok(())
        }
    }

    /// Write a NM connection file. Removes all stale profiles first.
    #[cfg(target_os = "linux")]
    fn write_nm_file(ssid: &str, password: Option<&str>) -> Result<(), String> {
        use std::process::Command;

        // Delete ALL existing profiles for this SSID
        for suffix in ["", " 1", " 2", " 3", " 4", " 5"] {
            let name = format!("{ssid}{suffix}");
            let _ = Command::new("nmcli").args(["connection", "delete", &name]).output();
        }
        // Remove any leftover .nmconnection files
        for ext in [".nmconnection", ""] {
            let _ = std::fs::remove_file(
                format!("/etc/NetworkManager/system-connections/{ssid}{ext}")
            );
        }

        // Write the connection file
        let conn_file = format!("/etc/NetworkManager/system-connections/{ssid}.nmconnection");
        let security_section = if let Some(psk) = password {
            format!("\n[wifi-security]\nkey-mgmt=wpa-psk\npsk={psk}\npsk-flags=0\n")
        } else {
            String::new()
        };

        let content = format!("\
[connection]
id={ssid}
type=wifi
autoconnect=true

[wifi]
ssid={ssid}
mode=infrastructure
{security_section}
[ipv4]
method=auto

[ipv6]
method=auto
");

        std::fs::write(&conn_file, &content)
            .map_err(|e| format!("Failed to write connection file: {e}"))?;
        let _ = Command::new("chmod").args(["600", &conn_file]).output();
        Ok(())
    }

    /// Restart NetworkManager so it reads connection files fresh from disk,
    /// then wait for the connection to come up.
    #[cfg(target_os = "linux")]
    fn restart_nm_and_wait(ssid: &str) -> Result<(), String> {
        use std::process::Command;

        // Full restart — NM re-reads all connection files from disk
        let _ = Command::new("systemctl").args(["restart", "NetworkManager"]).output();

        // Wait for NM to come back and auto-connect (up to 20 seconds)
        for i in 0..20 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            // NM might not be ready yet for the first few seconds
            if i < 3 { continue; }

            let output = Command::new("nmcli")
                .args(["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "dev", "status"])
                .output();
            if let Ok(output) = output {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.splitn(4, ':').collect();
                    if parts.len() >= 4
                        && parts[1] == "wifi"
                        && parts[2] == "connected"
                    {
                        return Ok(());
                    }
                }
            }
        }

        Err(format!("WiFi did not connect to {ssid} within 20 seconds"))
    }

    /// Save PSK to a simple file so we can retrieve it for reconnection.
    #[cfg(target_os = "linux")]
    fn save_psk(ssid: &str, password: &str) {
        let dir = "/var/lib/cartridge/wifi";
        let _ = std::fs::create_dir_all(dir);
        // Use a safe filename (replace non-alphanumeric chars)
        let safe_name: String = ssid.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        let path = format!("{dir}/{safe_name}.psk");
        let _ = std::fs::write(&path, password);
        let _ = std::process::Command::new("chmod").args(["600", &path]).output();
    }

    /// Read a previously saved PSK.
    #[cfg(target_os = "linux")]
    fn read_saved_psk(ssid: &str) -> Option<String> {
        let safe_name: String = ssid.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        let path = format!("/var/lib/cartridge/wifi/{safe_name}.psk");
        std::fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
    }

    /// Verify that we're actually connected after nmcli reports success.
    #[cfg(target_os = "linux")]
    fn verify_connection(&self, expected_ssid: &str) -> Result<(), String> {
        use std::process::Command;
        // Brief pause to let the connection fully establish
        std::thread::sleep(std::time::Duration::from_secs(1));

        // Check with nmcli (more reliable than iwgetid)
        let output = Command::new("nmcli")
            .args(["-t", "-f", "DEVICE,STATE,CONNECTION", "dev", "status"])
            .output()
            .ok();

        if let Some(output) = output {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.splitn(3, ':').collect();
                // Look for a wireless device in connected state
                if parts.len() >= 3 && parts[1] == "connected" && !parts[2].is_empty() {
                    return Ok(());
                }
            }
        }

        // Fallback: try iwgetid
        let output = Command::new("iwgetid").arg("-r").output();
        if let Ok(output) = output {
            let ssid = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !ssid.is_empty() {
                return Ok(());
            }
        }

        Err(format!("Connection to {expected_ssid} did not establish"))
    }

    pub fn disconnect(&self) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let output = Command::new("nmcli")
                .args(["dev", "disconnect", "wlan0"])
                .output()
                .map_err(|e| format!("nmcli disconnect failed: {e}"))?;
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("Failed to disconnect: {stderr}"))
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(())
        }
    }
}

impl Default for WifiManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "linux")]
fn read_signal_strength() -> u8 {
    if let Ok(content) = std::fs::read_to_string("/proc/net/wireless") {
        for line in content.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let sig_str = parts[3].trim_end_matches('.');
                let dbm: i32 = sig_str.parse().unwrap_or(-100);
                // Convert dBm to percentage: -30 = 100%, -90 = 0%
                return ((dbm + 90).clamp(0, 60) as f32 / 60.0 * 100.0) as u8;
            }
        }
    }
    0
}
