pub mod app;
pub mod data;
pub mod screens;
pub mod ui_constants;

use cartridge_core::atmosphere::Atmosphere;
use cartridge_core::font::FontCache;
use cartridge_core::image_cache::ImageCache;
use cartridge_core::input::InputManager;
use cartridge_core::screen::{Screen, HEIGHT, WIDTH};
use cartridge_core::text_cache::TextCache;
use cartridge_core::theme::Theme;
use std::path::{Path, PathBuf};
use std::time::Instant;

use app::LauncherApp;

/// Active frame rate (when input has happened recently).
const ACTIVE_FPS: u32 = 30;
/// Idle frame rate (when no input for a while). Saves CPU on battery.
const IDLE_FPS: u32 = 5;
/// Time of last input before considered idle (in seconds).
const IDLE_AFTER_SECS: f32 = 3.0;

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
    let game_controller_subsystem = sdl_context.game_controller()?;
    let _controllers = cartridge_core::input::open_all_controllers(&game_controller_subsystem);

    let window = video_subsystem
        .window("CartridgeOS", WIDTH, HEIGHT)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    // Note: present_vsync() is unreliable on RK3326's fbdev/DRM path and
    // would compound with the sleep-based frame cap below. Rely on the
    // sleep cap alone for predictable timing.
    let mut canvas = window
        .into_canvas()
        .accelerated()
        .build()
        .map_err(|e| e.to_string())?;

    let texture_creator = canvas.texture_creator();
    let mut fonts = FontCache::new(assets_dir)?;
    fonts.prewarm();
    let mut images = ImageCache::new(&texture_creator)?;
    let mut text_cache = TextCache::new(&texture_creator);

    let theme = Theme::default();
    let mut input_manager = InputManager::new();
    if !_controllers.is_empty() {
        input_manager.set_ignore_joystick(true);
    }
    let mut event_pump = sdl_context.event_pump()?;

    let mut launcher = LauncherApp::new(assets_dir);
    let mut atmosphere = Atmosphere::new();
    // Pre-render the static atmosphere into cached textures (3 fullscreen
    // image blits per frame -> 2 cached blits).
    atmosphere.precompose(&mut canvas, &texture_creator, &mut images, &theme);
    let mut last_frame = Instant::now();
    let mut last_input = Instant::now();

    // Optional FPS / frametime overlay enabled via CARTRIDGE_FPS=1
    let show_fps = std::env::var("CARTRIDGE_FPS").ok().as_deref() == Some("1");
    let mut frame_times: std::collections::VecDeque<f32> = std::collections::VecDeque::with_capacity(60);
    let mut last_stats_log = Instant::now();

    loop {
        let frame_start = Instant::now();
        let dt = frame_start.duration_since(last_frame).as_secs_f32();
        last_frame = frame_start;
        atmosphere.update(dt);

        // Drain new sysinfo snapshots from the background poller (cheap).
        launcher.refresh_sysinfo();

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
        let had_input = input_events.iter().any(|e| {
            matches!(
                e.action,
                cartridge_core::input::InputAction::Press
                    | cartridge_core::input::InputAction::Repeat
            )
        });
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
                text_cache: &mut text_cache,
                texture_creator: &texture_creator,
            };
            launcher.render(&mut screen, &atmosphere);

            // FPS / frametime overlay
            if show_fps {
                let last_ms = frame_times.back().copied().unwrap_or(0.0) * 1000.0;
                let avg_ms = if frame_times.is_empty() {
                    0.0
                } else {
                    frame_times.iter().sum::<f32>() / frame_times.len() as f32 * 1000.0
                };
                let max_ms = frame_times.iter().cloned().fold(0.0_f32, f32::max) * 1000.0;
                let fps = if avg_ms > 0.0 { 1000.0 / avg_ms } else { 0.0 };
                let stats = format!(
                    "fps {:.1} | last {:.0}ms | avg {:.0}ms | max {:.0}ms | cache {}h/{}m {}",
                    fps, last_ms, avg_ms, max_ms,
                    screen.text_cache.hits, screen.text_cache.misses,
                    screen.text_cache.entry_count(),
                );
                let bg = sdl2::pixels::Color::RGBA(0, 0, 0, 200);
                screen.canvas.set_draw_color(bg);
                screen.canvas.fill_rect(sdl2::rect::Rect::new(2, 2, 716, 16)).ok();
                screen.draw_text(&stats, 6, 4, Some(sdl2::pixels::Color::RGB(0, 255, 100)), 11, false, None);
            }
        }

        canvas.present();

        // Frame rate cap. If input happened, skip the sleep so the next
        // frame renders immediately (cuts input-to-pixel latency).
        // Otherwise, choose ACTIVE_FPS or IDLE_FPS based on time since last input.
        if had_input {
            last_input = Instant::now();
        }
        let frame_time = Instant::now().duration_since(frame_start);
        let idle_secs = last_input.elapsed().as_secs_f32();
        let target_fps = if idle_secs > IDLE_AFTER_SECS { IDLE_FPS } else { ACTIVE_FPS };
        let target_time = std::time::Duration::from_secs_f64(1.0 / target_fps as f64);
        if !had_input && frame_time < target_time {
            std::thread::sleep(target_time - frame_time);
        }

        // Track frametimes for the FPS overlay
        if show_fps {
            if frame_times.len() >= 60 {
                frame_times.pop_front();
            }
            frame_times.push_back(frame_time.as_secs_f32());
            if last_stats_log.elapsed().as_secs() >= 5 {
                let avg_ms: f32 = if frame_times.is_empty() {
                    0.0
                } else {
                    frame_times.iter().sum::<f32>() / frame_times.len() as f32 * 1000.0
                };
                log::info!(
                    "perf: fps={:.1} avg_render={:.0}ms cache_hits={} misses={} entries={}",
                    if avg_ms > 0.0 { 1000.0 / avg_ms } else { 0.0 },
                    avg_ms,
                    text_cache.hits,
                    text_cache.misses,
                    text_cache.entry_count(),
                );
                last_stats_log = Instant::now();
            }
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
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let cwd = std::env::current_dir().unwrap_or_default();
    let variants = crate::ui_constants::name_variants(app_id);

    // First pass: check ALL bundled paths (next to binary + cwd) for ALL variants
    for name in &variants {
        if let Some(ref dir) = exe_dir {
            let bundled = dir.join("lua_cartridges").join(name);
            if bundled.exists() {
                return bundled;
            }
        }
        let dev_path = cwd.join("lua_cartridges").join(name);
        if dev_path.exists() {
            return dev_path;
        }
    }

    // Second pass: fall back to user-installed paths
    for name in &variants {
        let installed_path = home.join(".cartridges/apps").join(name);
        if installed_path.exists() {
            return installed_path;
        }
    }

    home.join(".cartridges/apps").join(app_id)
}
