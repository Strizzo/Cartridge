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
        #[cfg(target_os = "linux")]
        Self::ensure_nm_headless_config();
        Self
    }

    /// Write a NetworkManager drop-in config that disables polkit and
    /// defaults psk-flags=0 (store secrets in file, no agent needed).
    /// Only writes once — skips if the file already exists.
    #[cfg(target_os = "linux")]
    fn ensure_nm_headless_config() {
        use std::path::Path;
        let conf = "/etc/NetworkManager/conf.d/90-cartridge-headless.conf";
        if Path::new(conf).exists() {
            return;
        }
        let content = "\
[main]\n\
auth-polkit=false\n\
\n\
[connection]\n\
wifi-sec.psk-flags=0\n";

        let _ = std::fs::create_dir_all("/etc/NetworkManager/conf.d");
        if std::fs::write(conf, content).is_ok() {
            log::info!("Wrote NM headless config to {conf}");
            // Reload NM config
            let _ = std::process::Command::new("nmcli")
                .args(["general", "reload"])
                .output();
        }
    }

    pub fn status(&self) -> WifiStatus {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;

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
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() >= 3 {
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

            networks.sort_by(|a, b| b.signal.cmp(&a.signal));
            networks.dedup_by(|a, b| a.ssid == b.ssid);
            networks
        }
        #[cfg(not(target_os = "linux"))]
        {
            vec![
                WifiNetwork { ssid: "HomeNetwork".into(), signal: 85, security: "WPA2".into(), is_saved: true },
                WifiNetwork { ssid: "Neighbor5G".into(), signal: 45, security: "WPA3".into(), is_saved: false },
                WifiNetwork { ssid: "CoffeeShop".into(), signal: 60, security: "WPA2".into(), is_saved: true },
                WifiNetwork { ssid: "OpenWifi".into(), signal: 30, security: "--".into(), is_saved: false },
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
    pub fn connect(&self, ssid: &str) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            let psk = Self::read_saved_psk(ssid);
            if let Some(password) = psk {
                self.connect_with_password(ssid, &password)
            } else {
                Err("No saved password for this network".to_string())
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = ssid;
            Ok(())
        }
    }

    /// Connect to a WiFi network with a password.
    ///
    /// Strategy: write a correct NM keyfile with psk-flags=0, load it,
    /// then activate. Falls back to `nmcli connection add` if file approach
    /// fails. Logs all steps to /tmp/cartridge_wifi.log for diagnostics.
    pub fn connect_with_password(&self, ssid: &str, password: &str) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            Self::save_psk(ssid, password);

            // Log everything for diagnostics
            let mut log = String::new();
            log.push_str(&format!("=== WiFi connect: '{}' at {} ===\n",
                ssid, chrono_now()));

            // Step 1: Clean up ALL stale profiles
            let cleanup = Self::cleanup_profiles(ssid);
            log.push_str(&format!("cleanup: {cleanup}\n"));

            // Step 2: Try primary approach — write keyfile + load + activate
            log.push_str("Trying keyfile approach...\n");
            match Self::connect_via_keyfile(ssid, password, &mut log) {
                Ok(()) => {
                    log.push_str("SUCCESS via keyfile\n");
                    let _ = std::fs::write("/tmp/cartridge_wifi.log", &log);
                    return self.verify_connection(ssid);
                }
                Err(e) => {
                    log.push_str(&format!("keyfile failed: {e}\n"));
                }
            }

            // Step 3: Fallback — use nmcli connection add with explicit psk-flags 0
            log.push_str("Trying nmcli connection add fallback...\n");
            let cleanup2 = Self::cleanup_profiles(ssid);
            log.push_str(&format!("cleanup2: {cleanup2}\n"));

            match Self::connect_via_nmcli_add(ssid, password, &mut log) {
                Ok(()) => {
                    log.push_str("SUCCESS via nmcli add\n");
                    let _ = std::fs::write("/tmp/cartridge_wifi.log", &log);
                    return self.verify_connection(ssid);
                }
                Err(e) => {
                    log.push_str(&format!("nmcli add failed: {e}\n"));
                }
            }

            let _ = std::fs::write("/tmp/cartridge_wifi.log", &log);
            Err("All connection methods failed. See /tmp/cartridge_wifi.log".to_string())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (ssid, password);
            Ok(())
        }
    }

    /// Primary approach: write a NM keyfile and load it.
    #[cfg(target_os = "linux")]
    fn connect_via_keyfile(ssid: &str, password: &str, log: &mut String) -> Result<(), String> {
        use std::process::Command;

        // Read UUID from /proc if available, otherwise generate one
        let uuid = std::fs::read_to_string("/proc/sys/kernel/random/uuid")
            .unwrap_or_else(|_| {
                format!("{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
                    std::process::id(),
                    std::process::id() as u16,
                    std::process::id() & 0xfff,
                    std::process::id() & 0xfff,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as u64 & 0xffffffffffff)
            })
            .trim()
            .to_string();

        // NM keyfile format — section names are [wifi] and [wifi-security],
        // NOT [802-11-wireless]. type is "wifi", NOT "802-11-wireless".
        let conn_file = format!("/etc/NetworkManager/system-connections/{ssid}.nmconnection");
        let content = format!("[connection]\n\
id={ssid}\n\
uuid={uuid}\n\
type=wifi\n\
autoconnect=true\n\
\n\
[wifi]\n\
mode=infrastructure\n\
ssid={ssid}\n\
\n\
[wifi-security]\n\
auth-alg=open\n\
key-mgmt=wpa-psk\n\
psk={password}\n\
psk-flags=0\n\
\n\
[ipv4]\n\
method=auto\n\
\n\
[ipv6]\n\
method=auto\n");

        log.push_str(&format!("Writing {conn_file}\n"));
        std::fs::write(&conn_file, &content)
            .map_err(|e| format!("write failed: {e}"))?;

        let _ = Command::new("chmod").args(["600", &conn_file]).output();

        // Use `nmcli connection load` for the specific file (NOT reload)
        let load_out = Command::new("nmcli")
            .args(["connection", "load", &conn_file])
            .output()
            .map_err(|e| format!("nmcli load failed: {e}"))?;

        let load_ok = load_out.status.success();
        let load_stdout = String::from_utf8_lossy(&load_out.stdout).trim().to_string();
        let load_stderr = String::from_utf8_lossy(&load_out.stderr).trim().to_string();
        log.push_str(&format!("load ok={load_ok} stdout='{load_stdout}' stderr='{load_stderr}'\n"));

        if !load_ok {
            // If load fails, try reload as fallback
            log.push_str("load failed, trying reload...\n");
            let _ = Command::new("nmcli").args(["connection", "reload"]).output();
        }

        std::thread::sleep(std::time::Duration::from_secs(1));

        // Activate
        let up_out = Command::new("nmcli")
            .args(["--wait", "20", "connection", "up", ssid])
            .output()
            .map_err(|e| format!("nmcli up failed: {e}"))?;

        let up_ok = up_out.status.success();
        let up_stdout = String::from_utf8_lossy(&up_out.stdout).trim().to_string();
        let up_stderr = String::from_utf8_lossy(&up_out.stderr).trim().to_string();
        log.push_str(&format!("up ok={up_ok} stdout='{up_stdout}' stderr='{up_stderr}'\n"));

        if up_ok {
            Ok(())
        } else {
            Err(format!("{up_stderr}"))
        }
    }

    /// Fallback approach: use `nmcli connection add` with explicit psk-flags.
    #[cfg(target_os = "linux")]
    fn connect_via_nmcli_add(ssid: &str, password: &str, log: &mut String) -> Result<(), String> {
        use std::process::Command;

        let add_out = Command::new("nmcli")
            .args([
                "connection", "add",
                "type", "wifi",
                "con-name", ssid,
                "ifname", "wlan0",
                "ssid", ssid,
                "wifi-sec.key-mgmt", "wpa-psk",
                "wifi-sec.psk", password,
                "wifi-sec.psk-flags", "0",
            ])
            .output()
            .map_err(|e| format!("nmcli add failed: {e}"))?;

        let add_ok = add_out.status.success();
        let add_stdout = String::from_utf8_lossy(&add_out.stdout).trim().to_string();
        let add_stderr = String::from_utf8_lossy(&add_out.stderr).trim().to_string();
        log.push_str(&format!("add ok={add_ok} stdout='{add_stdout}' stderr='{add_stderr}'\n"));

        if !add_ok {
            return Err(format!("add failed: {add_stderr}"));
        }

        std::thread::sleep(std::time::Duration::from_millis(500));

        // Activate
        let up_out = Command::new("nmcli")
            .args(["--wait", "20", "connection", "up", ssid])
            .output()
            .map_err(|e| format!("nmcli up failed: {e}"))?;

        let up_ok = up_out.status.success();
        let up_stdout = String::from_utf8_lossy(&up_out.stdout).trim().to_string();
        let up_stderr = String::from_utf8_lossy(&up_out.stderr).trim().to_string();
        log.push_str(&format!("up ok={up_ok} stdout='{up_stdout}' stderr='{up_stderr}'\n"));

        if up_ok {
            Ok(())
        } else {
            Err(format!("{up_stderr}"))
        }
    }

    /// Delete all connection profiles matching an SSID.
    #[cfg(target_os = "linux")]
    fn cleanup_profiles(ssid: &str) -> String {
        use std::process::Command;
        let mut result = String::new();

        for suffix in ["", " 1", " 2", " 3", " 4", " 5"] {
            let name = format!("{ssid}{suffix}");
            let out = Command::new("nmcli")
                .args(["connection", "delete", &name])
                .output();
            if let Ok(out) = out {
                if out.status.success() {
                    result.push_str(&format!("deleted '{name}'; "));
                }
            }
        }
        // Remove any leftover files
        for ext in [".nmconnection", ""] {
            let path = format!("/etc/NetworkManager/system-connections/{ssid}{ext}");
            if std::fs::remove_file(&path).is_ok() {
                result.push_str(&format!("removed {path}; "));
            }
        }
        if result.is_empty() {
            result = "nothing to clean".to_string();
        }
        result
    }

    /// Save PSK to a file so we can retrieve it for reconnection.
    #[cfg(target_os = "linux")]
    fn save_psk(ssid: &str, password: &str) {
        let dir = "/var/lib/cartridge/wifi";
        let _ = std::fs::create_dir_all(dir);
        let safe_name: String = ssid.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let path = format!("{dir}/{safe_name}.psk");
        let _ = std::fs::write(&path, password);
        let _ = std::process::Command::new("chmod").args(["600", &path]).output();
    }

    /// Read a previously saved PSK.
    #[cfg(target_os = "linux")]
    fn read_saved_psk(ssid: &str) -> Option<String> {
        let safe_name: String = ssid.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        let path = format!("/var/lib/cartridge/wifi/{safe_name}.psk");
        std::fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
    }

    /// Verify that we're actually connected.
    #[cfg(target_os = "linux")]
    fn verify_connection(&self, expected_ssid: &str) -> Result<(), String> {
        use std::process::Command;
        std::thread::sleep(std::time::Duration::from_secs(1));

        let output = Command::new("nmcli")
            .args(["-t", "-f", "DEVICE,STATE,CONNECTION", "dev", "status"])
            .output()
            .ok();

        if let Some(output) = output {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.splitn(3, ':').collect();
                if parts.len() >= 3 && parts[1] == "connected" && !parts[2].is_empty() {
                    return Ok(());
                }
            }
        }

        if let Ok(output) = Command::new("iwgetid").arg("-r").output() {
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
                return ((dbm + 90).clamp(0, 60) as f32 / 60.0 * 100.0) as u8;
            }
        }
    }
    0
}

#[cfg(target_os = "linux")]
fn chrono_now() -> String {
    // Simple timestamp without chrono dependency
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}
