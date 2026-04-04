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
            let output = Command::new("nmcli")
                .args(["-t", "-f", "SSID,SIGNAL,SECURITY", "dev", "wifi", "list"])
                .output()
                .ok();

            let saved = self.saved_connections();
            let mut networks = Vec::new();

            if let Some(output) = output {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    let parts: Vec<&str> = line.splitn(3, ':').collect();
                    if parts.len() >= 3 && !parts[0].is_empty() {
                        let ssid = parts[0].to_string();
                        let signal: u8 = parts[1].parse().unwrap_or(0);
                        let security = parts[2].to_string();
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

    /// Connect to a saved WiFi network using --passwd-file to supply secrets.
    /// Reads the PSK from the NM connection file and feeds it to nmcli.
    pub fn connect(&self, ssid: &str) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;

            // Try to read PSK from the saved connection file
            let psk = Self::read_saved_psk(ssid);

            let output = if let Some(ref psk) = psk {
                // Feed the password via --passwd-file
                let passwd_file = "/tmp/.cartridge_wifi_psk";
                let content = format!("802-11-wireless-security.psk:{psk}");
                let _ = std::fs::write(passwd_file, &content);
                let _ = Command::new("chmod").args(["600", passwd_file]).output();

                let result = Command::new("nmcli")
                    .args(["--wait", "15", "--passwd-file", passwd_file, "connection", "up", ssid])
                    .output()
                    .map_err(|e| format!("nmcli failed: {e}"))?;

                let _ = std::fs::remove_file(passwd_file);
                result
            } else {
                // No saved PSK found, try without
                Command::new("nmcli")
                    .args(["--wait", "15", "connection", "up", ssid])
                    .output()
                    .map_err(|e| format!("nmcli failed: {e}"))?
            };

            if output.status.success() {
                self.verify_connection(ssid)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(format!("Failed to connect: {stderr}"))
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = ssid;
            Ok(())
        }
    }

    /// Read the PSK from a saved NM connection file.
    #[cfg(target_os = "linux")]
    fn read_saved_psk(ssid: &str) -> Option<String> {
        // Check both common NM connection paths
        let paths = [
            format!("/etc/NetworkManager/system-connections/{ssid}.nmconnection"),
            format!("/etc/NetworkManager/system-connections/{ssid}"),
        ];
        for path in &paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("psk=") && !trimmed.starts_with("psk-flags") {
                        return Some(trimmed.trim_start_matches("psk=").to_string());
                    }
                }
            }
        }
        None
    }

    /// Connect to a WiFi network with a password.
    /// Saves the password to a NM connection file and uses --passwd-file for activation.
    pub fn connect_with_password(&self, ssid: &str, password: &str) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;

            // Delete any existing connection profile for this SSID
            let _ = Command::new("nmcli")
                .args(["connection", "delete", ssid])
                .output();

            // Write connection file directly to NetworkManager
            let conn_file = format!("/etc/NetworkManager/system-connections/{ssid}.nmconnection");
            let content = format!("\
[connection]
id={ssid}
type=wifi
autoconnect=true

[wifi]
ssid={ssid}
mode=infrastructure

[wifi-security]
key-mgmt=wpa-psk
psk={password}
psk-flags=0

[ipv4]
method=auto

[ipv6]
method=auto
");

            std::fs::write(&conn_file, &content)
                .map_err(|e| format!("Failed to write connection file: {e}"))?;

            // Connection files must be 600 root:root
            let _ = Command::new("chmod").args(["600", &conn_file]).output();

            // Reload connections so NetworkManager picks up the new file
            let _ = Command::new("nmcli").args(["connection", "reload"]).output();

            // Brief pause for reload
            std::thread::sleep(std::time::Duration::from_millis(500));

            // Activate using --passwd-file to feed the PSK directly to nmcli
            let passwd_file = "/tmp/.cartridge_wifi_psk";
            let psk_content = format!("802-11-wireless-security.psk:{password}");
            let _ = std::fs::write(passwd_file, &psk_content);
            let _ = Command::new("chmod").args(["600", passwd_file]).output();

            let output = Command::new("nmcli")
                .args(["--wait", "15", "--passwd-file", passwd_file, "connection", "up", ssid])
                .output()
                .map_err(|e| format!("nmcli up failed: {e}"))?;

            let _ = std::fs::remove_file(passwd_file);

            if output.status.success() {
                self.verify_connection(ssid)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Err(format!("Failed to connect: {stderr}"))
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (ssid, password);
            Ok(())
        }
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
