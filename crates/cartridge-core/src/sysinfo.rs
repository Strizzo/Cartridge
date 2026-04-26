use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

const HISTORY_SIZE: usize = 30;
const MAX_PROCESSES: usize = 12;

/// A single process entry for the htop-like panel.
#[derive(Clone, Debug)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub mem_mb: f32,
    pub state: char, // R=running, S=sleeping, etc.
}

/// Live system information collector.
/// Reads from /proc/ on Linux, uses shell commands or simulated data on macOS.
#[derive(Clone)]
pub struct SystemInfo {
    pub cpu_percent: f32,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub mem_percent: f32,
    pub disk_used_gb: f32,
    pub disk_total_gb: f32,
    pub uptime_secs: u64,
    pub hostname: String,
    pub wifi_ssid: Option<String>,
    pub wifi_signal: i32, // dBm, 0 = unknown
    pub battery_percent: i32, // 0-100, -1 = unknown
    pub battery_charging: bool,
    pub net_rx_rate: f32, // KB/s
    pub net_tx_rate: f32,
    pub process_count: u32,
    pub top_processes: Vec<ProcessEntry>,
    pub cpu_history: VecDeque<f32>,
    pub mem_history: VecDeque<f32>,
    pub net_history: VecDeque<f32>,
    // Internal state for CPU calculation
    #[cfg(target_os = "linux")]
    prev_cpu_idle: u64,
    #[cfg(target_os = "linux")]
    prev_cpu_total: u64,
    #[cfg(target_os = "linux")]
    prev_net_rx: u64,
    #[cfg(target_os = "linux")]
    prev_net_tx: u64,
    // macOS simulation state
    #[cfg(not(target_os = "linux"))]
    sim_time: f32,
    last_poll: Option<Instant>,
}

impl Default for SystemInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemInfo {
    pub fn new() -> Self {
        let hostname = Self::read_hostname();
        Self {
            cpu_percent: 0.0,
            mem_used_mb: 0,
            mem_total_mb: 1024,
            mem_percent: 0.0,
            disk_used_gb: 0.0,
            disk_total_gb: 32.0,
            uptime_secs: 0,
            hostname,
            wifi_ssid: None,
            wifi_signal: 0,
            battery_percent: -1,
            battery_charging: false,
            net_rx_rate: 0.0,
            net_tx_rate: 0.0,
            process_count: 0,
            top_processes: Vec::new(),
            cpu_history: VecDeque::with_capacity(HISTORY_SIZE),
            mem_history: VecDeque::with_capacity(HISTORY_SIZE),
            net_history: VecDeque::with_capacity(HISTORY_SIZE),
            #[cfg(target_os = "linux")]
            prev_cpu_idle: 0,
            #[cfg(target_os = "linux")]
            prev_cpu_total: 0,
            #[cfg(target_os = "linux")]
            prev_net_rx: 0,
            #[cfg(target_os = "linux")]
            prev_net_tx: 0,
            #[cfg(not(target_os = "linux"))]
            sim_time: 0.0,
            last_poll: None,
        }
    }

    /// Read hostname once at startup.
    fn read_hostname() -> String {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/etc/hostname")
                .unwrap_or_else(|_| "cartridge".to_string())
                .trim()
                .to_string()
        }
        #[cfg(not(target_os = "linux"))]
        {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "cartridge".to_string())
        }
    }

    fn push_history(buf: &mut VecDeque<f32>, value: f32) {
        if buf.len() >= HISTORY_SIZE {
            buf.pop_front();
        }
        buf.push_back(value);
    }

    /// Poll all system stats. Call roughly once per second.
    pub fn poll(&mut self) {
        let now = Instant::now();
        let dt = self
            .last_poll
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(1.0);
        self.last_poll = Some(now);

        #[cfg(target_os = "linux")]
        self.poll_linux(dt);

        #[cfg(not(target_os = "linux"))]
        self.poll_simulated(dt);

        Self::push_history(&mut self.cpu_history, self.cpu_percent);
        Self::push_history(&mut self.mem_history, self.mem_percent);
        Self::push_history(
            &mut self.net_history,
            self.net_rx_rate + self.net_tx_rate,
        );
    }

    // -----------------------------------------------------------------------
    // Linux implementation: read /proc/ files
    // -----------------------------------------------------------------------
    #[cfg(target_os = "linux")]
    fn poll_linux(&mut self, dt: f32) {
        self.poll_cpu_linux();
        self.poll_mem_linux();
        self.poll_uptime_linux();
        self.poll_net_linux(dt);
        self.poll_processes_linux();
        self.poll_top_processes_linux();
        self.poll_disk_linux();
        self.poll_wifi_linux();
        self.poll_battery_linux();
    }

    #[cfg(target_os = "linux")]
    fn poll_cpu_linux(&mut self) {
        if let Ok(content) = std::fs::read_to_string("/proc/stat") {
            if let Some(line) = content.lines().next() {
                let parts: Vec<u64> = line
                    .split_whitespace()
                    .skip(1)
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if parts.len() >= 4 {
                    let idle = parts[3];
                    let total: u64 = parts.iter().sum();
                    if self.prev_cpu_total > 0 {
                        let d_total = total.saturating_sub(self.prev_cpu_total);
                        let d_idle = idle.saturating_sub(self.prev_cpu_idle);
                        if d_total > 0 {
                            self.cpu_percent =
                                ((d_total - d_idle) as f32 / d_total as f32) * 100.0;
                        }
                    }
                    self.prev_cpu_idle = idle;
                    self.prev_cpu_total = total;
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn poll_mem_linux(&mut self) {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            let mut total = 0u64;
            let mut available = 0u64;
            for line in content.lines() {
                if let Some(val) = line.strip_prefix("MemTotal:") {
                    total = parse_kb(val);
                } else if let Some(val) = line.strip_prefix("MemAvailable:") {
                    available = parse_kb(val);
                }
            }
            self.mem_total_mb = total / 1024;
            let used = total.saturating_sub(available);
            self.mem_used_mb = used / 1024;
            if total > 0 {
                self.mem_percent = (used as f32 / total as f32) * 100.0;
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn poll_uptime_linux(&mut self) {
        if let Ok(content) = std::fs::read_to_string("/proc/uptime") {
            if let Some(val) = content.split_whitespace().next() {
                self.uptime_secs = val.parse::<f64>().unwrap_or(0.0) as u64;
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn poll_net_linux(&mut self, dt: f32) {
        if let Ok(content) = std::fs::read_to_string("/proc/net/dev") {
            let mut rx_total = 0u64;
            let mut tx_total = 0u64;
            for line in content.lines().skip(2) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 10 {
                    let iface = parts[0].trim_end_matches(':');
                    if iface == "lo" {
                        continue;
                    }
                    rx_total += parts[1].parse::<u64>().unwrap_or(0);
                    tx_total += parts[9].parse::<u64>().unwrap_or(0);
                }
            }
            if self.prev_net_rx > 0 && dt > 0.0 {
                let d_rx = rx_total.saturating_sub(self.prev_net_rx);
                let d_tx = tx_total.saturating_sub(self.prev_net_tx);
                self.net_rx_rate = d_rx as f32 / dt / 1024.0;
                self.net_tx_rate = d_tx as f32 / dt / 1024.0;
            }
            self.prev_net_rx = rx_total;
            self.prev_net_tx = tx_total;
        }
    }

    #[cfg(target_os = "linux")]
    fn poll_processes_linux(&mut self) {
        if let Ok(entries) = std::fs::read_dir("/proc") {
            self.process_count = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map(|s| s.chars().all(|c| c.is_ascii_digit()))
                        .unwrap_or(false)
                })
                .count() as u32;
        }
    }

    #[cfg(target_os = "linux")]
    fn poll_top_processes_linux(&mut self) {
        // Read top processes from ps (cheaper than parsing all /proc/*/stat)
        if let Ok(output) = std::process::Command::new("ps")
            .args(["--no-headers", "-eo", "pid,stat,%cpu,rss,comm", "--sort=-%cpu"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            self.top_processes.clear();
            for line in text.lines().take(MAX_PROCESSES) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 5 {
                    let pid = parts[0].parse().unwrap_or(0);
                    let state = parts[1].chars().next().unwrap_or('?');
                    let cpu = parts[2].parse().unwrap_or(0.0);
                    let rss_kb: f32 = parts[3].parse().unwrap_or(0.0);
                    let name = parts[4..].join(" ");
                    self.top_processes.push(ProcessEntry {
                        pid,
                        name,
                        cpu_percent: cpu,
                        mem_mb: rss_kb / 1024.0,
                        state,
                    });
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn poll_disk_linux(&mut self) {
        // Parse df output for root filesystem
        if let Ok(output) = std::process::Command::new("df")
            .args(["--output=size,used", "-B1", "/"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = text.lines().nth(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let (Ok(total), Ok(used)) =
                        (parts[0].parse::<u64>(), parts[1].parse::<u64>())
                    {
                        self.disk_total_gb = total as f32 / (1024.0 * 1024.0 * 1024.0);
                        self.disk_used_gb = used as f32 / (1024.0 * 1024.0 * 1024.0);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn poll_wifi_linux(&mut self) {
        self.wifi_ssid = None;

        // Primary: use nmcli to check active WiFi connection
        if let Ok(output) = std::process::Command::new("nmcli")
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
                    self.wifi_ssid = Some(parts[3].to_string());
                    break;
                }
            }
        }

        // Fallback: try iwgetid if nmcli didn't find anything
        if self.wifi_ssid.is_none() {
            if let Ok(output) = std::process::Command::new("iwgetid").arg("-r").output() {
                let ssid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !ssid.is_empty() {
                    self.wifi_ssid = Some(ssid);
                }
            }
        }

        // Signal strength from /proc/net/wireless
        if let Ok(content) = std::fs::read_to_string("/proc/net/wireless") {
            for line in content.lines().skip(2) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let sig = parts[3].trim_end_matches('.');
                    self.wifi_signal = sig.parse().unwrap_or(0);
                    break;
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn poll_battery_linux(&mut self) {
        // Try common power supply paths first (fast path)
        for name in &[
            "battery", "BAT0", "BAT1", "bat",
            "axp20x-battery", "axp_battery",
            "rk818-battery", "rk817-battery",
        ] {
            let base = format!("/sys/class/power_supply/{}", name);
            if let Ok(cap) = std::fs::read_to_string(format!("{}/capacity", base)) {
                if let Ok(pct) = cap.trim().parse::<i32>() {
                    if (0..=100).contains(&pct) {
                        self.battery_percent = pct;
                        if let Ok(status) = std::fs::read_to_string(format!("{}/status", base)) {
                            let s = status.trim();
                            self.battery_charging = s == "Charging" || s == "Full";
                        }
                        return;
                    }
                }
            }
        }
        // Scan all power_supply entries — first look for type=Battery, then any with capacity
        if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
            let mut fallback_path: Option<std::path::PathBuf> = None;
            for entry in entries.flatten() {
                let path = entry.path();
                // Check if this entry has a capacity file at all
                let cap_path = path.join("capacity");
                if !cap_path.exists() {
                    continue;
                }
                // Prefer entries with type=Battery
                if let Ok(ptype) = std::fs::read_to_string(path.join("type")) {
                    if ptype.trim() == "Battery" {
                        if let Ok(cap) = std::fs::read_to_string(&cap_path) {
                            if let Ok(pct) = cap.trim().parse::<i32>() {
                                if (0..=100).contains(&pct) {
                                    self.battery_percent = pct;
                                    if let Ok(status) = std::fs::read_to_string(path.join("status")) {
                                        let s = status.trim();
                                        self.battery_charging = s == "Charging" || s == "Full";
                                    }
                                    return;
                                }
                            }
                        }
                    }
                }
                // Remember any entry with a capacity file as fallback
                if fallback_path.is_none() {
                    fallback_path = Some(path);
                }
            }
            // If no type=Battery found, try the fallback (any entry with capacity)
            if let Some(path) = fallback_path {
                if let Ok(cap) = std::fs::read_to_string(path.join("capacity")) {
                    if let Ok(pct) = cap.trim().parse::<i32>() {
                        if (0..=100).contains(&pct) {
                            self.battery_percent = pct;
                            if let Ok(status) = std::fs::read_to_string(path.join("status")) {
                                let s = status.trim();
                                self.battery_charging = s == "Charging" || s == "Full";
                            }
                            return;
                        }
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // macOS / fallback: simulated data with gentle sine-wave variation
    // -----------------------------------------------------------------------
    #[cfg(not(target_os = "linux"))]
    fn poll_simulated(&mut self, dt: f32) {
        self.sim_time += dt;
        let t = self.sim_time;

        // CPU: oscillate between 10-60%
        self.cpu_percent = 25.0 + 20.0 * (t * 0.3).sin() + 10.0 * (t * 1.1).cos();
        self.cpu_percent = self.cpu_percent.clamp(5.0, 95.0);

        // Memory: slowly vary around 40%
        self.mem_total_mb = 1024;
        let mem_pct = 0.38 + 0.08 * (t * 0.15).sin();
        self.mem_used_mb = (self.mem_total_mb as f32 * mem_pct) as u64;
        self.mem_percent = mem_pct * 100.0;

        // Disk: static-ish
        self.disk_total_gb = 32.0;
        self.disk_used_gb = 12.4 + 0.3 * (t * 0.05).sin();

        // Uptime: real uptime via sysctl or just count up
        self.uptime_secs += dt as u64;
        if self.uptime_secs == 0 {
            // Try to get real uptime on macOS
            if let Ok(output) = std::process::Command::new("sysctl")
                .args(["-n", "kern.boottime"])
                .output()
            {
                let s = String::from_utf8_lossy(&output.stdout);
                // Format: { sec = 1234567890, usec = 0 } ...
                if let Some(sec_str) = s.split("sec = ").nth(1) {
                    if let Some(sec_end) = sec_str.find(',') {
                        if let Ok(boot_sec) = sec_str[..sec_end].trim().parse::<u64>() {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            self.uptime_secs = now.saturating_sub(boot_sec);
                        }
                    }
                }
            }
        }

        // Network: variable
        self.net_rx_rate = 1.2 + 2.0 * (t * 0.5).sin().abs();
        self.net_tx_rate = 0.3 + 0.5 * (t * 0.7).cos().abs();

        // Processes: use real ps on macOS
        self.poll_top_processes_macos();

        // WiFi: try real macOS airport command
        self.poll_wifi_macos();

        // Battery: simulated
        self.battery_percent = 72;
        self.battery_charging = false;
    }

    #[cfg(not(target_os = "linux"))]
    fn poll_top_processes_macos(&mut self) {
        if let Ok(output) = std::process::Command::new("ps")
            .args(["-eo", "pid,stat,%cpu,rss,comm"])
            .arg("-r") // sort by CPU
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            self.top_processes.clear();
            let mut count = 0u32;
            for line in text.lines().skip(1) {
                count += 1;
                if self.top_processes.len() >= MAX_PROCESSES {
                    continue; // still counting
                }
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 5 {
                    let pid = parts[0].parse().unwrap_or(0);
                    let state = parts[1].chars().next().unwrap_or('?');
                    let cpu = parts[2].parse().unwrap_or(0.0);
                    let rss_kb: f32 = parts[3].parse().unwrap_or(0.0);
                    // On macOS, comm can be a full path — take just the filename
                    let name_full = parts[4..].join(" ");
                    let name = name_full
                        .rsplit('/')
                        .next()
                        .unwrap_or(&name_full)
                        .to_string();
                    self.top_processes.push(ProcessEntry {
                        pid,
                        name,
                        cpu_percent: cpu,
                        mem_mb: rss_kb / 1024.0,
                        state,
                    });
                }
            }
            self.process_count = count;
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn poll_wifi_macos(&mut self) {
        // Try the macOS airport command for real WiFi info
        let airport = "/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport";
        if let Ok(output) = std::process::Command::new(airport).arg("-I").output() {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut ssid = None;
            let mut rssi = 0i32;
            for line in text.lines() {
                let line = line.trim();
                if let Some(val) = line.strip_prefix("SSID:") {
                    ssid = Some(val.trim().to_string());
                } else if let Some(val) = line.strip_prefix("agrCtlRSSI:") {
                    rssi = val.trim().parse().unwrap_or(0);
                }
            }
            self.wifi_ssid = ssid;
            self.wifi_signal = rssi;
        } else {
            self.wifi_ssid = Some("HomeNet".to_string());
            self.wifi_signal = -55;
        }
    }

    /// Format uptime as "Xd Xh Xm".
    pub fn format_uptime(&self) -> String {
        let secs = self.uptime_secs;
        let days = secs / 86400;
        let hours = (secs % 86400) / 3600;
        let mins = (secs % 3600) / 60;
        if days > 0 {
            format!("{}d {}h {}m", days, hours, mins)
        } else if hours > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}m", mins)
        }
    }

    /// Convert WiFi signal (dBm) to 0-4 bars.
    pub fn wifi_bars(&self) -> u8 {
        if self.wifi_ssid.is_none() {
            return 0;
        }
        let rssi = self.wifi_signal;
        if rssi == 0 { return 0; }
        if rssi >= -50 { return 4; }
        if rssi >= -60 { return 3; }
        if rssi >= -70 { return 2; }
        if rssi >= -80 { return 1; }
        1 // connected but very weak
    }

    /// Format network rate for display.
    pub fn format_rate(rate: f32) -> String {
        if rate > 1024.0 {
            format!("{:.1}M", rate / 1024.0)
        } else if rate > 1.0 {
            format!("{:.1}K", rate)
        } else {
            format!("{:.0}B", rate * 1024.0)
        }
    }
}

#[cfg(target_os = "linux")]
fn parse_kb(val: &str) -> u64 {
    val.split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// AsyncSystemInfo: polls SystemInfo on a background thread to keep the
// render thread responsive. nmcli/ps/df forks alone can take 100-400ms,
// which would otherwise stall the UI for 2-6 frames every poll.
// ---------------------------------------------------------------------------

/// A SystemInfo wrapper that polls in a background thread and exposes
/// the latest snapshot. Implements `Deref<Target = SystemInfo>` so
/// existing code using `info.cpu_percent` etc. works unchanged.
pub struct AsyncSystemInfo {
    current: SystemInfo,
    receiver: Receiver<SystemInfo>,
}

impl AsyncSystemInfo {
    /// Create a new AsyncSystemInfo. The background thread polls every
    /// `interval`. The first snapshot is computed synchronously on the
    /// caller's thread so render code has data on the first frame.
    pub fn new(interval: Duration) -> Self {
        let (tx, rx) = channel();

        // Synchronous initial poll so the first frame has data.
        let mut initial = SystemInfo::new();
        initial.poll();

        // Background thread polls and sends snapshots.
        let mut bg = initial.clone();
        thread::Builder::new()
            .name("sysinfo-poller".to_string())
            .spawn(move || {
                loop {
                    thread::sleep(interval);
                    bg.poll();
                    if tx.send(bg.clone()).is_err() {
                        break; // receiver dropped, exit
                    }
                }
            })
            .ok();

        Self {
            current: initial,
            receiver: rx,
        }
    }

    /// Drain pending snapshots and update `current` with the latest.
    /// Cheap: just a non-blocking try_recv loop. Call once per frame.
    pub fn refresh(&mut self) {
        loop {
            match self.receiver.try_recv() {
                Ok(snap) => self.current = snap,
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    pub fn current(&self) -> &SystemInfo {
        &self.current
    }
}

impl std::ops::Deref for AsyncSystemInfo {
    type Target = SystemInfo;
    fn deref(&self) -> &SystemInfo {
        &self.current
    }
}

impl Default for AsyncSystemInfo {
    fn default() -> Self {
        Self::new(Duration::from_secs(2))
    }
}
