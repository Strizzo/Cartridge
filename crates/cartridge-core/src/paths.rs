//! Filesystem path resolution shared by every crate.
//!
//! - `home_dir()`               `CARTRIDGE_HOME`, else `HOME`, else `.`
//! - `cartridges_dir()`         `<home>/.cartridges`
//! - `assets_dir()`             `CARTRIDGE_ASSETS`, else cwd/exe-dir probing
//! - `bundled_cartridges_dir()` `lua_cartridges/` next to the binary, else cwd
//!
//! The simulator points `CARTRIDGE_HOME` at a scratch directory so desktop
//! runs never touch the real `~/.cartridges`, and `CARTRIDGE_ASSETS` at the
//! repo's `assets/` so the binary can live anywhere under `target/`.

use std::path::{Path, PathBuf};

/// Home directory used for all `~/.cartridges` state.
pub fn home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("CARTRIDGE_HOME") {
        let h = h.trim();
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// `<home>/.cartridges` -- root of all persisted state (settings, app data,
/// installed apps, boot state).
pub fn cartridges_dir() -> PathBuf {
    home_dir().join(".cartridges")
}

/// `<home>/.cartridges/apps` -- store-installed cartridges.
pub fn installed_apps_dir() -> PathBuf {
    cartridges_dir().join("apps")
}

/// `<home>/.ssh` -- scanned for well-known SSH keys.
pub fn ssh_dir() -> PathBuf {
    home_dir().join(".ssh")
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

fn looks_like_assets(dir: &Path) -> bool {
    dir.join("fonts").is_dir()
}

/// Resolve the assets directory (fonts, overlays, boot logo).
///
/// Order: `CARTRIDGE_ASSETS` (if set), `./assets`, `<exe>/assets`,
/// `<exe>/../assets`, `../assets`. Falls back to `./assets` even when it
/// does not exist so callers get a deterministic path to report.
pub fn assets_dir() -> PathBuf {
    if let Ok(a) = std::env::var("CARTRIDGE_ASSETS") {
        let a = a.trim();
        if !a.is_empty() {
            let p = PathBuf::from(a);
            if looks_like_assets(&p) {
                return p;
            }
            log::warn!(
                "CARTRIDGE_ASSETS={} has no fonts/ directory; probing defaults",
                p.display()
            );
        }
    }

    let cwd = std::env::current_dir().unwrap_or_default();
    let mut candidates: Vec<PathBuf> = vec![cwd.join("assets")];
    if let Some(dir) = exe_dir() {
        candidates.push(dir.join("assets"));
        if let Some(parent) = dir.parent() {
            candidates.push(parent.join("assets"));
        }
    }
    candidates.push(cwd.join("../assets"));

    for c in &candidates {
        if looks_like_assets(c) {
            return c.clone();
        }
    }
    cwd.join("assets")
}

/// Directory holding bundled Lua cartridges. `lua_cartridges/` next to the
/// binary wins (device layout: `/roms/Cartridge/lua_cartridges`), then the
/// current working directory (dev layout: repo root).
pub fn bundled_cartridges_dir() -> PathBuf {
    if let Some(dir) = exe_dir() {
        let bundled = dir.join("lua_cartridges");
        if bundled.is_dir() {
            return bundled;
        }
    }
    std::env::current_dir()
        .unwrap_or_default()
        .join("lua_cartridges")
}

/// Directory where F12 / SIGUSR1 screenshots are written: `screenshots/`
/// under the current working directory (repo root in the simulator,
/// `/roms/Cartridge` on the device).
pub fn screenshots_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join("screenshots")
}
