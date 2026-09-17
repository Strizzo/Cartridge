pub mod app;
pub mod data;
pub mod neo;
pub mod screens;
pub mod ui_constants;
pub mod ui_sounds;

use cartridge_core::atmosphere::Atmosphere;
use cartridge_core::font::FontCache;
use cartridge_core::image_cache::ImageCache;
use cartridge_core::input::{Button, InputAction, InputEvent, InputManager};
use cartridge_core::screen::{Screen, HEIGHT, WIDTH};
use cartridge_core::text_cache::TextCache;
use cartridge_core::theme::Theme;
use std::path::{Path, PathBuf};
use std::time::Instant;

use app::LauncherApp;
use ui_sounds::UiSounds;

/// Active frame rate (when input has happened recently).
const ACTIVE_FPS: u32 = 30;
/// Idle frame rate (when no input for a while). Saves CPU on battery.
const IDLE_FPS: u32 = 5;
/// Idle frame rate when an animated theme is active. Higher than IDLE_FPS
/// so the sweep line looks smooth, lower than ACTIVE_FPS so we still save
/// some battery while idle.
const ANIM_IDLE_FPS: u32 = 18;
/// Time of last input before considered idle (in seconds).
const IDLE_AFTER_SECS: f32 = 3.0;

/// The result of running the launcher -- either the user quit, or they want to launch an app.
pub enum LauncherResult {
    /// User closed the launcher (Escape or window close).
    Quit,
    /// User wants to launch a Lua app at this path.
    LaunchApp(PathBuf),
}

/// Stats collected during a launcher run -- used by perf benches and tests.
/// Shared with the Lua loop via `cartridge_core::perf::FrameStats`.
pub type LauncherStats = cartridge_core::perf::FrameStats;

/// One scripted input frame: a list of button presses to inject and how
/// many frames to run before the next entry.
#[derive(Debug, Clone)]
pub struct ScriptStep {
    pub buttons: Vec<Button>,
    pub frames_after: u32,
}

/// Configuration for headless / scripted runs.
#[derive(Default)]
pub struct LauncherConfig {
    /// Stop after this many frames (None = run forever).
    pub max_frames: Option<u64>,
    /// Pre-scripted input. Each ScriptStep injects buttons then waits N frames.
    pub script: Vec<ScriptStep>,
    /// If set, dump frame N as PNG to this directory (file name: frame_N.png).
    /// Use snapshot_at to control which frames.
    pub capture_dir: Option<PathBuf>,
    /// Frames at which to capture (e.g. [10, 30, 60]).
    pub capture_frames: Vec<u64>,
    /// Skip the frame-rate sleep so benches run as fast as possible.
    pub uncapped: bool,
    /// Print perf stats every 5 seconds (or before exit).
    pub print_stats: bool,
}

/// Run the Cartridge launcher UI.
pub fn run_launcher(assets_dir: &Path) -> Result<LauncherResult, String> {
    let (result, _stats) = run_launcher_with_config(assets_dir, LauncherConfig::default())?;
    Ok(result)
}

/// Run the launcher with optional bench/test config. Returns (result, stats).
pub fn run_launcher_with_config(
    assets_dir: &Path,
    config: LauncherConfig,
) -> Result<(LauncherResult, LauncherStats), String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let joystick_subsystem = sdl_context.joystick()?;
    let _joysticks = cartridge_core::input::open_all_joysticks(&joystick_subsystem);
    let game_controller_subsystem = sdl_context.game_controller()?;
    let _controllers = cartridge_core::input::open_all_controllers(&game_controller_subsystem);

    // Window + canvas via the shared helper: honors CARTRIDGE_HIDDEN (headless
    // capture), CARTRIDGE_SOFTWARE (reliable read_pixels), CARTRIDGE_SCALE and
    // CARTRIDGE_FULLSCREEN (simulator). Never vsync (unreliable on RK3326;
    // the sleep-based frame cap below provides timing).
    let mut canvas = cartridge_core::window::create_canvas(
        &video_subsystem,
        "CartridgeOS",
        cartridge_core::window::WindowOptions::default(),
    )?;

    let texture_creator = canvas.texture_creator();
    let mut fonts = FontCache::new(assets_dir)?;
    let mut images = ImageCache::new(&texture_creator)?;
    let mut text_cache = TextCache::new(&texture_creator);

    let mut input_manager = InputManager::new();
    if !_controllers.is_empty() {
        input_manager.set_ignore_joystick(true);
    }
    let mut event_pump = sdl_context.event_pump()?;

    let mut launcher = LauncherApp::new(assets_dir);
    // Build the theme AFTER the launcher so we honor the user's saved
    // theme_id. Atmosphere is pre-composited from theme colors, so it
    // must be re-built whenever the user picks a different preset.
    let mut theme_id = launcher.theme_id().to_string();
    let mut theme = Theme::by_id(&theme_id);
    fonts.set_family(theme.font_regular, theme.font_bold);
    fonts.set_display(theme.font_display);
    fonts.prewarm();
    let mut atmosphere = Atmosphere::new();
    atmosphere.precompose(&mut canvas, &texture_creator, &mut images, &theme);

    let mut sounds = UiSounds::new();
    sounds.set_enabled(launcher.sounds_enabled());
    let mut last_frame = Instant::now();
    let mut last_input = Instant::now();

    // Optional FPS / frametime overlay enabled via CARTRIDGE_FPS=1
    let show_fps = std::env::var("CARTRIDGE_FPS").ok().as_deref() == Some("1");
    let mut frame_times = cartridge_core::perf::FrameTimes::new();
    let mut last_stats_log = Instant::now();

    // Bench/test infrastructure
    let mut frame_count: u64 = 0;
    let mut script_idx = 0usize;
    let mut script_wait_frames: u32 = 0;
    let mut all_frame_ms: Vec<f32> = Vec::with_capacity(1024);
    let bench_start = Instant::now();
    let result;

    // Dirty rendering: skip render+present when nothing has changed.
    // Always render the first few frames (warmup, asset loading).
    let mut dirty = true;
    let mut last_render = Instant::now();
    // Force a re-render at least every N seconds even when idle (sysinfo
    // history grows, clock ticks, etc.). 1 second is fine -- still saves
    // 4 of every 5 idle frames at IDLE_FPS=5.
    let force_render_every = std::time::Duration::from_secs(1);

    loop {
        let frame_start = Instant::now();
        let dt = frame_start.duration_since(last_frame).as_secs_f32();
        last_frame = frame_start;
        atmosphere.update(dt);

        // Drain new sysinfo snapshots from the background poller (cheap).
        if launcher.refresh_sysinfo() {
            dirty = true;
        }

        // Collect SDL events
        let events: Vec<sdl2::event::Event> = event_pump.poll_iter().collect();

        // Check for quit / escape
        for event in &events {
            match event {
                sdl2::event::Event::Quit { .. } => {
                    result = LauncherResult::Quit;
                    return Ok((result, build_stats(frame_count, &all_frame_ms, &text_cache, bench_start)));
                }
                sdl2::event::Event::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Escape),
                    ..
                } => {
                    result = LauncherResult::Quit;
                    return Ok((result, build_stats(frame_count, &all_frame_ms, &text_cache, bench_start)));
                }
                _ => {}
            }
        }

        // Screenshot hotkey (F12) or SIGUSR1: force a render this frame and
        // capture it just before present.
        let screenshot_requested = cartridge_core::screenshot::requested(&events);
        if screenshot_requested {
            dirty = true;
        }

        // Process input
        let mut input_events = input_manager.process_events(&events);

        // Inject scripted input if applicable
        if !config.script.is_empty() && script_idx < config.script.len() {
            if script_wait_frames == 0 {
                let step = &config.script[script_idx];
                for b in &step.buttons {
                    input_events.push(InputEvent { button: *b, action: InputAction::Press });
                    input_events.push(InputEvent { button: *b, action: InputAction::Release });
                }
                script_wait_frames = step.frames_after;
                script_idx += 1;
            } else {
                script_wait_frames -= 1;
            }
        }

        let had_input = input_events.iter().any(|e| {
            matches!(e.action, InputAction::Press | InputAction::Repeat)
        });
        if had_input {
            dirty = true;
        }

        // Sound feedback for navigation. Triggered on Press only (not
        // Repeat) so holding a direction doesn't machine-gun beeps.
        for ev in &input_events {
            if ev.action != InputAction::Press {
                continue;
            }
            match ev.button {
                Button::DpadUp | Button::DpadDown | Button::DpadLeft | Button::DpadRight => {
                    sounds.click();
                }
                Button::A => sounds.confirm(),
                Button::B => sounds.back(),
                _ => {}
            }
        }
        // While the theme has an animated sweep line and the user has
        // animations enabled, force redraws so the line actually moves.
        if atmosphere.has_animation() && launcher.animations_enabled() {
            dirty = true;
        }
        if launcher.handle_input(&input_events) {
            if let Some(app_id) = launcher.pending_launch() {
                sounds.launch();
                // Give the audio device ~150ms to actually emit the
                // launch chirp before we surrender SDL to the cartridge.
                std::thread::sleep(std::time::Duration::from_millis(120));
                let app_dir = resolve_app_dir(app_id, assets_dir);
                result = LauncherResult::LaunchApp(app_dir);
                return Ok((result, build_stats(frame_count, &all_frame_ms, &text_cache, bench_start)));
            }
            result = LauncherResult::Quit;
            return Ok((result, build_stats(frame_count, &all_frame_ms, &text_cache, bench_start)));
        }

        // Reflect setting changes (sounds toggle).
        sounds.set_enabled(launcher.sounds_enabled());

        // If the user picked a different theme, rebuild the palette,
        // swap the font family, and re-precompose the atmosphere texture
        // (background, scanlines, corner markers are baked from theme).
        if launcher.theme_id() != theme_id {
            theme_id = launcher.theme_id().to_string();
            theme = Theme::by_id(&theme_id);
            fonts.set_family(theme.font_regular, theme.font_bold);
            fonts.set_display(theme.font_display);
            text_cache.clear();
            atmosphere.precompose(&mut canvas, &texture_creator, &mut images, &theme);
            dirty = true;
        }

        // Force a render every N seconds even when idle (clock, sysinfo
        // history, etc). Also force when this frame is being captured.
        let should_capture = config.capture_frames.contains(&frame_count);
        let force = last_render.elapsed() >= force_render_every || should_capture;
        let render_this_frame = dirty || force;

        if render_this_frame {
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

                if show_fps {
                    cartridge_core::perf::draw_overlay(&mut screen, &frame_times);
                }
            }

            // Capture frame BEFORE present so we get exactly what was drawn.
            if should_capture {
                if let Some(ref dir) = config.capture_dir {
                    let path = dir.join(format!("frame_{frame_count:04}.png"));
                    if let Err(e) = capture_frame_to_png(&canvas, &path) {
                        log::warn!("Failed to capture frame: {e}");
                    } else {
                        log::info!("Captured frame {frame_count} to {}", path.display());
                    }
                }
            }
            if screenshot_requested {
                if let Err(e) = cartridge_core::screenshot::save_now(&canvas) {
                    log::warn!("Screenshot failed: {e}");
                }
            }

            canvas.present();
            dirty = false;
            last_render = Instant::now();
        }

        // Frame rate cap.
        if had_input {
            last_input = Instant::now();
        }
        let frame_time = Instant::now().duration_since(frame_start);
        all_frame_ms.push(frame_time.as_secs_f32() * 1000.0);

        if !config.uncapped {
            let idle_secs = last_input.elapsed().as_secs_f32();
            let animating = atmosphere.has_animation() && launcher.animations_enabled();
            // When animations are on, don't drop to IDLE_FPS or the sweep
            // line will visibly stutter. Cap at a moderate ANIM_IDLE_FPS so
            // we still save some battery vs. ACTIVE_FPS when nothing else
            // is happening.
            let target_fps = if idle_secs > IDLE_AFTER_SECS {
                if animating { ANIM_IDLE_FPS } else { IDLE_FPS }
            } else {
                ACTIVE_FPS
            };
            let target_time = std::time::Duration::from_secs_f64(1.0 / target_fps as f64);
            if !had_input && frame_time < target_time {
                std::thread::sleep(target_time - frame_time);
            }
        }

        // Track frametimes for the FPS overlay
        if show_fps {
            frame_times.push(frame_time.as_secs_f32());
            if last_stats_log.elapsed().as_secs() >= 5 {
                let stats = build_stats(frame_count, &all_frame_ms, &text_cache, bench_start);
                log::info!("{}", stats.log_line());
                last_stats_log = Instant::now();
            }
        }

        frame_count += 1;

        // Check exit conditions for benches
        if let Some(max) = config.max_frames {
            if frame_count >= max {
                result = LauncherResult::Quit;
                let stats = build_stats(frame_count, &all_frame_ms, &text_cache, bench_start);
                if config.print_stats {
                    print_stats_summary(&stats);
                }
                return Ok((result, stats));
            }
        }
    }
}

fn build_stats(
    frames: u64,
    frame_ms: &[f32],
    text_cache: &TextCache,
    start: Instant,
) -> LauncherStats {
    LauncherStats::build(frames, frame_ms, text_cache, start)
}

fn print_stats_summary(stats: &LauncherStats) {
    stats.print_summary("Launcher Perf Stats");
}

/// Capture the current canvas contents as a PNG file (shared implementation
/// in cartridge_core; handles scaled / HiDPI windows).
fn capture_frame_to_png(
    canvas: &sdl2::render::Canvas<sdl2::video::Window>,
    path: &Path,
) -> Result<(), String> {
    cartridge_core::screenshot::capture_frame_to_png(canvas, path)
}

/// Resolve the directory for an installed app given its id.
///
/// Checks both the full app_id and the short name (last segment of dotted ID):
/// 1. `lua_cartridges/{name}/` relative to the binary (bundled — preferred, always up to date)
/// 2. `~/.cartridges/apps/{name}/` (user-installed from store)
fn resolve_app_dir(app_id: &str, _assets_dir: &Path) -> PathBuf {
    let bundled_dir = cartridge_core::paths::bundled_cartridges_dir();
    let installed_dir = cartridge_core::paths::installed_apps_dir();
    let variants = crate::ui_constants::name_variants(app_id);

    // First pass: bundled cartridges (next to binary, else cwd) for ALL variants
    for name in &variants {
        let bundled = bundled_dir.join(name);
        if bundled.exists() {
            return bundled;
        }
    }

    // Second pass: fall back to user-installed paths
    for name in &variants {
        let installed_path = installed_dir.join(name);
        if installed_path.exists() {
            return installed_path;
        }
    }

    installed_dir.join(app_id)
}
