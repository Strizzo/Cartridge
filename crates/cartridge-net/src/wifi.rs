/// A visible WiFi network from scanning.
#[derive(Debug, Clone)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: u8,
    pub security: String,
    pub is_saved: bool,
    /// A saved NetworkManager profile that did not appear in the latest scan.
    pub is_saved_only: bool,
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

fn simulated_status() -> WifiStatus {
    match cartridge_core::sim::wifi_status() {
        Some((ssid, rssi)) => WifiStatus::Connected {
            ssid,
            signal: cartridge_core::sim::rssi_to_percent(rssi),
        },
        None => WifiStatus::Disconnected,
    }
}

fn simulated_networks() -> Vec<WifiNetwork> {
    let mut networks: Vec<_> = cartridge_core::sim::wifi_networks()
        .into_iter()
        .map(|n| WifiNetwork {
            ssid: n.ssid,
            signal: n.signal,
            security: n.security,
            is_saved: n.saved,
            is_saved_only: false,
        })
        .collect();
    networks.sort_by(|a, b| b.signal.cmp(&a.signal));
    networks
}

impl WifiManager {
    pub fn new() -> Self {
        #[cfg(target_os = "linux")]
        if !cartridge_core::sim::is_sim() {
            Self::ensure_nm_headless_config();
        }
        Self
    }

    /// Write a NetworkManager drop-in config for headless operation.
    /// Disables polkit, defaults psk-flags=0, disables MAC randomization.
    /// Only writes once — skips if the file already exists.
    #[cfg(target_os = "linux")]
    fn ensure_nm_headless_config() {
        use std::path::Path;
        let conf = "/etc/NetworkManager/conf.d/90-cartridge-headless.conf";
        if Path::new(conf).exists() {
            return;
        }
        // Matches ArkOS's own NM configuration
        let content = "[main]\n\
auth-polkit=false\n\
\n\
[device]\n\
wifi.scan-rand-mac-address=no\n\
\n\
[connection]\n\
wifi-sec.psk-flags=0\n";

        let _ = std::fs::create_dir_all("/etc/NetworkManager/conf.d");
        if std::fs::write(conf, content).is_ok() {
            log::info!("Wrote NM headless config to {conf}");
            let _ = std::process::Command::new("nmcli")
                .args(["general", "reload"])
                .output();
        }
    }

    pub fn status(&self) -> WifiStatus {
        if cartridge_core::sim::is_sim() {
            return simulated_status();
        }
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
            simulated_status()
        }
    }

    pub fn scan_networks(&self) -> Result<Vec<WifiNetwork>, String> {
        if cartridge_core::sim::is_sim() {
            return Ok(simulated_networks());
        }
        #[cfg(target_os = "linux")]
        {
            let result = (|| -> Result<Vec<WifiNetwork>, String> {
                use std::process::Command;
                use std::time::Duration;

                let networking = Command::new("nmcli")
                    .arg("networking")
                    .output()
                    .map_err(|e| format!("Cannot check networking state: {e}"))?;
                if !networking.status.success() {
                    return Err(nmcli_error("Cannot check networking state", &networking));
                }
                if String::from_utf8_lossy(&networking.stdout).trim() == "disabled" {
                    let enabled = Command::new("nmcli")
                        .args(["networking", "on"])
                        .output()
                        .map_err(|e| format!("Cannot enable networking: {e}"))?;
                    if !enabled.status.success() {
                        return Err(nmcli_error("Cannot enable networking", &enabled));
                    }
                }
                let interface = wifi_interface()?;
                let radio = Command::new("nmcli")
                    .args(["radio", "wifi"])
                    .output()
                    .map_err(|e| format!("Cannot check Wi-Fi radio: {e}"))?;
                if !radio.status.success() {
                    return Err(nmcli_error("Cannot check Wi-Fi radio", &radio));
                }
                if String::from_utf8_lossy(&radio.stdout).trim() == "disabled" {
                    let enabled = Command::new("nmcli")
                        .args(["radio", "wifi", "on"])
                        .output()
                        .map_err(|e| format!("Cannot enable Wi-Fi: {e}"))?;
                    if !enabled.status.success() {
                        return Err(nmcli_error("Cannot enable Wi-Fi", &enabled));
                    }
                }
                // NetworkManager acknowledges a scan request before the AP list is
                // updated. Reading it immediately can report an empty network list
                // on the handheld even when nearby networks are present.
                let scan_request = Command::new("nmcli")
                    .args(["device", "wifi", "rescan", "ifname", &interface])
                    .output();
                let request_error = match scan_request {
                    Ok(output) if output.status.success() => None,
                    Ok(output) => Some(nmcli_error("Scan request failed", &output)),
                    Err(error) => Some(format!("Cannot request Wi-Fi scan: {error}")),
                };

                let saved = self.saved_connections();
                for attempt in 0..10 {
                    if attempt > 0 {
                        std::thread::sleep(Duration::from_millis(500));
                    }
                    let output = Command::new("nmcli")
                        .args([
                            "-t",
                            "-f",
                            "SSID,SIGNAL,SECURITY",
                            "device",
                            "wifi",
                            "list",
                            "--rescan",
                            "no",
                            "ifname",
                            &interface,
                        ])
                        .output()
                        .map_err(|e| format!("Cannot read Wi-Fi networks: {e}"))?;
                    if !output.status.success() {
                        return Err(nmcli_error("Cannot read Wi-Fi networks", &output));
                    }
                    let networks =
                        parse_wifi_list(&String::from_utf8_lossy(&output.stdout), &saved);
                    if !networks.is_empty() {
                        return Ok(networks);
                    }
                }

                let diagnostic = wifi_scan_diagnostic(&interface, request_error.as_deref());
                Err(diagnostic)
            })();
            if let Err(error) = &result {
                log::warn!("Wi-Fi scan failed: {error}");
                save_wifi_scan_diagnostic(error);
            }
            result
        }
        #[cfg(not(target_os = "linux"))]
        {
            Ok(simulated_networks())
        }
    }

    pub fn saved_connections(&self) -> Vec<String> {
        if cartridge_core::sim::is_sim() {
            return cartridge_core::sim::wifi_saved();
        }
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let output = Command::new("nmcli")
                .args(["-t", "-f", "NAME,TYPE", "con", "show"])
                .output()
                .ok();
            output
                .filter(|output| output.status.success())
                .map(|output| parse_saved_connections(&String::from_utf8_lossy(&output.stdout)))
                .unwrap_or_default()
        }
        #[cfg(not(target_os = "linux"))]
        {
            cartridge_core::sim::wifi_saved()
        }
    }

    /// Connect to a saved WiFi network.
    pub fn connect(&self, ssid: &str) -> Result<(), String> {
        if cartridge_core::sim::is_sim() {
            return if cartridge_core::sim::wifi_saved().iter().any(|s| s == ssid) {
                cartridge_core::sim::wifi_connect(ssid)
            } else {
                Err("No saved password for this network".into())
            };
        }
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            if self.saved_connections().iter().any(|name| name == ssid) {
                let output = Command::new("nmcli")
                    .args(["connection", "up", "id", ssid])
                    .output()
                    .map_err(|e| format!("Cannot activate saved connection: {e}"))?;
                if output.status.success() {
                    return Ok(());
                }
                return Err(nmcli_error("Saved connection failed", &output));
            }
            let psk = Self::read_saved_psk(ssid);
            if let Some(password) = psk {
                self.connect_with_password(ssid, &password)
            } else {
                Err("No saved password for this network".to_string())
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            if cartridge_core::sim::wifi_saved().iter().any(|s| s == ssid) {
                cartridge_core::sim::wifi_connect(ssid)
            } else {
                Err("No saved password for this network".to_string())
            }
        }
    }

    /// Connect to a WiFi network with a password.
    ///
    /// Uses the exact pattern from ArkOS's importwifi.sh (the fix for issue #580):
    ///   1. nmcli c add (bare wifi, no security)
    ///   2. nmcli c modify (add WPA-PSK credentials)
    ///   3. nmcli con up
    /// Falls back to keyfile approach if that fails.
    /// Logs all steps to /tmp/cartridge_wifi.log for diagnostics.
    pub fn connect_with_password(&self, ssid: &str, password: &str) -> Result<(), String> {
        if cartridge_core::sim::is_sim() {
            return cartridge_core::sim::wifi_connect(ssid);
        }
        #[cfg(target_os = "linux")]
        {
            let interface = wifi_interface()?;
            Self::save_psk(ssid, password);

            let mut log = String::new();
            log.push_str(&format!(
                "=== WiFi connect: '{}' at {} ===\n",
                ssid,
                chrono_now()
            ));

            // Step 0: Ensure NM headless config exists
            if !cartridge_core::sim::is_sim() {
                Self::ensure_nm_headless_config();
            }

            // Step 1: Clean up ALL stale profiles
            let cleanup = Self::cleanup_profiles(ssid);
            log.push_str(&format!("cleanup: {cleanup}\n"));

            // Step 2: Primary — ArkOS importwifi.sh pattern (add → modify → up)
            log.push_str("Trying ArkOS add/modify/up pattern...\n");
            match Self::connect_arkos_pattern(ssid, password, &interface, &mut log) {
                Ok(()) => {
                    log.push_str("SUCCESS via add/modify/up\n");
                    let _ = std::fs::write("/tmp/cartridge_wifi.log", &log);
                    return self.verify_connection(ssid);
                }
                Err(e) => {
                    log.push_str(&format!("add/modify/up failed: {e}\n"));
                }
            }

            // Step 3: Fallback — write keyfile directly + load + up
            log.push_str("Trying keyfile fallback...\n");
            let cleanup2 = Self::cleanup_profiles(ssid);
            log.push_str(&format!("cleanup2: {cleanup2}\n"));

            match Self::connect_via_keyfile(ssid, password, &interface, &mut log) {
                Ok(()) => {
                    log.push_str("SUCCESS via keyfile\n");
                    let _ = std::fs::write("/tmp/cartridge_wifi.log", &log);
                    return self.verify_connection(ssid);
                }
                Err(e) => {
                    log.push_str(&format!("keyfile failed: {e}\n"));
                }
            }

            let _ = std::fs::write("/tmp/cartridge_wifi.log", &log);
            Err("All connection methods failed. See /tmp/cartridge_wifi.log".to_string())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = password;
            cartridge_core::sim::wifi_connect(ssid)
        }
    }

    /// ArkOS importwifi.sh pattern — the proven fix for issue #580.
    /// Exact sequence: add bare connection → modify to add WPA-PSK → activate.
    #[cfg(target_os = "linux")]
    fn connect_arkos_pattern(
        ssid: &str,
        password: &str,
        interface: &str,
        log: &mut String,
    ) -> Result<(), String> {
        use std::process::Command;

        // Step A: Create bare wifi connection (NO security params in add)
        let add_out = Command::new("nmcli")
            .args([
                "c", "add", "con-name", ssid, "type", "wifi", "ssid", ssid, "ifname", interface,
            ])
            .output()
            .map_err(|e| format!("nmcli add: {e}"))?;

        let add_ok = add_out.status.success();
        let add_msg = String::from_utf8_lossy(&add_out.stdout).trim().to_string();
        let add_err = String::from_utf8_lossy(&add_out.stderr).trim().to_string();
        log.push_str(&format!(
            "add ok={add_ok} out='{add_msg}' err='{add_err}'\n"
        ));

        if !add_ok {
            return Err(format!("add failed: {add_err}"));
        }

        // Step B: Modify to add WPA-PSK (using wifi-sec.* shorthand)
        let mod_out = Command::new("nmcli")
            .args([
                "c",
                "modify",
                ssid,
                "wifi-sec.key-mgmt",
                "wpa-psk",
                "wifi-sec.psk",
                password,
            ])
            .output()
            .map_err(|e| format!("nmcli modify: {e}"))?;

        let mod_ok = mod_out.status.success();
        let mod_msg = String::from_utf8_lossy(&mod_out.stdout).trim().to_string();
        let mod_err = String::from_utf8_lossy(&mod_out.stderr).trim().to_string();
        log.push_str(&format!(
            "modify ok={mod_ok} out='{mod_msg}' err='{mod_err}'\n"
        ));

        if !mod_ok {
            // Clean up the profile we just created
            let _ = Command::new("nmcli").args(["c", "delete", ssid]).output();
            return Err(format!("modify failed: {mod_err}"));
        }

        // Step C: Activate
        let up_out = Command::new("nmcli")
            .args(["con", "up", ssid])
            .output()
            .map_err(|e| format!("nmcli up: {e}"))?;

        let up_ok = up_out.status.success();
        let up_msg = String::from_utf8_lossy(&up_out.stdout).trim().to_string();
        let up_err = String::from_utf8_lossy(&up_out.stderr).trim().to_string();
        log.push_str(&format!("up ok={up_ok} out='{up_msg}' err='{up_err}'\n"));

        if up_ok {
            Ok(())
        } else {
            Err(format!("{up_err}"))
        }
    }

    /// Fallback: write a NM keyfile directly and load it.
    #[cfg(target_os = "linux")]
    fn connect_via_keyfile(
        ssid: &str,
        password: &str,
        interface: &str,
        log: &mut String,
    ) -> Result<(), String> {
        use std::process::Command;

        let uuid = std::fs::read_to_string("/proc/sys/kernel/random/uuid")
            .unwrap_or_else(|_| "00000000-0000-4000-8000-000000000000".to_string())
            .trim()
            .to_string();

        let conn_file = format!("/etc/NetworkManager/system-connections/{ssid}.nmconnection");
        let content = format!(
            "[connection]\n\
id={ssid}\n\
uuid={uuid}\n\
type=wifi\n\
interface-name={interface}\n\
autoconnect=true\n\
\n\
[wifi]\n\
mode=infrastructure\n\
ssid={ssid}\n\
\n\
[wifi-security]\n\
key-mgmt=wpa-psk\n\
psk={password}\n\
psk-flags=0\n\
\n\
[ipv4]\n\
method=auto\n\
\n\
[ipv6]\n\
method=auto\n"
        );

        log.push_str(&format!("Writing {conn_file}\n"));
        std::fs::write(&conn_file, &content).map_err(|e| format!("write failed: {e}"))?;
        let _ = Command::new("chmod").args(["600", &conn_file]).output();

        // Load the specific file
        let load_out = Command::new("nmcli")
            .args(["connection", "load", &conn_file])
            .output()
            .map_err(|e| format!("nmcli load: {e}"))?;

        let load_ok = load_out.status.success();
        let load_err = String::from_utf8_lossy(&load_out.stderr).trim().to_string();
        log.push_str(&format!("load ok={load_ok} err='{load_err}'\n"));

        if !load_ok {
            let _ = Command::new("nmcli")
                .args(["connection", "reload"])
                .output();
        }

        std::thread::sleep(std::time::Duration::from_secs(1));

        // Activate
        let up_out = Command::new("nmcli")
            .args(["con", "up", ssid])
            .output()
            .map_err(|e| format!("nmcli up: {e}"))?;

        let up_ok = up_out.status.success();
        let up_msg = String::from_utf8_lossy(&up_out.stdout).trim().to_string();
        let up_err = String::from_utf8_lossy(&up_out.stderr).trim().to_string();
        log.push_str(&format!("up ok={up_ok} out='{up_msg}' err='{up_err}'\n"));

        if up_ok {
            Ok(())
        } else {
            Err(format!("{up_err}"))
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
        let safe_name: String = ssid
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = format!("{dir}/{safe_name}.psk");
        let _ = std::fs::write(&path, password);
        let _ = std::process::Command::new("chmod")
            .args(["600", &path])
            .output();
    }

    /// Read a previously saved PSK.
    #[cfg(target_os = "linux")]
    fn read_saved_psk(ssid: &str) -> Option<String> {
        let safe_name: String = ssid
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = format!("/var/lib/cartridge/wifi/{safe_name}.psk");
        std::fs::read_to_string(&path)
            .ok()
            .map(|s| s.trim().to_string())
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
        if cartridge_core::sim::is_sim() {
            cartridge_core::sim::wifi_disconnect();
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let interface = wifi_interface()?;
            let output = Command::new("nmcli")
                .args(["dev", "disconnect", &interface])
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
            cartridge_core::sim::wifi_disconnect();
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
fn nmcli_error(context: &str, output: &std::process::Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if detail.is_empty() {
        context.to_string()
    } else {
        format!("{context}: {detail}")
    }
}

#[cfg(any(target_os = "linux", test))]
fn parse_saved_connections(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let (name, kind) = line.rsplit_once(':')?;
            (kind == "wifi" || kind.contains("wireless")).then(|| name.replace("\\:", ":"))
        })
        .collect()
}

#[cfg(any(target_os = "linux", test))]
fn parse_wifi_list(text: &str, saved: &[String]) -> Vec<WifiNetwork> {
    let mut networks = Vec::new();
    for line in text.lines() {
        // nmcli escapes colons inside SSIDs; split from the right so an SSID
        // with a colon still leaves SIGNAL and SECURITY in their own fields.
        let parts: Vec<_> = line.rsplitn(3, ':').collect();
        if parts.len() != 3 {
            continue;
        }
        let ssid = parts[2].replace("\\:", ":");
        if ssid.is_empty() {
            continue;
        }
        networks.push(WifiNetwork {
            is_saved: saved.iter().any(|name| name == &ssid),
            is_saved_only: false,
            ssid,
            signal: parts[1].parse().unwrap_or(0),
            security: parts[0].to_string(),
        });
    }
    networks.sort_by(|a, b| b.signal.cmp(&a.signal));
    let mut seen = std::collections::HashSet::new();
    networks.retain(|network| seen.insert(network.ssid.clone()));
    networks
}

/// Keep saved profiles selectable when the radio does not return a scan list.
pub fn merge_saved_connections(
    mut networks: Vec<WifiNetwork>,
    saved: &[String],
) -> Vec<WifiNetwork> {
    for name in saved {
        if name.is_empty() || networks.iter().any(|network| network.ssid == *name) {
            continue;
        }
        networks.push(WifiNetwork {
            ssid: name.clone(),
            signal: 0,
            security: String::new(),
            is_saved: true,
            is_saved_only: true,
        });
    }
    networks
}

#[cfg(target_os = "linux")]
fn wifi_scan_diagnostic(interface: &str, request_error: Option<&str>) -> String {
    let status = std::process::Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE,STATE", "device", "status"])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default();
    let state = status
        .lines()
        .find_map(|line| {
            let mut fields = line.splitn(3, ':');
            match (fields.next(), fields.next(), fields.next()) {
                (Some(device), Some("wifi"), Some(state)) if device == interface => Some(state),
                _ => None,
            }
        })
        .unwrap_or("unknown");
    let radio = std::process::Command::new("nmcli")
        .args(["radio", "wifi"])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let mut message =
        format!("No access points after scan on {interface} (state {state}, radio {radio}).");
    if let Some(error) = request_error {
        message.push(' ');
        message.push_str(error);
    }
    message
}

#[cfg(target_os = "linux")]
fn save_wifi_scan_diagnostic(summary: &str) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let dir = std::path::PathBuf::from(home).join(".cartridges");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let mut report = format!("{summary}\n");
    for (label, command, args) in [
        (
            "devices",
            "nmcli",
            vec!["-t", "-f", "DEVICE,TYPE,STATE", "device", "status"],
        ),
        ("wifi devices", "nmcli", vec!["device", "show"]),
        ("networking", "nmcli", vec!["networking"]),
        ("radio", "nmcli", vec!["radio", "wifi"]),
        (
            "access points",
            "nmcli",
            vec![
                "-t",
                "-f",
                "SSID,SIGNAL,SECURITY",
                "device",
                "wifi",
                "list",
                "--rescan",
                "no",
            ],
        ),
        ("rfkill", "rfkill", vec!["list"]),
        ("interfaces", "iw", vec!["dev"]),
        ("loaded modules", "lsmod", vec![]),
        (
            "network services",
            "systemctl",
            vec!["is-active", "NetworkManager", "wpa_supplicant"],
        ),
        (
            "network service log",
            "journalctl",
            vec![
                "-b",
                "-u",
                "NetworkManager",
                "-u",
                "wpa_supplicant",
                "-n",
                "120",
                "--no-pager",
            ],
        ),
    ] {
        report.push_str(&format!("\n[{label}]\n"));
        match std::process::Command::new(command).args(args).output() {
            Ok(output) => {
                report.push_str(&String::from_utf8_lossy(&output.stdout));
                report.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            Err(error) => report.push_str(&format!("{error}\n")),
        }
    }
    if let Ok(interface) = wifi_interface() {
        report.push_str(&format!("\n[interface {interface}]\n"));
        for (label, command, args) in [
            ("link", "ip", vec!["-d", "link", "show", &interface]),
            ("wireless extensions", "iwconfig", vec![&interface]),
            (
                "NetworkManager reason",
                "nmcli",
                vec![
                    "-t",
                    "-f",
                    "GENERAL.STATE,GENERAL.REASON",
                    "device",
                    "show",
                    &interface,
                ],
            ),
        ] {
            report.push_str(&format!("\n[{label}]\n"));
            match std::process::Command::new(command).args(args).output() {
                Ok(output) => {
                    report.push_str(&String::from_utf8_lossy(&output.stdout));
                    report.push_str(&String::from_utf8_lossy(&output.stderr));
                }
                Err(error) => report.push_str(&format!("{error}\n")),
            }
        }
        let driver = std::path::Path::new("/sys/class/net")
            .join(&interface)
            .join("device/driver");
        report.push_str(&format!("\n[driver]\n{:?}\n", std::fs::read_link(driver)));
    }
    report.push_str("\n[kernel wireless messages]\n");
    match std::process::Command::new("journalctl")
        .args(["-b", "-k", "-n", "2000", "--no-pager"])
        .output()
    {
        Ok(output) => {
            let kernel_log = String::from_utf8_lossy(&output.stdout);
            let matches: Vec<_> = kernel_log
                .lines()
                .filter(|line| {
                    let line = line.to_ascii_lowercase();
                    [
                        "wlan", "wifi", "wireless", "firmware", "rtl", "brcm", "rfkill", "sdio",
                        "cfg80211", "80211", "mmc", "usb",
                    ]
                    .iter()
                    .any(|term| line.contains(term))
                })
                .collect();
            for line in &matches[matches.len().saturating_sub(100)..] {
                report.push_str(line);
                report.push('\n');
            }
            report.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        Err(error) => report.push_str(&format!("{error}\n")),
    }
    let _ = std::fs::write(dir.join("wifi-scan.log"), report);
}

#[cfg(any(target_os = "linux", test))]
fn wifi_device_from_status(text: &str) -> Option<&str> {
    text.lines().find_map(|line| {
        let mut parts = line.splitn(3, ':');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(device), Some("wifi"), Some(_)) if !device.is_empty() => Some(device),
            _ => None,
        }
    })
}

#[cfg(target_os = "linux")]
fn wifi_interface() -> Result<String, String> {
    let output = std::process::Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE,STATE", "device", "status"])
        .output()
        .map_err(|e| format!("NetworkManager is unavailable: {e}"))?;
    if !output.status.success() {
        return Err(nmcli_error("NetworkManager is unavailable", &output));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    wifi_device_from_status(&text)
        .map(str::to_string)
        .ok_or_else(|| "No Wi-Fi interface detected. Check the adapter or driver.".to_string())
}

#[cfg(test)]
mod wifi_tests {
    use super::{
        merge_saved_connections, parse_saved_connections, parse_wifi_list, wifi_device_from_status,
    };

    #[test]
    fn finds_wifi_profiles_and_unescapes_profile_names() {
        let profiles = parse_saved_connections(
            "Home:802-11-wireless\nCafe\\:WiFi:wifi\nWired:802-3-ethernet\n",
        );
        assert_eq!(profiles, ["Home", "Cafe:WiFi"]);
    }

    #[test]
    fn finds_wifi_without_assuming_wlan0() {
        let status =
            "lo:loopback:connected\neth0:ethernet:connected\nwlx001122:wifi:disconnected\n";
        assert_eq!(wifi_device_from_status(status), Some("wlx001122"));
        assert_eq!(wifi_device_from_status("eth0:ethernet:connected\n"), None);
    }

    #[test]
    fn parses_saved_ssids_and_escaped_colons() {
        let networks = parse_wifi_list(
            "Cafe\\:WiFi:78:WPA2\nOther:43:--\n:29:WPA2\nCafe\\:WiFi:61:WPA2\n",
            &["Cafe:WiFi".to_string()],
        );
        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].ssid, "Cafe:WiFi");
        assert!(networks[0].is_saved);
        assert_eq!(networks[0].signal, 78);
    }

    #[test]
    fn keeps_saved_profiles_when_scan_is_empty_without_duplicating_visible_ones() {
        let saved = vec!["Home".to_string(), "Cafe".to_string()];
        let scanned = parse_wifi_list("Home:75:WPA2\n", &saved);
        let networks = merge_saved_connections(scanned, &saved);
        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].ssid, "Home");
        assert!(!networks[0].is_saved_only);
        assert_eq!(networks[1].ssid, "Cafe");
        assert!(networks[1].is_saved_only);
        assert!(networks[1].is_saved);
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
