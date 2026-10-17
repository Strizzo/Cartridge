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

/// Names supported by older bundled and local cartridge layouts.
pub fn cartridge_name_variants(app_id: &str) -> Vec<String> {
    let short = app_id.rsplit('.').next().unwrap_or(app_id);
    let mut names = vec![
        app_id.to_string(),
        short.to_string(),
        short.replace('-', "_"),
        short.replace('_', "-"),
    ];
    names.dedup();
    names
}

/// Resolve code, manifests and icons from the same cartridge tree. An installed
/// override wins across ALL name variants; removing it reveals the bundle.
pub fn resolve_cartridge_dir(app_id: &str) -> Option<PathBuf> {
    resolve_cartridge_dir_in(app_id, &installed_apps_dir(), &bundled_cartridges_dir())
}

/// Explicit roots keep filesystem discovery testable without changing HOME.
pub fn resolve_cartridge_dir_in(app_id: &str, installed: &Path, bundled: &Path) -> Option<PathBuf> {
    if app_id.is_empty() || app_id == "." || app_id == ".." || app_id.contains(['/', '\\']) {
        return None;
    }
    let variants = cartridge_name_variants(app_id);
    for root in [installed, bundled] {
        for name in &variants {
            let dir = root.join(name);
            // A half-extracted directory or a same-short-name cartridge must
            // never mask the working app. The manifest is the identity source.
            let Ok(bytes) = std::fs::read(dir.join("cartridge.json")) else {
                continue;
            };
            let Ok(meta) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            if meta.get("id").and_then(|v| v.as_str()) == Some(app_id)
                && has_launchable_entry(&dir, &meta)
            {
                return Some(dir);
            }
        }
    }
    None
}

/// Match the runner's required entry field, but reject a partial tree or an
/// entry that escapes it (including through a symlink). Discovery must not
/// select an unusable legacy installation ahead of the working bundle.
fn has_launchable_entry(dir: &Path, meta: &serde_json::Value) -> bool {
    use std::path::Component;
    let Some(entry) = meta.get("entry").and_then(|v| v.as_str()) else {
        return false;
    };
    if entry.is_empty()
        || entry.contains('\\')
        || Path::new(entry)
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return false;
    }
    let (Ok(root), Ok(path)) = (dir.canonicalize(), dir.join(entry).canonicalize()) else {
        return false;
    };
    path.starts_with(root) && path.is_file()
}

#[cfg(test)]
mod cartridge_paths_tests {
    use super::*;
    #[test]
    fn override_wins_across_aliases_and_removal_reveals_bundle() {
        let root = std::env::temp_dir().join(format!("cartridge-paths-{}", std::process::id()));
        let installed = root.join("apps");
        let bundled = root.join("bundled");
        let id = "dev.cartridge.mission-control";
        let bundle = bundled.join("mission_control");
        let local = installed.join(id);
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        let manifest = serde_json::json!({"id": id, "name":"Mission Control", "version":"1.0.0", "author":"Test", "entry":"main.lua"});
        std::fs::write(bundle.join("cartridge.json"), manifest.to_string()).unwrap();
        std::fs::write(bundle.join("main.lua"), "return {}").unwrap();
        assert_eq!(
            resolve_cartridge_dir_in(id, &installed, &bundled),
            Some(bundle.clone())
        );
        std::fs::write(local.join("cartridge.json"), manifest.to_string()).unwrap();
        // A preexisting partially extracted legacy install does not win.
        assert_eq!(
            resolve_cartridge_dir_in(id, &installed, &bundled),
            Some(bundle.clone())
        );
        std::fs::write(local.join("main.lua"), "return {}").unwrap();
        assert_eq!(
            resolve_cartridge_dir_in(id, &installed, &bundled),
            Some(local.clone())
        );
        std::fs::remove_dir_all(local).unwrap();
        assert_eq!(
            resolve_cartridge_dir_in(id, &installed, &bundled),
            Some(bundle)
        );
        assert!(resolve_cartridge_dir_in("../escape", &installed, &bundled).is_none());
        assert!(resolve_cartridge_dir_in("other.mission-control", &installed, &bundled).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_or_escaping_entries_cannot_mask_a_working_bundle() {
        let root =
            std::env::temp_dir().join(format!("cartridge-invalid-paths-{}", std::process::id()));
        let installed = root.join("apps");
        let bundled = root.join("bundled");
        let id = "dev.cartridge.test";
        let bundle = bundled.join("test");
        let local = installed.join(id);
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::create_dir_all(local.join("src")).unwrap();
        std::fs::write(bundle.join("main.lua"), "return {}").unwrap();
        let mut manifest = serde_json::json!({"id":id,"name":"Test","version":"1.0.0","author":"Test","entry":"main.lua"});
        std::fs::write(bundle.join("cartridge.json"), manifest.to_string()).unwrap();
        let outside = root.join("outside.lua");
        std::fs::write(&outside, "return {}").unwrap();
        for entry in [
            serde_json::Value::Null,
            serde_json::json!(""),
            serde_json::json!("../outside.lua"),
            serde_json::json!(outside.to_string_lossy()),
            serde_json::json!("src"),
            serde_json::json!("missing.lua"),
            serde_json::json!("..\\outside.lua"),
        ] {
            manifest["entry"] = entry;
            std::fs::write(local.join("cartridge.json"), manifest.to_string()).unwrap();
            assert_eq!(
                resolve_cartridge_dir_in(id, &installed, &bundled),
                Some(bundle.clone())
            );
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, local.join("escape.lua")).unwrap();
            manifest["entry"] = serde_json::json!("escape.lua");
            std::fs::write(local.join("cartridge.json"), manifest.to_string()).unwrap();
            assert_eq!(
                resolve_cartridge_dir_in(id, &installed, &bundled),
                Some(bundle.clone())
            );
        }
        std::fs::write(local.join("src/main.lua"), "return {}").unwrap();
        manifest["entry"] = serde_json::json!("src/main.lua");
        std::fs::write(local.join("cartridge.json"), manifest.to_string()).unwrap();
        assert_eq!(
            resolve_cartridge_dir_in(id, &installed, &bundled),
            Some(local)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
