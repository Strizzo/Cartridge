//! Device hardware controls: backlight brightness and audio volume.
//!
//! On Linux, reads/writes /sys/class/backlight/ and shells out to amixer
//! for volume. On macOS, no-ops (returns reasonable mock values) so the
//! UI still works during desktop development.

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Brightness
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn backlight_dir() -> Option<PathBuf> {
    let entries = std::fs::read_dir("/sys/class/backlight").ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.join("brightness").exists() && p.join("max_brightness").exists() {
            return Some(p);
        }
    }
    None
}

/// Get current brightness as 0..100. Returns 100 on platforms with no
/// backlight control.
pub fn get_brightness_percent() -> u8 {
    #[cfg(target_os = "linux")]
    {
        if let Some(dir) = backlight_dir() {
            let cur: u32 = std::fs::read_to_string(dir.join("brightness"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
            let max: u32 = std::fs::read_to_string(dir.join("max_brightness"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(1);
            if max > 0 {
                return ((cur * 100) / max).min(100) as u8;
            }
        }
        100
    }
    #[cfg(not(target_os = "linux"))]
    {
        100
    }
}

/// Set brightness as 0..100. Clamped. No-op if no backlight is available.
pub fn set_brightness_percent(pct: u8) {
    let pct = pct.min(100);
    #[cfg(target_os = "linux")]
    {
        if let Some(dir) = backlight_dir() {
            let max: u32 = std::fs::read_to_string(dir.join("max_brightness"))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
            if max > 0 {
                let target = (pct as u32 * max / 100).max(1);
                let _ = std::fs::write(dir.join("brightness"), target.to_string());
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pct;
    }
}

// ---------------------------------------------------------------------------
// Volume (via amixer)
// ---------------------------------------------------------------------------

/// Get current master volume as 0..100.
pub fn get_volume_percent() -> u8 {
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("amixer")
            .args(["-M", "sget", "Master"])
            .output()
            .ok();
        if let Some(output) = output {
            let text = String::from_utf8_lossy(&output.stdout);
            // Match the first occurrence of [N%]
            for cap in text.split('[').skip(1) {
                if let Some(end) = cap.find('%') {
                    if let Ok(v) = cap[..end].parse::<u8>() {
                        return v.min(100);
                    }
                }
            }
        }
        50
    }
    #[cfg(not(target_os = "linux"))]
    {
        50
    }
}

/// Set master volume to 0..100. No-op if amixer is unavailable.
pub fn set_volume_percent(pct: u8) {
    let pct = pct.min(100);
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("amixer")
            .args(["-q", "-M", "sset", "Master", &format!("{pct}%")])
            .output();
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pct;
    }
}
