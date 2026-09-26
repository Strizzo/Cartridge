//! Run the real renderer/input/app loops with deterministic desktop scenarios.
use cartridge_core::input::Button;
use cartridge_launcher::{LauncherConfig, LauncherResult, ScriptStep, run_launcher_with_config};
use std::path::PathBuf;

fn scenario(
    name: &str,
    buttons: &[Button],
    out: &std::path::Path,
) -> Result<LauncherResult, String> {
    scenario_resuming(name, buttons, out, None)
}

fn scenario_resuming(
    name: &str,
    buttons: &[Button],
    out: &std::path::Path,
    resume: Option<cartridge_launcher::games::GameRequest>,
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
            resume_game: resume,
            script_wait_for_background: true,
            max_frames: Some(35),
            uncapped: true,
            capture_dir: Some(dir.clone()),
            capture_frames: vec![25],
            script,
            ..Default::default()
        },
    )?;
    if name != "handoff" && name != "game-launch" {
        check_png(&dir.join("frame_0025.png"))?;
    }
    println!(
        "{name}: {} frames, host render p95 {:.2}ms",
        stats.frames, stats.render_ms_p95
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

/// Exercise the same SDL pacing inbox used by both interactive loops. This
/// deliberately requests a long idle wait, then sends real SDL button events.
fn check_input_wake() -> Result<(), String> {
    use cartridge_core::event_wait::EventInbox;
    use cartridge_core::input::{InputAction, InputManager};
    use sdl2::{
        event::Event,
        keyboard::{Keycode, Mod},
    };
    use std::time::{Duration, Instant};
    let sdl = sdl2::init()?;
    let subsystem = sdl.event()?;
    let mut pump = sdl.event_pump()?;
    let mut inbox = EventInbox::default();
    inbox.collect(&mut pump);
    let sender = subsystem.event_sender();
    let worker = std::thread::spawn(move || -> Result<Instant, String> {
        std::thread::sleep(Duration::from_millis(30));
        let sent = Instant::now();
        sender.push_event(Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(Keycode::Z),
            scancode: None,
            keymod: Mod::NOMOD,
            repeat: false,
        })?;
        sender.push_event(Event::KeyUp {
            timestamp: 0,
            window_id: 0,
            keycode: Some(Keycode::Z),
            scancode: None,
            keymod: Mod::NOMOD,
            repeat: false,
        })?;
        Ok(sent)
    });
    inbox.wait(&mut pump, Duration::from_secs(1));
    let woke = Instant::now();
    let sent = worker.join().map_err(|_| "Input sender panicked")??;
    if woke < sent || woke.duration_since(sent) > Duration::from_millis(500) {
        return Err("Idle pacing slept through an SDL button event".into());
    }
    // Re-entering wait must not overwrite its pending event.
    inbox.wait(&mut pump, Duration::from_millis(20));
    let mut input = InputManager::new();
    let buttons = input.process_events(inbox.collect(&mut pump));
    if buttons.len() != 2
        || buttons[0].button != Button::A
        || buttons[0].action != InputAction::Press
        || buttons[1].action != InputAction::Release
    {
        return Err("Idle wake lost, reordered or duplicated a button event".into());
    }
    if !input.process_events(inbox.collect(&mut pump)).is_empty() {
        return Err("Idle wake delivered the same input twice".into());
    }
    let empty = Instant::now();
    inbox.wait(&mut pump, Duration::from_millis(40));
    if empty.elapsed() < Duration::from_millis(20) {
        return Err("Idle pacing did not wait without input".into());
    }
    println!(
        "SDL idle wake passed: {:.2}ms from queued button to wake",
        woke.saturating_duration_since(sent).as_secs_f64() * 1000.0
    );
    Ok(())
}

/// Every app scenario starts with empty storage, including after a previous
/// city-change scenario. Never delete or overwrite the interactive sim home.
struct ScenarioStorage(PathBuf);
impl ScenarioStorage {
    fn new() -> Result<Self, String> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("cartridge-scenario-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        Ok(Self(root))
    }
}
impl Drop for ScenarioStorage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
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
    check_input_wake()?;
    for (name, buttons) in [
        ("home", vec![]),
        ("settings", vec![Button::Start]),
        ("store", vec![Button::Y]),
        ("systems", vec![Button::L2]),
        ("games", vec![Button::L2, Button::A]),
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
    let game = match scenario(
        "game-launch",
        &[Button::L2, Button::A, Button::DpadDown, Button::A],
        &out,
    )? {
        LauncherResult::LaunchGame(game) => game,
        _ => return Err("Game selection did not request an emulator launch".into()),
    };
    cartridge_launcher::games::launch(&game)?;
    let report = std::fs::read_to_string(home.join(".cartridges/games/last-launch.json"))
        .map_err(|e| e.to_string())?;
    if !report.contains("\"simulated\": true") {
        return Err("Game launch was not isolated to the simulator".into());
    }
    if !matches!(
        scenario_resuming("game-return", &[], &out, Some(game.clone()))?,
        LauncherResult::Quit
    ) {
        return Err("Game return did not restore the library".into());
    }
    match scenario_resuming("game-launch", &[Button::A], &out, Some(game.clone()))? {
        LauncherResult::LaunchGame(restored)
            if restored.system == game.system && restored.path == game.path =>
        {
            ()
        }
        _ => return Err("Game return lost the selected system or ROM".into()),
    }
    let fixture = cartridge_core::paths::assets_dir()
        .parent()
        .unwrap()
        .join("sim/fixtures/http.json");
    for (name, app, buttons) in [
        ("todo", "todo", vec![]),
        ("calculator", "calculator", vec![]),
        ("pomodoro", "pomodoro", vec![]),
        ("system-monitor", "system_monitor", vec![]),
        ("papers", "ai_papers", vec![]),
        ("paper-detail", "ai_papers", vec![Button::A]),
        ("paper-loading", "ai_papers", vec![Button::A, Button::X]),
        ("paper-reader", "ai_papers", vec![Button::A, Button::X]),
        ("network", "network_tool", vec![]),
        (
            "network-dns",
            "network_tool",
            vec![Button::R1, Button::R1, Button::A],
        ),
        (
            "network-probes",
            "network_tool",
            vec![Button::L1, Button::A],
        ),
        ("hn-offline", "hacker_news", vec![]),
        ("stocks-offline", "stock_market", vec![]),
        ("weather-offline", "weather", vec![]),
        ("weather-current-data", "weather", vec![]),
        ("weather-forecast-data", "weather", vec![Button::R1]),
        (
            "weather-city-data",
            "weather",
            vec![Button::R1, Button::R1, Button::DpadDown, Button::A],
        ),
        ("hn-list-data", "hacker_news", vec![]),
        ("hn-detail-data", "hacker_news", vec![Button::A]),
        ("stocks-list-data", "stock_market", vec![]),
        ("stocks-detail-data", "stock_market", vec![Button::A]),
        (
            "stocks-period-data",
            "stock_market",
            vec![Button::A, Button::R1],
        ),
    ] {
        let storage = ScenarioStorage::new()?;
        let dir = out.join(name);
        let capture = if name == "paper-loading" { 15 } else { 32 };
        let script = buttons
            .into_iter()
            .enumerate()
            .map(|(i, b)| (8 + i as u64 * 4, b))
            .collect();
        let stats = cartridge_lua::run_lua_app_with_config(
            &cartridge_core::paths::bundled_cartridges_dir().join(app),
            &cartridge_core::paths::assets_dir(),
            cartridge_lua::LuaAppConfig {
                hidden: true,
                uncapped: true,
                max_frames: Some(40),
                capture_dir: Some(dir.clone()),
                capture_frames: vec![capture],
                http_fixture: Some(if name.ends_with("-data") {
                    fixture.with_file_name("populated.json")
                } else {
                    fixture.clone()
                }),
                storage_root: Some(storage.0.clone()),
                script,
                fail_on_error: true,
                ..Default::default()
            },
        )?;
        check_png(&dir.join(format!("frame_{capture:04}.png")))?;
        println!(
            "{name}: {} loops, {} renders, host render p95 {:.2}ms",
            stats.frames, stats.rendered_frames, stats.render_ms_p95
        );
        if name == "todo" && stats.rendered_frames > 3 {
            return Err("Static app is redrawing during clean idle iterations".into());
        }
    }
    println!(
        "SIMULATOR CHECK PASSED: native 720x720 rendering, navigation, games and selection restore, Lua app, readiness, ES handoff.\nScreenshots: {}",
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
