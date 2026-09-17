//! Frame-timing statistics, the `CARTRIDGE_FPS=1` on-screen overlay and
//! headless frame capture. Shared by the launcher loop and the Lua
//! cartridge loop so both report identical numbers.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Instant;

use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::screen::{Screen, HEIGHT, WIDTH};
use crate::text_cache::TextCache;

/// Stats collected during a run -- used by perf benches and tests.
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
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

impl FrameStats {
    pub fn fps_avg(&self) -> f32 {
        if self.frame_ms_avg > 0.0 { 1000.0 / self.frame_ms_avg } else { 0.0 }
    }

    /// Summarize a run from its per-frame durations (milliseconds).
    pub fn build(frames: u64, frame_ms: &[f32], text_cache: &TextCache, start: Instant) -> Self {
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
            let idx = ((sorted.len() as f32 * 0.95) as usize)
                .saturating_sub(1)
                .min(sorted.len() - 1);
            sorted[idx]
        };
        FrameStats {
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

    /// One-line summary for periodic `log::info!` output.
    pub fn log_line(&self) -> String {
        format!(
            "perf: fps={:.1} avg={:.1}ms p95={:.1}ms cache {}h/{}m ({})",
            self.fps_avg(), self.frame_ms_avg, self.frame_ms_p95,
            self.cache_hits, self.cache_misses, self.cache_entries,
        )
    }

    /// Multi-line summary printed by benches at exit.
    pub fn print_summary(&self, title: &str) {
        println!("\n=== {title} ===");
        println!("  frames    : {}", self.frames);
        println!("  elapsed   : {:.2}s", self.elapsed_secs);
        println!("  fps avg   : {:.1}", self.fps_avg());
        println!("  frame ms  : min={:.2} avg={:.2} p95={:.2} max={:.2}",
            self.frame_ms_min, self.frame_ms_avg, self.frame_ms_p95, self.frame_ms_max);
        let total = (self.cache_hits + self.cache_misses).max(1);
        let hit_rate = self.cache_hits as f64 / total as f64 * 100.0;
        println!("  text cache: {} hits / {} misses ({:.1}% hit rate, {} entries)",
            self.cache_hits, self.cache_misses, hit_rate, self.cache_entries);
        println!();
    }
}

/// Rolling window of the most recent frame durations (seconds), feeding
/// the on-screen overlay.
pub struct FrameTimes {
    window: VecDeque<f32>,
}

const WINDOW: usize = 60;

impl Default for FrameTimes {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameTimes {
    pub fn new() -> Self {
        Self { window: VecDeque::with_capacity(WINDOW) }
    }

    pub fn push(&mut self, secs: f32) {
        if self.window.len() >= WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(secs);
    }

    pub fn last_ms(&self) -> f32 {
        self.window.back().copied().unwrap_or(0.0) * 1000.0
    }

    pub fn avg_ms(&self) -> f32 {
        if self.window.is_empty() {
            0.0
        } else {
            self.window.iter().sum::<f32>() / self.window.len() as f32 * 1000.0
        }
    }

    pub fn max_ms(&self) -> f32 {
        self.window.iter().cloned().fold(0.0_f32, f32::max) * 1000.0
    }

    pub fn fps(&self) -> f32 {
        let avg = self.avg_ms();
        if avg > 0.0 { 1000.0 / avg } else { 0.0 }
    }
}

/// Draw the `CARTRIDGE_FPS=1` overlay strip along the top edge.
pub fn draw_overlay(screen: &mut Screen, times: &FrameTimes) {
    let stats = format!(
        "fps {:.1} | last {:.0}ms | avg {:.0}ms | max {:.0}ms | cache {}h/{}m {}",
        times.fps(), times.last_ms(), times.avg_ms(), times.max_ms(),
        screen.text_cache.hits, screen.text_cache.misses,
        screen.text_cache.entry_count(),
    );
    let bg = Color::RGBA(0, 0, 0, 200);
    screen.canvas.set_draw_color(bg);
    screen.canvas.fill_rect(Rect::new(2, 2, 716, 16)).ok();
    screen.draw_text(&stats, 6, 4, Some(Color::RGB(0, 255, 100)), 11, false, None);
}

/// Capture the current canvas contents as a PNG file (headless tools).
pub fn capture_frame_to_png(
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
