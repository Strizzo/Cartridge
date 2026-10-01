//! Focused native app scenarios, isolated storage, real 720×720 screenshots.
use cartridge_core::input::Button;
use cartridge_lua::{manifest::CartridgeManifest, run_lua_app_with_config, LuaAppConfig};
use std::path::{Path, PathBuf};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn button(value: &str) -> Result<Button, String> {
    Ok(match value {
        "up" => Button::DpadUp,
        "down" => Button::DpadDown,
        "left" => Button::DpadLeft,
        "right" => Button::DpadRight,
        "a" => Button::A,
        "b" => Button::B,
        "x" => Button::X,
        "y" => Button::Y,
        "l1" => Button::L1,
        "r1" => Button::R1,
        "l2" => Button::L2,
        "r2" => Button::R2,
        "start" => Button::Start,
        "select" => Button::Select,
        _ => return Err(format!("Unknown button {value}")),
    })
}
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let name = args.next().ok_or("Usage: app-check APP [--fixture FILE] [--seed KEY=JSON] [--press FRAME:BUTTON] [--frames N] [--capture N,N] [--out DIR] [--visible]")?;
    let app = if Path::new(&name).is_dir() {
        PathBuf::from(&name)
    } else {
        cartridge_core::paths::bundled_cartridges_dir().join(&name)
    };
    let manifest = CartridgeManifest::load(&app)?;
    let mut fixture = None;
    let mut seeds = Vec::new();
    let mut script = Vec::new();
    let mut frames = 90u64;
    let mut capture = Vec::new();
    let mut visible = false;
    let mut out = PathBuf::from(".sim/app-check").join(app.file_name().ok_or("Invalid app path")?);
    while let Some(flag) = args.next() {
        if flag == "--visible" {
            visible = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {flag}"))?;
        match flag.as_str() {
            "--fixture" => fixture = Some(PathBuf::from(value)),
            "--seed" => {
                let (key, path) = value.split_once('=').ok_or("Expected --seed KEY=JSON")?;
                if key.is_empty()
                    || !key
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
                {
                    return Err("Invalid storage key".into());
                }
                seeds.push((key.to_string(), PathBuf::from(path)));
            }
            "--press" => {
                let (frame, key) = value
                    .split_once(':')
                    .ok_or("Expected --press FRAME:BUTTON")?;
                script.push((
                    frame.parse::<u64>().map_err(|_| "Invalid input frame")?,
                    button(key)?,
                ));
            }
            "--frames" => frames = value.parse().map_err(|_| "Invalid frame count")?,
            "--capture" => {
                for frame in value.split(',') {
                    capture.push(frame.parse::<u64>().map_err(|_| "Invalid capture frame")?);
                }
            }
            "--out" => out = PathBuf::from(value),
            _ => return Err(format!("Unknown argument {flag}")),
        }
    }
    if !(2..=3600).contains(&frames) {
        return Err("Frames must be 2..3600".into());
    }
    if capture.is_empty() {
        capture.push(frames - 1);
    }
    if capture.iter().any(|n| *n >= frames) || script.iter().any(|(n, _)| *n >= frames) {
        return Err("Capture/input frames must be smaller than --frames".into());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let storage = Scratch(std::env::temp_dir().join(format!(
        "cartridge-app-check-{}-{nonce}",
        std::process::id()
    )));
    let root = storage.0.join(".cartridges");
    let data = root.join(&manifest.id).join("data");
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    for (key, path) in seeds {
        std::fs::copy(path, data.join(format!("{key}.json"))).map_err(|e| e.to_string())?;
    }
    // This dedicated binary is single-threaded before starting SDL/app workers.
    unsafe {
        std::env::set_var("CARTRIDGE_SIM", "1");
        std::env::set_var("CARTRIDGE_HOME", &storage.0);
        std::env::set_var("CARTRIDGE_SOFTWARE", "1");
        std::env::set_var("CARTRIDGE_HIDDEN", if visible { "0" } else { "1" });
        std::env::set_var("CARTRIDGE_FPS", "0");
    }
    let stats = run_lua_app_with_config(
        &app,
        &cartridge_core::paths::assets_dir(),
        LuaAppConfig {
            hidden: !visible,
            uncapped: fixture.is_some(),
            max_frames: Some(frames),
            capture_dir: Some(out.clone()),
            capture_frames: capture.clone(),
            http_fixture: fixture,
            storage_root: Some(root),
            script,
            fail_on_error: true,
            print_stats: true,
        },
    )?;
    if stats.frames != frames {
        return Err(format!(
            "App exited after {} of {frames} expected frames",
            stats.frames
        ));
    }
    for frame in capture {
        if !out.join(format!("frame_{frame:04}.png")).is_file() {
            return Err(format!("Missing requested capture at frame {frame}"));
        }
    }
    println!(
        "{}: {} frames, {} renders; screenshots {}",
        manifest.name,
        stats.frames,
        stats.rendered_frames,
        out.display()
    );
    Ok(())
}
fn main() {
    env_logger::init();
    if let Err(error) = run() {
        eprintln!("App check failed: {error}");
        std::process::exit(1);
    }
}
