pub mod app;
pub mod data;
pub mod screens;
pub mod ui_constants;

use cartridge_core::atmosphere::Atmosphere;
use cartridge_core::font::FontCache;
use cartridge_core::image_cache::ImageCache;
use cartridge_core::input::InputManager;
use cartridge_core::screen::{Screen, HEIGHT, WIDTH};
use cartridge_core::theme::Theme;
use std::path::{Path, PathBuf};
use std::time::Instant;

use app::LauncherApp;

const TARGET_FPS: u32 = 30;

/// The result of running the launcher -- either the user quit, or they want to launch an app.
pub enum LauncherResult {
    /// User closed the launcher (Escape or window close).
    Quit,
    /// User wants to launch a Lua app at this path.
    LaunchApp(PathBuf),
}

/// Run the Cartridge launcher UI.
pub fn run_launcher(assets_dir: &Path) -> Result<LauncherResult, String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let joystick_subsystem = sdl_context.joystick()?;
    let _joysticks = cartridge_core::input::open_all_joysticks(&joystick_subsystem);

    let window = video_subsystem
        .window("Cartridge", WIDTH, HEIGHT)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .map_err(|e| e.to_string())?;

    let texture_creator = canvas.texture_creator();
    let mut fonts = FontCache::new(assets_dir)?;
    fonts.prewarm();
    let mut images = ImageCache::new(&texture_creator)?;

    let theme = Theme::default();
    let mut input_manager = InputManager::new();
    let mut event_pump = sdl_context.event_pump()?;

    let mut launcher = LauncherApp::new(assets_dir);
    let mut atmosphere = Atmosphere::new();
    let mut last_frame = Instant::now();
    let mut sysinfo_timer = 0.0_f32;

    loop {
        let frame_start = Instant::now();
        let dt = frame_start.duration_since(last_frame).as_secs_f32();
        last_frame = frame_start;
        atmosphere.update(dt);

        // Poll system info every 2 seconds
        sysinfo_timer += dt;
        if sysinfo_timer >= 2.0 {
            sysinfo_timer -= 2.0;
            launcher.poll_sysinfo();
        }

        // Collect SDL events
        let events: Vec<sdl2::event::Event> = event_pump.poll_iter().collect();

        // Check for quit / escape
        for event in &events {
            match event {
                sdl2::event::Event::Quit { .. } => return Ok(LauncherResult::Quit),
                sdl2::event::Event::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Escape),
                    ..
                } => return Ok(LauncherResult::Quit),
                _ => {}
            }
        }

        // Process input
        let input_events = input_manager.process_events(&events);
        if launcher.handle_input(&input_events) {
            // Check if the launcher wants to launch an app
            if let Some(app_id) = launcher.pending_launch() {
                let app_dir = resolve_app_dir(app_id, assets_dir);
                return Ok(LauncherResult::LaunchApp(app_dir));
            }
            return Ok(LauncherResult::Quit);
        }

        // Render
        {
            let mut screen = Screen {
                canvas: &mut canvas,
                theme: &theme,
                fonts: &mut fonts,
                images: &mut images,
                texture_creator: &texture_creator,
            };
            launcher.render(&mut screen, &atmosphere);
        }

        canvas.present();

        // Frame rate cap
        let frame_time = Instant::now().duration_since(frame_start);
        let target_time = std::time::Duration::from_secs_f64(1.0 / TARGET_FPS as f64);
        if frame_time < target_time {
            std::thread::sleep(target_time - frame_time);
        }
    }
}

/// Resolve the directory for an installed app given its id.
///
/// Checks both the full app_id and the short name (last segment of dotted ID):
/// 1. `lua_cartridges/{name}/` relative to the binary (bundled — preferred, always up to date)
/// 2. `~/.cartridges/apps/{name}/` (user-installed from store)
fn resolve_app_dir(app_id: &str, _assets_dir: &Path) -> PathBuf {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    // Bundled path: next to the running binary
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let cwd = std::env::current_dir().unwrap_or_default();

    for name in crate::ui_constants::name_variants(app_id) {
        // Prefer bundled/dev path (always up to date with the binary)
        if let Some(ref dir) = exe_dir {
            let bundled = dir.join("lua_cartridges").join(&name);
            if bundled.exists() {
                return bundled;
            }
        }
        let dev_path = cwd.join("lua_cartridges").join(&name);
        if dev_path.exists() {
            return dev_path;
        }

        // Fall back to user-installed path
        let installed_path = home.join(".cartridges/apps").join(&name);
        if installed_path.exists() {
            return installed_path;
        }
    }

    // Fall back to the installed path
    home.join(".cartridges/apps").join(app_id)
}
