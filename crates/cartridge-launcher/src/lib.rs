pub mod app;
pub mod data;
pub mod screens;
pub mod ui_constants;

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

/// Stats collected during a launcher run -- used by perf benches and tests.
#[derive(Debug, Clone, Default)]
pub struct LauncherStats {
    pub frames: u64,
    pub elapsed_secs: f32,
    /// Min frame time in milliseconds.
    pub frame_ms_min: f32,
    pub frame_ms_max: f32,
    pub frame_ms_avg: f32,
    pub frame_ms_p95: f32,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_entries: usize,
}

impl LauncherStats {
    pub fn fps_avg(&self) -> f32 {
        if self.frame_ms_avg > 0.0 { 1000.0 / self.frame_ms_avg } else { 0.0 }
    }
}

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

    // Hidden window for headless capture (perf benches, snapshot tool).
    let hidden = std::env::var("CARTRIDGE_HIDDEN").as_deref() == Ok("1");
    let mut window_builder = video_subsystem.window("CartridgeOS", WIDTH, HEIGHT);
    window_builder.position_centered();
    if hidden {
        window_builder.hidden();
    }
    let window = window_builder.build().map_err(|e| e.to_string())?;

    // Note: present_vsync() is unreliable on RK3326's fbdev/DRM path and
    // would compound with the sleep-based frame cap below. Rely on the
    // sleep cap alone for predictable timing.
    //
    // Software rendering when CARTRIDGE_SOFTWARE=1 (for headless capture
    // -- read_pixels is reliable on software renderers).
    let software = std::env::var("CARTRIDGE_SOFTWARE").as_deref() == Ok("1");
    let mut canvas_builder = window.into_canvas();
    if software {
        canvas_builder = canvas_builder.software();
    } else {
        canvas_builder = canvas_builder.accelerated();
    }
    let mut canvas = canvas_builder.build().map_err(|e| e.to_string())?;

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
    atmosphere.precompose(&mut canvas, &texture_creator, &mut images, &theme);
    let mut last_frame = Instant::now();
    let mut last_input = Instant::now();

    // Optional FPS / frametime overlay enabled via CARTRIDGE_FPS=1
    let show_fps = std::env::var("CARTRIDGE_FPS").ok().as_deref() == Some("1");
    let mut frame_times: std::collections::VecDeque<f32> = std::collections::VecDeque::with_capacity(60);
    let mut last_stats_log = Instant::now();

    // Bench/test infrastructure
    let mut frame_count: u64 = 0;
    let mut script_idx = 0usize;
    let mut script_wait_frames: u32 = 0;
    let mut all_frame_ms: Vec<f32> = Vec::with_capacity(1024);
    let bench_start = Instant::now();
    let result;

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
        if launcher.handle_input(&input_events) {
            if let Some(app_id) = launcher.pending_launch() {
                let app_dir = resolve_app_dir(app_id, assets_dir);
                result = LauncherResult::LaunchApp(app_dir);
                return Ok((result, build_stats(frame_count, &all_frame_ms, &text_cache, bench_start)));
            }
            result = LauncherResult::Quit;
            return Ok((result, build_stats(frame_count, &all_frame_ms, &text_cache, bench_start)));
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

            if show_fps {
                draw_fps_overlay(&mut screen, &frame_times);
            }
        }

        // Capture frame BEFORE present so we get exactly what was drawn.
        if config.capture_frames.contains(&frame_count) {
            if let Some(ref dir) = config.capture_dir {
                let path = dir.join(format!("frame_{frame_count:04}.png"));
                if let Err(e) = capture_frame_to_png(&canvas, &path) {
                    log::warn!("Failed to capture frame: {e}");
                } else {
                    log::info!("Captured frame {frame_count} to {}", path.display());
                }
            }
        }

        canvas.present();

        // Frame rate cap.
        if had_input {
            last_input = Instant::now();
        }
        let frame_time = Instant::now().duration_since(frame_start);
        all_frame_ms.push(frame_time.as_secs_f32() * 1000.0);

        if !config.uncapped {
            let idle_secs = last_input.elapsed().as_secs_f32();
            let target_fps = if idle_secs > IDLE_AFTER_SECS { IDLE_FPS } else { ACTIVE_FPS };
            let target_time = std::time::Duration::from_secs_f64(1.0 / target_fps as f64);
            if !had_input && frame_time < target_time {
                std::thread::sleep(target_time - frame_time);
            }
        }

        // Track frametimes for the FPS overlay
        if show_fps {
            if frame_times.len() >= 60 {
                frame_times.pop_front();
            }
            frame_times.push_back(frame_time.as_secs_f32());
            if last_stats_log.elapsed().as_secs() >= 5 {
                let stats = build_stats(frame_count, &all_frame_ms, &text_cache, bench_start);
                log::info!(
                    "perf: fps={:.1} avg={:.1}ms p95={:.1}ms cache {}h/{}m ({})",
                    stats.fps_avg(), stats.frame_ms_avg, stats.frame_ms_p95,
                    stats.cache_hits, stats.cache_misses, stats.cache_entries,
                );
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

fn draw_fps_overlay(screen: &mut Screen, frame_times: &std::collections::VecDeque<f32>) {
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

fn build_stats(
    frames: u64,
    frame_ms: &[f32],
    text_cache: &TextCache,
    start: Instant,
) -> LauncherStats {
    let mut sorted: Vec<f32> = frame_ms.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let avg = if frame_ms.is_empty() {
        0.0
    } else {
        frame_ms.iter().sum::<f32>() / frame_ms.len() as f32
    };
    let p95 = if sorted.is_empty() {
        0.0
    } else {
        sorted[(sorted.len() as f32 * 0.95) as usize - sorted.len().min(1)]
    };
    LauncherStats {
        frames,
        elapsed_secs: start.elapsed().as_secs_f32(),
        frame_ms_min: sorted.first().copied().unwrap_or(0.0),
        frame_ms_max: sorted.last().copied().unwrap_or(0.0),
        frame_ms_avg: avg,
        frame_ms_p95: p95,
        cache_hits: text_cache.hits,
        cache_misses: text_cache.misses,
        cache_entries: text_cache.entry_count(),
    }
}

fn print_stats_summary(stats: &LauncherStats) {
    println!("\n=== Launcher Perf Stats ===");
    println!("  frames    : {}", stats.frames);
    println!("  elapsed   : {:.2}s", stats.elapsed_secs);
    println!("  fps avg   : {:.1}", stats.fps_avg());
    println!("  frame ms  : min={:.2} avg={:.2} p95={:.2} max={:.2}",
        stats.frame_ms_min, stats.frame_ms_avg, stats.frame_ms_p95, stats.frame_ms_max);
    let total = (stats.cache_hits + stats.cache_misses).max(1);
    let hit_rate = stats.cache_hits as f64 / total as f64 * 100.0;
    println!("  text cache: {} hits / {} misses ({:.1}% hit rate, {} entries)",
        stats.cache_hits, stats.cache_misses, hit_rate, stats.cache_entries);
    println!();
}

/// Capture the current canvas contents as a PNG file.
fn capture_frame_to_png(
    canvas: &sdl2::render::Canvas<sdl2::video::Window>,
    path: &Path,
) -> Result<(), String> {
    let pixel_format = sdl2::pixels::PixelFormatEnum::RGBA32;
    let pixels = canvas
        .read_pixels(None, pixel_format)
        .map_err(|e| format!("read_pixels failed: {e}"))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let img = image::RgbaImage::from_raw(WIDTH, HEIGHT, pixels)
        .ok_or_else(|| "buffer size mismatch".to_string())?;
    img.save(path).map_err(|e| format!("PNG save failed: {e}"))?;
    Ok(())
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
