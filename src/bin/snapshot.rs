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
use cartridge_launcher::{run_launcher_with_config, LauncherConfig, ScriptStep};

fn assets_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    let candidates = [cwd.join("assets"), cwd.join("../assets")];
    for c in &candidates {
        if c.join("fonts").exists() {
            return c.clone();
        }
    }
    panic!("Could not find assets directory");
}

fn output_dir() -> PathBuf {
    PathBuf::from("snapshots")
}

fn snap_home(assets: &PathBuf, dir: &PathBuf) -> Result<(), String> {
    println!("Capturing: home");
    let config = LauncherConfig {
        max_frames: Some(40),
        uncapped: true,
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
        capture_dir: Some(dir.clone()),
        capture_frames: vec![60],
        script: vec![
            ScriptStep { buttons: vec![], frames_after: 30 },
            ScriptStep { buttons: vec![Button::Y], frames_after: 30 },
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
        capture_dir: Some(dir.clone()),
        capture_frames: vec![60],
        script: vec![
            ScriptStep { buttons: vec![], frames_after: 30 },
            ScriptStep { buttons: vec![Button::Start], frames_after: 30 },
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
        capture_dir: Some(dir.clone()),
        capture_frames: vec![60],
        script: vec![
            ScriptStep { buttons: vec![], frames_after: 30 },
            ScriptStep { buttons: vec![Button::Select], frames_after: 30 },
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
        capture_dir: Some(dir.clone()),
        capture_frames: vec![140],
        script: vec![
            ScriptStep { buttons: vec![], frames_after: 30 },
            ScriptStep { buttons: vec![Button::Y], frames_after: 30 },
            ScriptStep { buttons: vec![Button::A], frames_after: 60 },
        ],
        ..Default::default()
    };
    let (_, _) = run_launcher_with_config(assets, config)?;
    rename(&dir.join("frame_0140.png"), &dir.join("app_detail.png"));
    Ok(())
}

fn rename(from: &PathBuf, to: &PathBuf) {
    if from.exists() {
        let _ = std::fs::rename(from, to);
    }
}

fn main() -> Result<(), String> {
    env_logger::init();
    // Hidden window + software renderer (read_pixels works reliably).
    unsafe {
        std::env::set_var("CARTRIDGE_HIDDEN", "1");
        std::env::set_var("CARTRIDGE_SOFTWARE", "1");
    }

    let assets = assets_dir();
    let dir = output_dir();
    std::fs::create_dir_all(&dir).ok();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let scenarios = if args.is_empty() {
        vec!["home".into(), "store".into(), "settings".into(), "detail".into(), "power_menu".into()]
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
            other => eprintln!("unknown scenario: {other}"),
        }
    }

    println!("\nSnapshots saved to: {}", dir.display());
    Ok(())
}
