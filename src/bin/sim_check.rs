//! Run the real renderer/input/app loops with deterministic desktop scenarios.
use cartridge_core::input::Button;
use cartridge_launcher::{LauncherConfig, LauncherResult, ScriptStep, run_launcher_with_config};
use std::path::PathBuf;

fn scenario(
    name: &str,
    buttons: &[Button],
    out: &std::path::Path,
) -> Result<LauncherResult, String> {
    let dir = out.join(name);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut script = vec![ScriptStep {
        buttons: vec![],
        frames_after: 4,
    }];
    for button in buttons {
        script.push(ScriptStep {
            buttons: vec![*button],
            frames_after: 3,
        });
    }
    let (result, stats) = run_launcher_with_config(
        &cartridge_core::paths::assets_dir(),
        LauncherConfig {
            max_frames: Some(35),
            uncapped: true,
            capture_dir: Some(dir.clone()),
            capture_frames: vec![25],
            script,
            ..Default::default()
        },
    )?;
    if name != "handoff" {
        check_png(&dir.join("frame_0025.png"))?;
    }
    println!(
        "{name}: {} frames, host p95 {:.2}ms",
        stats.frames, stats.frame_ms_p95
    );
    Ok(result)
}

fn check_png(path: &std::path::Path) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if data.len() < 24
        || &data[..8] != b"\x89PNG\r\n\x1a\n"
        || u32::from_be_bytes(data[16..20].try_into().unwrap()) != 720
        || u32::from_be_bytes(data[20..24].try_into().unwrap()) != 720
    {
        return Err(format!("Expected a 720x720 PNG: {}", path.display()));
    }
    Ok(())
}

fn run() -> Result<(), String> {
    if !cartridge_core::sim::is_sim() {
        return Err(
            "Use ./sim.sh check; this command requires the isolated simulator environment".into(),
        );
    }
    // Exercise device controls through the real public APIs. On Linux too,
    // simulator mode must never touch NetworkManager/backlight/audio hardware.
    cartridge_core::device::set_brightness_percent(43);
    cartridge_core::device::set_volume_percent(37);
    if cartridge_core::device::get_brightness_percent() != 43
        || cartridge_core::device::get_volume_percent() != 37
    {
        return Err("Simulator controls did not retain brightness/volume".into());
    }
    let wifi = cartridge_net::WifiManager::new();
    let before = wifi.status();
    wifi.disconnect()?;
    if !matches!(wifi.status(), cartridge_net::wifi::WifiStatus::Disconnected) {
        return Err("Simulated WiFi did not disconnect".into());
    }
    if let cartridge_net::wifi::WifiStatus::Connected { ssid, .. } = before {
        wifi.connect_with_password(&ssid, "simulated")?;
    }
    let home = cartridge_core::paths::home_dir();
    let out = home.join("checks");
    let ready = std::env::var_os("CARTRIDGE_READY_FILE")
        .map(PathBuf::from)
        .ok_or("Missing simulator readiness path")?;
    let handoff = [Button::Select, Button::DpadUp, Button::A];
    if std::env::args().any(|a| a == "--handoff") {
        let result = scenario("handoff", &handoff, &out)?;
        if !ready.is_file() || !matches!(result, LauncherResult::EmulationStation) {
            return Err("First-frame acknowledgment or EmulationStation handoff failed".into());
        }
        std::process::exit(20);
    }
    for (name, buttons) in [
        ("home", vec![]),
        ("settings", vec![Button::Start]),
        ("store", vec![Button::Y]),
    ] {
        if !matches!(scenario(name, &buttons, &out)?, LauncherResult::Quit) {
            return Err(format!("{name}: unexpected launcher exit"));
        }
    }
    if !ready.is_file() {
        return Err("Launcher never acknowledged its first frame".into());
    }
    if !matches!(
        scenario("handoff", &handoff, &out)?,
        LauncherResult::EmulationStation
    ) {
        return Err("Power menu did not return the EmulationStation handoff".into());
    }
    let app_dir = cartridge_core::paths::bundled_cartridges_dir().join("todo");
    let dir = out.join("todo");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stats = cartridge_lua::run_lua_app_with_config(
        &app_dir,
        &cartridge_core::paths::assets_dir(),
        cartridge_lua::LuaAppConfig {
            hidden: true,
            uncapped: true,
            max_frames: Some(35),
            capture_dir: Some(dir.clone()),
            capture_frames: vec![25],
            ..Default::default()
        },
    )?;
    check_png(&dir.join("frame_0025.png"))?;
    println!(
        "todo: {} frames, host p95 {:.2}ms",
        stats.frames, stats.frame_ms_p95
    );
    println!(
        "SIMULATOR CHECK PASSED: native 720x720 rendering, navigation, Lua app, readiness, ES handoff.\nScreenshots: {}",
        out.display()
    );
    Ok(())
}

fn main() {
    env_logger::init();
    if let Err(e) = run() {
        eprintln!("Simulator check failed: {e}");
        std::process::exit(1);
    }
}
