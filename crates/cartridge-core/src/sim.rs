//! Simulated device profile for desktop runs.
//!
//! On non-Linux hosts there is no battery, backlight, amixer or nmcli, so
//! `sysinfo`, `device` and `cartridge_net::wifi` fall back to this single
//! source of truth instead of scattered hardcoded mocks.
//!
//! The profile is loaded lazily, once per process, from
//! `CARTRIDGE_SIM_PROFILE=<path.json>` (see `sim/profiles/r36s-plus.json`),
//! with quick overrides:
//!
//! - `CARTRIDGE_SIM_BATTERY=<0-100>`
//! - `CARTRIDGE_SIM_WIFI=off|<ssid>`
//! - `CARTRIDGE_SIM_HOSTNAME=<name>`
//!
//! Mutable state (brightness, volume, WiFi connection) lives in a process
//! wide `Mutex` so settings changes persist for the life of the process,
//! exactly like they would on the device.
//!
//! The Linux code paths never touch this module.

use serde::Deserialize;
use std::sync::{Mutex, OnceLock};

/// `CARTRIDGE_SIM=1` -- set by `sim.sh`; enables the keyboard cheat sheet
/// and other desktop-only niceties.
pub fn is_sim() -> bool {
    std::env::var("CARTRIDGE_SIM").as_deref() == Ok("1")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SimBattery {
    pub percent: i32,
    pub charging: bool,
}

impl Default for SimBattery {
    fn default() -> Self {
        Self { percent: 72, charging: false }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SimNetwork {
    pub ssid: String,
    /// 0..100
    pub signal: u8,
    pub security: String,
    pub saved: bool,
}

impl Default for SimNetwork {
    fn default() -> Self {
        Self {
            ssid: String::new(),
            signal: 50,
            security: "WPA2".to_string(),
            saved: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SimWifi {
    pub connected: bool,
    pub ssid: String,
    /// dBm (e.g. -55)
    pub rssi: i32,
    /// Networks returned by a scan. The connected SSID is added if missing.
    pub networks: Vec<SimNetwork>,
}

impl Default for SimWifi {
    fn default() -> Self {
        Self {
            connected: true,
            ssid: "HomeNet".to_string(),
            rssi: -55,
            networks: vec![
                SimNetwork { ssid: "HomeNet".into(), signal: 85, security: "WPA2".into(), saved: true },
                SimNetwork { ssid: "Neighbor5G".into(), signal: 45, security: "WPA3".into(), saved: false },
                SimNetwork { ssid: "CoffeeShop".into(), signal: 60, security: "WPA2".into(), saved: true },
                SimNetwork { ssid: "OpenWifi".into(), signal: 30, security: "--".into(), saved: false },
            ],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SimDisk {
    pub total_gb: f32,
    pub used_gb: f32,
}

impl Default for SimDisk {
    fn default() -> Self {
        Self { total_gb: 32.0, used_gb: 12.4 }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SimProfile {
    /// `None` = use the host machine's hostname.
    pub hostname: Option<String>,
    pub battery: SimBattery,
    pub wifi: SimWifi,
    pub mem_total_mb: u64,
    pub disk: SimDisk,
    /// Baseline CPU load; the simulator wobbles a few percent around it.
    pub cpu_percent: f32,
    pub brightness: u8,
    pub volume: u8,
}

impl Default for SimProfile {
    fn default() -> Self {
        Self {
            hostname: None,
            battery: SimBattery::default(),
            wifi: SimWifi::default(),
            mem_total_mb: 1024,
            disk: SimDisk::default(),
            cpu_percent: 25.0,
            brightness: 80,
            volume: 60,
        }
    }
}

fn load_profile() -> SimProfile {
    let mut profile = match std::env::var("CARTRIDGE_SIM_PROFILE") {
        Ok(path) if !path.trim().is_empty() => match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<SimProfile>(&text) {
                Ok(p) => {
                    log::info!("sim: loaded profile {path}");
                    p
                }
                Err(e) => {
                    log::warn!("sim: invalid profile {path}: {e}; using defaults");
                    SimProfile::default()
                }
            },
            Err(e) => {
                log::warn!("sim: cannot read profile {path}: {e}; using defaults");
                SimProfile::default()
            }
        },
        _ => SimProfile::default(),
    };

    if let Ok(v) = std::env::var("CARTRIDGE_SIM_BATTERY") {
        match v.trim().parse::<i32>() {
            Ok(pct) => profile.battery.percent = pct.clamp(0, 100),
            Err(_) => log::warn!("sim: ignoring CARTRIDGE_SIM_BATTERY={v}"),
        }
    }
    if let Ok(v) = std::env::var("CARTRIDGE_SIM_WIFI") {
        let v = v.trim();
        if v.eq_ignore_ascii_case("off") || v.is_empty() {
            profile.wifi.connected = false;
        } else {
            profile.wifi.connected = true;
            profile.wifi.ssid = v.to_string();
        }
    }
    if let Ok(v) = std::env::var("CARTRIDGE_SIM_HOSTNAME") {
        if !v.trim().is_empty() {
            profile.hostname = Some(v.trim().to_string());
        }
    }

    // Make sure the connected SSID shows up in scans.
    if profile.wifi.connected
        && !profile.wifi.ssid.is_empty()
        && !profile.wifi.networks.iter().any(|n| n.ssid == profile.wifi.ssid)
    {
        profile.wifi.networks.insert(
            0,
            SimNetwork {
                ssid: profile.wifi.ssid.clone(),
                signal: rssi_to_percent(profile.wifi.rssi),
                security: "WPA2".to_string(),
                saved: true,
            },
        );
    }

    profile
}

/// The immutable profile (loaded once).
pub fn profile() -> &'static SimProfile {
    static PROFILE: OnceLock<SimProfile> = OnceLock::new();
    PROFILE.get_or_init(load_profile)
}

/// Convert dBm to a 0..100 percentage the same way the Linux path does.
pub fn rssi_to_percent(dbm: i32) -> u8 {
    ((dbm + 90).clamp(0, 60) as f32 / 60.0 * 100.0) as u8
}

// ---------------------------------------------------------------------------
// Mutable runtime state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SimState {
    brightness: u8,
    volume: u8,
    /// Connected SSID, if any.
    wifi_ssid: Option<String>,
    wifi_rssi: i32,
}

fn state() -> &'static Mutex<SimState> {
    static STATE: OnceLock<Mutex<SimState>> = OnceLock::new();
    STATE.get_or_init(|| {
        let p = profile();
        Mutex::new(SimState {
            brightness: p.brightness.min(100),
            volume: p.volume.min(100),
            wifi_ssid: if p.wifi.connected && !p.wifi.ssid.is_empty() {
                Some(p.wifi.ssid.clone())
            } else {
                None
            },
            wifi_rssi: p.wifi.rssi,
        })
    })
}

fn lock() -> std::sync::MutexGuard<'static, SimState> {
    state().lock().unwrap_or_else(|e| e.into_inner())
}

pub fn brightness() -> u8 {
    lock().brightness
}

pub fn set_brightness(pct: u8) {
    lock().brightness = pct.min(100);
}

pub fn volume() -> u8 {
    lock().volume
}

pub fn set_volume(pct: u8) {
    lock().volume = pct.min(100);
}

/// Hostname to report: profile/override, else the host machine's.
pub fn hostname() -> String {
    if let Some(h) = &profile().hostname {
        return h.clone();
    }
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "cartridge".to_string())
}

/// `(ssid, rssi_dbm)` when connected.
pub fn wifi_status() -> Option<(String, i32)> {
    let s = lock();
    s.wifi_ssid.clone().map(|ssid| (ssid, s.wifi_rssi))
}

/// Networks visible to a scan (`saved` reflects the profile plus any
/// network connected during this process).
pub fn wifi_networks() -> Vec<SimNetwork> {
    let connected = lock().wifi_ssid.clone();
    profile()
        .wifi
        .networks
        .iter()
        .cloned()
        .map(|mut n| {
            if connected.as_deref() == Some(n.ssid.as_str()) {
                n.saved = true;
            }
            n
        })
        .collect()
}

pub fn wifi_saved() -> Vec<String> {
    wifi_networks().into_iter().filter(|n| n.saved).map(|n| n.ssid).collect()
}

pub fn wifi_connect(ssid: &str) -> Result<(), String> {
    if ssid.trim().is_empty() {
        return Err("empty SSID".to_string());
    }
    let rssi = profile()
        .wifi
        .networks
        .iter()
        .find(|n| n.ssid == ssid)
        .map(|n| -90 + (n.signal as i32 * 60 / 100))
        .unwrap_or(-60);
    let mut s = lock();
    s.wifi_ssid = Some(ssid.to_string());
    s.wifi_rssi = rssi;
    log::info!("sim: wifi connected to {ssid} ({rssi} dBm)");
    Ok(())
}

pub fn wifi_disconnect() {
    lock().wifi_ssid = None;
    log::info!("sim: wifi disconnected");
}
