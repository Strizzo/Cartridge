//! Snapshot tool: runs the launcher headlessly through scripted scenarios
//! and dumps PNG captures of each screen to `snapshots/`.
//!
//! Usage:
//!     cargo run --bin snapshot
//!     cargo run --bin snapshot home store settings
//!
//! By default captures: home, store, settings, wifi, detail.
//! Useful for:
//!   - Visual regression detection (run before+after a change, diff PNGs)
//!   - Reviewing UI changes without flashing the device

use std::path::PathBuf;

use cartridge_core::input::Button;
use cartridge_launcher::{LauncherConfig, ScriptStep, run_launcher_with_config};

fn assets_dir() -> PathBuf {
    let dir = cartridge_core::paths::assets_dir();
    if !dir.join("fonts").exists() {
        panic!("Could not find assets directory (tried {})", dir.display());
    }
    dir
}

fn output_dir() -> PathBuf {
    PathBuf::from("snapshots")
}

fn snap_home(assets: &PathBuf, dir: &PathBuf) -> Result<(), String> {
    println!("Capturing: home");
    let config = LauncherConfig {
        max_frames: Some(40),
        uncapped: true,
        script_wait_for_background: true,
        capture_dir: Some(dir.clone()),
        capture_frames: vec![30],
        ..Default::default()
    };
    let (_, _) = run_launcher_with_config(assets, config)?;
    rename(&dir.join("frame_0030.png"), &dir.join("home.png"));
    Ok(())
}

fn snap_store(assets: &PathBuf, dir: &PathBuf) -> Result<(), String> {
    println!("Capturing: store");
    let config = LauncherConfig {
        max_frames: Some(80),
        uncapped: true,
        script_wait_for_background: true,
        capture_dir: Some(dir.clone()),
        capture_frames: vec![60],
        script: vec![
            ScriptStep {
                buttons: vec![],
                frames_after: 30,
            },
            ScriptStep {
                buttons: vec![Button::Y],
                frames_after: 30,
            },
        ],
        ..Default::default()
    };
    let (_, _) = run_launcher_with_config(assets, config)?;
    rename(&dir.join("frame_0060.png"), &dir.join("store.png"));
    Ok(())
}

fn snap_settings(assets: &PathBuf, dir: &PathBuf) -> Result<(), String> {
    println!("Capturing: settings");
    let config = LauncherConfig {
        max_frames: Some(80),
        uncapped: true,
        script_wait_for_background: true,
        capture_dir: Some(dir.clone()),
        capture_frames: vec![60],
        script: vec![
            ScriptStep {
                buttons: vec![],
                frames_after: 30,
            },
            ScriptStep {
                buttons: vec![Button::Start],
                frames_after: 30,
            },
        ],
        ..Default::default()
    };
    let (_, _) = run_launcher_with_config(assets, config)?;
    rename(&dir.join("frame_0060.png"), &dir.join("settings.png"));
    Ok(())
}

fn snap_power_menu(assets: &PathBuf, dir: &PathBuf) -> Result<(), String> {
    println!("Capturing: power_menu");
    // Press Select to bring up the system menu (BootOverlay).
    let config = LauncherConfig {
        max_frames: Some(80),
        uncapped: true,
        script_wait_for_background: true,
        capture_dir: Some(dir.clone()),
        capture_frames: vec![60],
        script: vec![
            ScriptStep {
                buttons: vec![],
                frames_after: 30,
            },
            ScriptStep {
                buttons: vec![Button::Select],
                frames_after: 30,
            },
        ],
        ..Default::default()
    };
    let (_, _) = run_launcher_with_config(assets, config)?;
    rename(&dir.join("frame_0060.png"), &dir.join("power_menu.png"));
    Ok(())
}

fn snap_detail(assets: &PathBuf, dir: &PathBuf) -> Result<(), String> {
    println!("Capturing: app_detail");
    // Open the store, navigate to first app, press A to open detail
    let config = LauncherConfig {
        max_frames: Some(160),
        uncapped: true,
        script_wait_for_background: true,
        capture_dir: Some(dir.clone()),
        capture_frames: vec![140],
        script: vec![
            ScriptStep {
                buttons: vec![],
                frames_after: 30,
            },
            ScriptStep {
                buttons: vec![Button::Y],
                frames_after: 30,
            },
            ScriptStep {
                buttons: vec![Button::A],
                frames_after: 60,
            },
        ],
        ..Default::default()
    };
    let (_, _) = run_launcher_with_config(assets, config)?;
    rename(&dir.join("frame_0140.png"), &dir.join("app_detail.png"));
    Ok(())
}

fn snap_navigation(assets: &PathBuf, dir: &PathBuf, name: &str, buttons: Vec<Button>) -> Result<(), String> {
    println!("Capturing: {name}");
    let capture = buttons.len() as u64 * 4 + 20;
    let script = std::iter::once(ScriptStep { buttons: vec![], frames_after: 4 })
        .chain(buttons.into_iter().map(|button| ScriptStep { buttons: vec![button], frames_after: 4 }))
        .collect();
    run_launcher_with_config(assets, LauncherConfig {
        max_frames: Some(capture + 5), uncapped: true, script_wait_for_background: true,
        capture_dir: Some(dir.clone()), capture_frames: vec![capture], script,
        ..Default::default()
    })?;
    rename(&dir.join(format!("frame_{capture:04}.png")), &dir.join(format!("{name}.png")));
    Ok(())
}

fn rename(from: &PathBuf, to: &PathBuf) {
    if from.exists() {
        let _ = std::fs::rename(from, to);
    }
}

fn main() -> Result<(), String> {
    env_logger::init();
    // Isolated deterministic device state; snapshots must not read or mutate
    // the developer's real installed apps, recents, network or power settings.
    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "cartridge-snapshots-{}-{nonce}",
        std::process::id()
    )));
    std::fs::create_dir_all(&scratch.0).map_err(|e| e.to_string())?;
    // Hidden window + software renderer (read_pixels works reliably).
    unsafe {
        std::env::set_var("CARTRIDGE_SIM", "1");
        std::env::set_var("CARTRIDGE_HOME", &scratch.0);
        std::env::set_var("CARTRIDGE_SIM_PROFILE", "sim/profiles/r36s-plus.json");
        std::env::set_var("CARTRIDGE_SIM_HOSTNAME", "r36s-plus");
        std::env::set_var("CARTRIDGE_SIM_CLOCK", "12:00");
        std::env::set_var("CARTRIDGE_SIM_BATTERY", "72");
        std::env::set_var("CARTRIDGE_SIM_WIFI", "HomeNet");
        std::env::set_var("CARTRIDGE_HIDDEN", "1");
        std::env::set_var("CARTRIDGE_SOFTWARE", "1");
    }

    let assets = assets_dir();
    let dir = output_dir();
    std::fs::create_dir_all(&dir).ok();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let scenarios = if args.is_empty() {
        vec![
            "home".into(),
            "store".into(),
            "settings".into(),
            "detail".into(),
            "power_menu".into(),
            "store_installed".into(),
            "store_updates".into(),
            "settings_about".into(),
            "wifi".into(),
        ]
    } else {
        args
    };

    for s in &scenarios {
        match s.as_str() {
            "home" => snap_home(&assets, &dir)?,
            "store" => snap_store(&assets, &dir)?,
            "settings" => snap_settings(&assets, &dir)?,
            "detail" => snap_detail(&assets, &dir)?,
            "power_menu" => snap_power_menu(&assets, &dir)?,
            "store_installed" => snap_navigation(&assets, &dir, s, vec![Button::Y, Button::R1])?,
            "store_updates" => snap_navigation(&assets, &dir, s, vec![Button::Y, Button::R1, Button::R1])?,
            "settings_about" => snap_navigation(&assets, &dir, s, std::iter::once(Button::Start).chain(std::iter::repeat_n(Button::DpadDown, 10)).collect())?,
            "wifi" => snap_navigation(&assets, &dir, s, std::iter::once(Button::Start).chain(std::iter::repeat_n(Button::DpadDown, 7)).chain([Button::A]).collect())?,
            other => eprintln!("unknown scenario: {other}"),
        }
    }

    println!("\nSnapshots saved to: {}", dir.display());
    Ok(())
}
