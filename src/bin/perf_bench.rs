//! Performance bench harness for the Cartridge launcher.
//!
//! Runs the launcher headlessly through scripted input scenarios and prints
//! frame-time statistics. Used to detect performance regressions without
//! needing to deploy to the device.
//!
//! Usage:
//!     cargo run --bin perf-bench --release [-- <scenario>]
//!
//! Scenarios:
//!     home       - Idle on home screen for 600 frames (~20s @ 30fps)
//!     navigate   - Walk dock left/right/up/down, open a few screens
//!     store      - Open store, scroll through apps
//!
//! Outputs frame timing percentiles, FPS, and text cache hit rate.
//! Exits with non-zero status if frame_ms_p95 exceeds the threshold.

use std::path::PathBuf;
use std::time::Instant;

use cartridge_core::input::Button;
use cartridge_launcher::{run_launcher_with_config, LauncherConfig, LauncherStats, ScriptStep};

fn assets_dir() -> PathBuf {
    // Try ./assets first, then next to binary.
    let cwd = std::env::current_dir().unwrap_or_default();
    let candidates = [
        cwd.join("assets"),
        cwd.join("../assets"),
    ];
    for c in &candidates {
        if c.join("fonts").exists() {
            return c.clone();
        }
    }
    panic!("Could not find assets directory");
}

fn main() -> Result<(), String> {
    env_logger::init();

    // Hidden window for headless bench. Real video driver so SDL accel
    // works; window is just invisible. CARTRIDGE_BENCH_VISIBLE=1 to show it.
    if std::env::var("CARTRIDGE_BENCH_VISIBLE").as_deref() != Ok("1") {
        unsafe { std::env::set_var("CARTRIDGE_HIDDEN", "1") };
    }

    let scenario = std::env::args().nth(1).unwrap_or_else(|| "home".to_string());
    let assets = assets_dir();

    println!("Running scenario: {scenario}");
    let start = Instant::now();

    let stats = match scenario.as_str() {
        "home" => run_idle_home(&assets)?,
        "navigate" => run_navigate(&assets)?,
        "store" => run_store(&assets)?,
        s => return Err(format!("unknown scenario: {s}")),
    };

    let elapsed = start.elapsed().as_secs_f32();
    print_summary(&scenario, &stats, elapsed);

    // Threshold check (release builds only — debug is much slower).
    let threshold_ms = std::env::var("CARTRIDGE_BENCH_P95_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(if cfg!(debug_assertions) { 200.0 } else { 30.0 });

    if stats.frame_ms_p95 > threshold_ms {
        eprintln!(
            "FAIL: p95 frame time {:.2}ms exceeds threshold {threshold_ms:.0}ms",
            stats.frame_ms_p95
        );
        std::process::exit(1);
    }

    Ok(())
}

fn run_idle_home(assets: &PathBuf) -> Result<LauncherStats, String> {
    let config = LauncherConfig {
        max_frames: Some(600),
        uncapped: true,
        print_stats: true,
        ..Default::default()
    };
    let (_, stats) = run_launcher_with_config(assets, config)?;
    Ok(stats)
}

fn run_navigate(assets: &PathBuf) -> Result<LauncherStats, String> {
    let config = LauncherConfig {
        max_frames: Some(800),
        uncapped: true,
        print_stats: true,
        script: vec![
            // Wait 30 frames for warmup
            ScriptStep { buttons: vec![], frames_after: 30 },
            // Walk through dock
            ScriptStep { buttons: vec![Button::DpadRight], frames_after: 5 },
            ScriptStep { buttons: vec![Button::DpadRight], frames_after: 5 },
            ScriptStep { buttons: vec![Button::DpadRight], frames_after: 5 },
            ScriptStep { buttons: vec![Button::DpadLeft], frames_after: 5 },
            ScriptStep { buttons: vec![Button::DpadLeft], frames_after: 5 },
            // Switch zone
            ScriptStep { buttons: vec![Button::DpadDown], frames_after: 10 },
            ScriptStep { buttons: vec![Button::DpadUp], frames_after: 10 },
            // Idle
            ScriptStep { buttons: vec![], frames_after: 100 },
        ],
        ..Default::default()
    };
    let (_, stats) = run_launcher_with_config(assets, config)?;
    Ok(stats)
}

fn run_store(assets: &PathBuf) -> Result<LauncherStats, String> {
    let config = LauncherConfig {
        max_frames: Some(600),
        uncapped: true,
        print_stats: true,
        script: vec![
            ScriptStep { buttons: vec![], frames_after: 30 },
            // Open store with Y
            ScriptStep { buttons: vec![Button::Y], frames_after: 30 },
            // Scroll through apps
            ScriptStep { buttons: vec![Button::DpadDown], frames_after: 5 },
            ScriptStep { buttons: vec![Button::DpadDown], frames_after: 5 },
            ScriptStep { buttons: vec![Button::DpadDown], frames_after: 5 },
            ScriptStep { buttons: vec![Button::DpadDown], frames_after: 5 },
            ScriptStep { buttons: vec![Button::DpadDown], frames_after: 5 },
            // Back
            ScriptStep { buttons: vec![Button::B], frames_after: 30 },
        ],
        ..Default::default()
    };
    let (_, stats) = run_launcher_with_config(assets, config)?;
    Ok(stats)
}

fn print_summary(scenario: &str, stats: &LauncherStats, wall_secs: f32) {
    println!("\n=== Bench: {scenario} ===");
    println!("  wall time  : {wall_secs:.2}s ({} frames)", stats.frames);
    println!("  uncapped   : {} fps", stats.fps_avg().round());
    println!("  frame ms   : min={:.2}  avg={:.2}  p95={:.2}  max={:.2}",
        stats.frame_ms_min, stats.frame_ms_avg, stats.frame_ms_p95, stats.frame_ms_max);
    let total = (stats.cache_hits + stats.cache_misses).max(1);
    let hit_rate = stats.cache_hits as f64 / total as f64 * 100.0;
    println!("  text cache : {} hits / {} misses ({hit_rate:.1}% hit, {} entries)",
        stats.cache_hits, stats.cache_misses, stats.cache_entries);
    println!();
}
