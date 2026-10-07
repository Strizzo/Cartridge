//! Frame-timing statistics, the `CARTRIDGE_FPS=1` on-screen overlay and
//! headless frame capture. Shared by the launcher loop and the Lua
//! cartridge loop so both report identical numbers.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Instant;

use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::screen::{HEIGHT, Screen, WIDTH};
use crate::text_cache::TextCache;

/// Stats collected during a run -- used by perf benches and tests.
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    pub frames: u64,
    pub rendered_frames: u64,
    pub render_ms_avg: f32,
    pub render_ms_p95: f32,
    pub render_ms_max: f32,
    pub percentile_samples: usize,
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
        if self.elapsed_secs > 0.0 {
            self.rendered_frames as f32 / self.elapsed_secs
        } else {
            0.0
        }
    }

    /// Summarize a run from its per-frame durations (milliseconds).
    pub fn build(
        frames: u64,
        frame_ms: &FrameSamples,
        render_ms: &FrameSamples,
        text_cache: &TextCache,
        start: Instant,
    ) -> Self {
        FrameStats {
            frames,
            rendered_frames: render_ms.count,
            render_ms_avg: render_ms.avg(),
            render_ms_p95: render_ms.p95(),
            render_ms_max: render_ms.max,
            percentile_samples: frame_ms.samples.len(),
            elapsed_secs: start.elapsed().as_secs_f32(),
            frame_ms_min: if frame_ms.count == 0 {
                0.0
            } else {
                frame_ms.min
            },
            frame_ms_max: frame_ms.max,
            frame_ms_avg: frame_ms.avg(),
            frame_ms_p95: frame_ms.p95(),
            cache_hits: text_cache.hits,
            cache_misses: text_cache.misses,
            cache_entries: text_cache.entry_count(),
        }
    }

    /// One-line summary for periodic `log::info!` output.
    pub fn log_line(&self) -> String {
        format!(
            "perf: presents={:.1}/s rendered={} render_avg={:.1}ms render_p95={:.1}ms cache {}h/{}m ({})",
            self.fps_avg(),
            self.rendered_frames,
            self.render_ms_avg,
            self.render_ms_p95,
            self.cache_hits,
            self.cache_misses,
            self.cache_entries,
        )
    }

    /// Multi-line summary printed by benches at exit.
    pub fn print_summary(&self, title: &str) {
        println!("\n=== {title} ===");
        println!("  frames    : {}", self.frames);
        println!("  elapsed   : {:.2}s", self.elapsed_secs);
        println!(
            "  presents  : {} ({:.1}/s)",
            self.rendered_frames,
            self.fps_avg()
        );
        println!(
            "  rendered  : avg={:.2} p95={:.2} max={:.2}ms",
            self.render_ms_avg, self.render_ms_p95, self.render_ms_max
        );
        println!(
            "  p95 scope : last {} work samples (full run for bounded benches)",
            self.percentile_samples
        );
        println!(
            "  frame ms  : min={:.2} avg={:.2} p95={:.2} max={:.2}",
            self.frame_ms_min, self.frame_ms_avg, self.frame_ms_p95, self.frame_ms_max
        );
        let total = (self.cache_hits + self.cache_misses).max(1);
        let hit_rate = self.cache_hits as f64 / total as f64 * 100.0;
        println!(
            "  text cache: {} hits / {} misses ({:.1}% hit rate, {} entries)",
            self.cache_hits, self.cache_misses, hit_rate, self.cache_entries
        );
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
        Self {
            window: VecDeque::with_capacity(WINDOW),
        }
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
}

/// Draw the `CARTRIDGE_FPS=1` overlay strip along the top edge.
pub fn draw_overlay(screen: &mut Screen, times: &FrameTimes) {
    let stats = format!(
        "CPU work | last {:.1}ms | avg {:.1}ms | max {:.1}ms | cache {}h/{}m {}",
        times.last_ms(),
        times.avg_ms(),
        times.max_ms(),
        screen.text_cache.hits,
        screen.text_cache.misses,
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
    img.save(path)
        .map_err(|e| format!("PNG save failed: {e}"))?;
    Ok(())
}

/// Constant-memory recording in interactive sessions. Explicit bounded runs
/// retain all samples so benchmark percentiles cover the complete scenario.
pub struct FrameSamples {
    samples: Vec<f32>,
    cursor: usize,
    full_run: bool,
    pub count: u64,
    total: f64,
    min: f32,
    max: f32,
}

impl FrameSamples {
    const CAPACITY: usize = 1024;

    pub fn new(full_run: bool) -> Self {
        Self {
            samples: Vec::with_capacity(Self::CAPACITY),
            cursor: 0,
            full_run,
            count: 0,
            total: 0.0,
            min: f32::INFINITY,
            max: 0.0,
        }
    }

    pub fn push(&mut self, ms: f32) {
        if !ms.is_finite() || ms < 0.0 {
            return;
        }
        self.count += 1;
        self.total += ms as f64;
        self.min = self.min.min(ms);
        self.max = self.max.max(ms);
        if self.full_run || self.samples.len() < Self::CAPACITY {
            self.samples.push(ms);
        } else {
            self.samples[self.cursor] = ms;
            self.cursor = (self.cursor + 1) % Self::CAPACITY;
        }
    }

    pub fn avg(&self) -> f32 {
        if self.count == 0 {
            0.0
        } else {
            (self.total / self.count as f64) as f32
        }
    }

    pub fn p95(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_by(f32::total_cmp);
        sorted[(sorted.len() * 95).div_ceil(100) - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_idle_session_stays_bounded_without_losing_totals() {
        let mut samples = FrameSamples::new(false);
        samples.push(50.0);
        for _ in 0..100_000 {
            samples.push(1.0);
        }
        assert_eq!(samples.samples.len(), 1024);
        assert_eq!(samples.samples.capacity(), 1024);
        assert_eq!(samples.count, 100_001);
        assert_eq!(samples.max, 50.0);
        assert!((samples.avg() - 100_050.0 / 100_001.0).abs() < 0.00001);
        assert_eq!(samples.p95(), 1.0); // explicitly the recent window
    }

    #[test]
    fn benchmark_keeps_complete_distribution_and_nearest_rank_percentile() {
        let mut samples = FrameSamples::new(true);
        for n in 1..=2000 {
            samples.push(n as f32);
        }
        assert_eq!(samples.samples.len(), 2000);
        assert_eq!(samples.p95(), 1900.0);
        assert_eq!(samples.avg(), 1000.5);
        let mut short = FrameSamples::new(false);
        assert_eq!(short.p95(), 0.0);
        short.push(1.0);
        short.push(30.0);
        short.push(f32::NAN);
        assert_eq!(short.p95(), 30.0);
        assert_eq!(short.count, 2);
    }
}
