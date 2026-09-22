pub mod api;
pub mod manifest;
pub mod runner;
mod http_fixture;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cartridge_core::font::FontCache;
use cartridge_core::image_cache::ImageCache;
use cartridge_core::input::{Button, InputManager};
use cartridge_core::perf::{self, FrameStats, FrameTimes};
use cartridge_core::screen::Screen;
use cartridge_core::text_cache::TextCache;
use cartridge_core::theme::Theme;

use manifest::CartridgeManifest;
use runner::LuaAppRunner;

/// Frame rate while the user is interacting (input within IDLE_AFTER_SECS).
const ACTIVE_FPS: u32 = 30;
/// Seconds without input before dropping to the app's idle rate
/// (`app.set_idle_fps(n)`, default 5). on_update keeps running at the
/// idle rate; on_render/present only run on dirty frames.
const IDLE_AFTER_SECS: f32 = 3.0;
/// Force a render at least this often even when nothing reported dirty
/// (clocks, timers, anything the app forgot to flag).
const FORCE_RENDER_EVERY: Duration = Duration::from_secs(1);

/// Configuration for headless / bench runs of a Lua cartridge.
#[derive(Debug, Clone, Default)]
pub struct LuaAppConfig {
    /// Create the SDL window hidden (also honoured via CARTRIDGE_HIDDEN=1).
    pub hidden: bool,
    /// Skip the frame-rate sleep so benches run as fast as possible.
    pub uncapped: bool,
    /// Stop after this many frames (None = run until quit).
    pub max_frames: Option<u64>,
    /// If set, frames listed in `capture_frames` are dumped here as
    /// `frame_NNNN.png`.
    pub capture_dir: Option<PathBuf>,
    /// Frames at which to capture (e.g. [10, 30, 60]).
    pub capture_frames: Vec<u64>,
    /// Print the perf summary when the run ends.
    pub print_stats: bool,
    /// Simulator-only HTTP replies; unmatched requests fail offline.
    pub http_fixture: Option<PathBuf>,
    /// Frame-numbered button presses for deterministic interaction checks.
    pub script: Vec<(u64, Button)>,
    /// Automated checks fail on Lua errors instead of capturing an error panel.
    pub fail_on_error: bool,
}

/// Run a Lua cartridge app from the given directory.
///
/// This function:
/// 1. Reads `cartridge.json` from `app_dir`
/// 2. Initializes SDL2, creates a 720x720 window
/// 3. Creates the Lua VM and registers all APIs
/// 4. Loads the entry Lua file
/// 5. Enters the frame loop calling Lua lifecycle functions
/// 6. Handles Escape/window close to quit
pub fn run_lua_app(app_dir: &Path, assets_dir: &Path) -> Result<(), String> {
    let http_fixture = if cartridge_core::sim::is_sim() {
        std::env::var_os("CARTRIDGE_HTTP_FIXTURE").map(PathBuf::from)
    } else { None };
    run_lua_app_with_config(app_dir, assets_dir, LuaAppConfig {
        http_fixture, ..Default::default()
    }).map(|_| ())
}

/// Run a Lua cartridge with bench/test config. Returns frame stats.
///
/// Frame loop: on_input for each event, then on_update every frame, then
/// on_render + present only when the frame is dirty. A frame is dirty on
/// any input event, when http.poll() delivered results, while the text
/// input overlay is visible, after a hot reload, on an explicit
/// `app.request_redraw()`, when on_update returns a truthy value, and at
/// least once per second. The loop runs at ACTIVE_FPS while the user is
/// interacting and drops to `app.set_idle_fps(n)` (default 5) after
/// IDLE_AFTER_SECS without input. Frames that saw input never sleep.
pub fn run_lua_app_with_config(
    app_dir: &Path,
    assets_dir: &Path,
    config: LuaAppConfig,
) -> Result<FrameStats, String> {
    let manifest = CartridgeManifest::load(app_dir)?;
    log::info!(
        "Running cartridge: {} v{} by {}",
        manifest.name,
        manifest.version,
        manifest.author,
    );

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let joystick_subsystem = sdl_context.joystick()?;
    let _joysticks = cartridge_core::input::open_all_joysticks(&joystick_subsystem);
    let game_controller_subsystem = sdl_context.game_controller()?;
    let _controllers = cartridge_core::input::open_all_controllers(&game_controller_subsystem);

    let window_title = format!("CartridgeOS - {}", manifest.name);
    // Shared helper: honors CARTRIDGE_HIDDEN/SOFTWARE/SCALE/FULLSCREEN.
    // No present_vsync(): unreliable on RK3326; sleep cap below provides timing.
    let mut canvas = cartridge_core::window::create_canvas(
        &video_subsystem,
        &window_title,
        cartridge_core::window::WindowOptions {
            hidden: config.hidden,
            ..Default::default()
        },
    )?;

    let texture_creator = canvas.texture_creator();
    let mut fonts = FontCache::new(assets_dir)?;
    let mut images = ImageCache::new(&texture_creator)?;
    let mut text_cache = TextCache::new(&texture_creator);

    // Inherit the launcher's selected theme. Cartridges share the user's
    // palette and font family for a consistent look across the OS.
    let theme = Theme::user_selected();
    fonts.set_family(theme.font_regular, theme.font_bold);
    fonts.set_display(theme.font_display);
    fonts.prewarm();
    let mut input_manager = InputManager::new();
    if !_controllers.is_empty() {
        input_manager.set_ignore_joystick(true);
    }
    let mut event_pump = sdl_context.event_pump()?;

    let mut app = LuaAppRunner::new_with_http_fixture(app_dir, &manifest.entry, &manifest.id, &theme, &manifest.permissions, config.http_fixture.as_deref())?;

    // Call on_init
    app.call_init();

    let mut last_frame = Instant::now();
    let mut last_input = Instant::now();

    // Dirty rendering: skip on_render + present when nothing has changed.
    let mut dirty = true;
    let mut last_render = Instant::now();
    let mut error_shown = false;

    // Optional FPS / frametime overlay enabled via CARTRIDGE_FPS=1
    let show_fps = std::env::var("CARTRIDGE_FPS").ok().as_deref() == Some("1");
    let mut frame_times = FrameTimes::new();
    let mut last_stats_log = Instant::now();

    // Bench/test infrastructure
    let mut frame_count: u64 = 0;
    let mut all_frame_ms = cartridge_core::perf::FrameSamples::new(config.max_frames.is_some());
    let mut render_frame_ms = cartridge_core::perf::FrameSamples::new(config.max_frames.is_some());
    let bench_start = Instant::now();

    // Hot-reload support: when CARTRIDGE_HOT_RELOAD=1, watch the cartridge
    // directory for .lua file changes and recreate the Lua VM on edits.
    // Useful for cartridge development; ignored on the device by default.
    let hot_reload = std::env::var("CARTRIDGE_HOT_RELOAD").as_deref() == Ok("1");
    let mut last_lua_mtime = if hot_reload {
        latest_lua_mtime(app_dir)
    } else {
        0
    };
    let mut last_reload_check = Instant::now();

    'running: loop {
        let frame_start = Instant::now();
        let mut capture_time = std::time::Duration::ZERO;
        let dt = frame_start.duration_since(last_frame).as_secs_f32();
        last_frame = frame_start;

        // Hot reload: every 1s, check for .lua file changes and recreate the VM.
        if hot_reload && last_reload_check.elapsed().as_secs_f32() >= 1.0 {
            last_reload_check = Instant::now();
            let cur = latest_lua_mtime(app_dir);
            if cur > last_lua_mtime && last_lua_mtime > 0 {
                log::info!("Hot reload: detected change, restarting cartridge VM");
                app.call_destroy();
                drop(app);
                match LuaAppRunner::new_with_http_fixture(app_dir, &manifest.entry, &manifest.id, &theme, &manifest.permissions, config.http_fixture.as_deref()) {
                    Ok(mut new_app) => {
                        new_app.call_init();
                        app = new_app;
                    }
                    Err(e) => {
                        log::error!("Hot reload failed: {e}");
                        // Caller will see a Lua error screen on next render via
                        // the runner's error path -- but we couldn't reach a
                        // runner. Re-create with the old code if possible.
                        app = LuaAppRunner::new_with_http_fixture(app_dir, &manifest.entry, &manifest.id, &theme, &manifest.permissions, config.http_fixture.as_deref())?;
                        app.call_init();
                    }
                }
                dirty = true;
            }
            last_lua_mtime = cur;
        }

        // Collect events
        let events: Vec<sdl2::event::Event> = event_pump.poll_iter().collect();

        // Check for quit via raw SDL events (bypasses input manager).
        // This catches Select/Start regardless of GameController mapping.
        let mut raw_select = false;
        let mut raw_start = false;
        for event in &events {
            match event {
                sdl2::event::Event::Quit { .. } => break 'running,
                sdl2::event::Event::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Escape),
                    ..
                } => break 'running,
                // Joystick API: button 12=Select, 13=Start on R36S Plus
                sdl2::event::Event::JoyButtonDown { button_idx: 12, .. } => {
                    raw_select = true;
                }
                sdl2::event::Event::JoyButtonDown { button_idx: 13, .. } => {
                    raw_start = true;
                }
                // GameController API: Back=Select, Start=Start
                sdl2::event::Event::ControllerButtonDown { button, .. } => {
                    match button {
                        sdl2::controller::Button::Back
                        | sdl2::controller::Button::Guide => {
                            raw_select = true;
                        }
                        sdl2::controller::Button::Start => {
                            raw_start = true;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        // Exit on Select alone or Start+Select combo
        if raw_select || (raw_start && raw_select) {
            break 'running;
        }
        // Screenshot (F12 / SIGUSR1): reads back the last presented frame.
        // Best-effort on accelerated renderers; exact with CARTRIDGE_SOFTWARE=1.
        if cartridge_core::screenshot::requested(&events) {
            let capture_start = Instant::now();
            if let Err(e) = cartridge_core::screenshot::save_now(&canvas) {
                log::warn!("Screenshot failed: {e}");
            }
            capture_time += capture_start.elapsed();
        }

        // Process input
        let mut input_events = input_manager.process_events(&events);
        for &(frame, button) in &config.script {
            if frame == frame_count {
                use cartridge_core::input::{InputEvent, InputAction};
                input_events.push(InputEvent { button, action: InputAction::Press });
                input_events.push(InputEvent { button, action: InputAction::Release });
            }
        }
        let had_input = !input_events.is_empty();
        if had_input {
            dirty = true;
            last_input = frame_start;
        }

        // If the text input widget is up, route events to it instead of
        // delivering them to the Lua app. Lua reads results via text_input.poll().
        if app.text_input_active() {
            for ev in &input_events {
                if ev.button == Button::Select {
                    continue;
                }
                app.text_input_handle(ev);
            }
            // The overlay animates (cursor) and reflects typing; keep it live.
            dirty = true;
        } else {
            // Deliver input to Lua (filter out Select so apps don't see it)
            let lua_events: Vec<_> = input_events
                .into_iter()
                .filter(|ie| ie.button != Button::Select)
                .collect();
            app.call_input(&lua_events);
        }

        // Update runs every frame (apps poll http/timers here). A truthy
        // return value or app.request_redraw() marks the frame dirty.
        if app.call_update(dt) {
            dirty = true;
        }
        if app.control().take_redraw() {
            dirty = true;
        }

        // Show a newly raised (or cleared) Lua error immediately.
        let has_error = app.error().is_some();
        if has_error != error_shown {
            error_shown = has_error;
            dirty = true;
        }

        let should_capture = config.capture_frames.contains(&frame_count);
        let force = last_render.elapsed() >= FORCE_RENDER_EVERY || should_capture;

        let render_this_frame = dirty || force;
        if render_this_frame {
            {
                let mut screen = Screen {
                    canvas: &mut canvas,
                    theme: &theme,
                    fonts: &mut fonts,
                    images: &mut images,
                    text_cache: &mut text_cache,
                    texture_creator: &texture_creator,
                };

                if let Some(error_msg) = app.error() {
                    let msg = error_msg.to_string();
                    LuaAppRunner::render_error_screen(&mut screen, &msg);
                } else {
                    app.call_render(&mut screen);

                    // If render produced an error, show it on the next frame
                    if let Some(error_msg) = app.error() {
                        let msg = error_msg.to_string();
                        LuaAppRunner::render_error_screen(&mut screen, &msg);
                    }
                }

                // Draw the text input widget over whatever the cartridge rendered.
                app.text_input_draw(&mut screen);

                if show_fps {
                    perf::draw_overlay(&mut screen, &frame_times);
                }
            }

            // Capture BEFORE present so we get exactly what was drawn.
            let capture_start = Instant::now();
            if should_capture {
                if let Some(ref dir) = config.capture_dir {
                    let path = dir.join(format!("frame_{frame_count:04}.png"));
                    if let Err(e) = perf::capture_frame_to_png(&canvas, &path) {
                        log::warn!("Failed to capture frame: {e}");
                    } else {
                        log::info!("Captured frame {frame_count} to {}", path.display());
                    }
                }
            }

            capture_time += capture_start.elapsed();
            canvas.present();
            dirty = false;
            last_render = Instant::now();
        }

        if config.fail_on_error {
            if let Some(error) = app.error() { return Err(format!("{}: {error}", manifest.name)); }
        }
        let frame_time = frame_start.elapsed().saturating_sub(capture_time);
        all_frame_ms.push(frame_time.as_secs_f32() * 1000.0);
        if render_this_frame { render_frame_ms.push(frame_time.as_secs_f32() * 1000.0); }

        // Adaptive frame rate cap. Never sleep past a frame that saw input.
        if !config.uncapped {
            let idle_secs = last_input.elapsed().as_secs_f32();
            let target_fps = if idle_secs > IDLE_AFTER_SECS {
                app.control().idle_fps()
            } else {
                ACTIVE_FPS
            };
            let target_time = Duration::from_secs_f64(1.0 / target_fps as f64);
            if !had_input {
                std::thread::sleep(target_time.saturating_sub(frame_start.elapsed()));
            }
        }

        // Track frametimes for the FPS overlay
        if show_fps {
            frame_times.push(frame_time.as_secs_f32());
            if last_stats_log.elapsed().as_secs() >= 5 {
                let stats = FrameStats::build(frame_count, &all_frame_ms, &render_frame_ms, &text_cache, bench_start);
                log::info!("{}", stats.log_line());
                last_stats_log = Instant::now();
            }
        }

        frame_count += 1;

        if let Some(max) = config.max_frames {
            if frame_count >= max {
                break 'running;
            }
        }
    }

    // Call on_destroy
    app.call_destroy();

    let stats = FrameStats::build(frame_count, &all_frame_ms, &render_frame_ms, &text_cache, bench_start);
    if config.print_stats {
        stats.print_summary("Lua App Perf Stats");
    }
    Ok(stats)
}

/// Walk the cartridge directory and return the latest mtime (in seconds
/// since UNIX epoch) of any .lua file. Used for hot reload detection.
/// Returns 0 if the directory can't be read.
fn latest_lua_mtime(app_dir: &Path) -> u64 {
    let mut latest = 0u64;
    walk_lua(app_dir, &mut |p| {
        if let Ok(meta) = std::fs::metadata(p) {
            if let Ok(modified) = meta.modified() {
                if let Ok(d) = modified.duration_since(std::time::UNIX_EPOCH) {
                    let secs = d.as_secs();
                    if secs > latest {
                        latest = secs;
                    }
                }
            }
        }
    });
    latest
}

fn walk_lua(dir: &Path, visit: &mut dyn FnMut(&Path)) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_lua(&path, visit);
        } else if path.extension().map(|e| e == "lua").unwrap_or(false) {
            visit(&path);
        }
    }
}
