//! Frame capture shared by every render loop.
//!
//! - `capture_frame_to_png(canvas, path)` reads back the current canvas
//!   (before `present()`, which is the only reliable moment on accelerated
//!   renderers) and writes a 720x720 PNG. On HiDPI/scaled windows the
//!   read-back is resampled to 720x720 so screenshots match the device.
//! - `requested(events)` is true when F12 was pressed this frame or a
//!   SIGUSR1 arrived since the last call (unix only). The signal handler is
//!   installed lazily on first use.
//! - `save_now(canvas)` writes `screenshots/<UTC timestamp>.png` under the
//!   working directory (see `paths::screenshots_dir`).
//!
//! read_pixels on the accelerated renderer is best-effort: capturing after
//! `present()` (as the Lua loop does, to keep its edit minimal) may return a
//! stale or blank frame on some drivers. Use `CARTRIDGE_SOFTWARE=1` when a
//! screenshot must be exact.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::screen::{HEIGHT, WIDTH};

static SIGNAL_REQUESTED: AtomicBool = AtomicBool::new(false);
static INSTALL: Once = Once::new();

#[cfg(unix)]
extern "C" fn on_sigusr1(_sig: libc::c_int) {
    SIGNAL_REQUESTED.store(true, Ordering::SeqCst);
}

/// Install the SIGUSR1 handler (idempotent, no-op off unix).
pub fn install_signal_handler() {
    INSTALL.call_once(|| {
        #[cfg(unix)]
        unsafe {
            libc::signal(libc::SIGUSR1, on_sigusr1 as usize as libc::sighandler_t);
        }
    });
}

/// True when a screenshot was requested this frame: F12 in `events`, or a
/// SIGUSR1 delivered since the previous call.
pub fn requested(events: &[Event]) -> bool {
    install_signal_handler();
    let f12 = events.iter().any(|e| {
        matches!(
            e,
            Event::KeyDown { keycode: Some(Keycode::F12), repeat: false, .. }
        )
    });
    f12 | SIGNAL_REQUESTED.swap(false, Ordering::SeqCst)
}

/// Capture the current canvas contents as a 720x720 PNG file.
pub fn capture_frame_to_png(canvas: &Canvas<Window>, path: &Path) -> Result<(), String> {
    let pixel_format = sdl2::pixels::PixelFormatEnum::RGBA32;
    let (out_w, out_h) = canvas
        .output_size()
        .map_err(|e| format!("output_size failed: {e}"))?;
    let pixels = canvas
        .read_pixels(None, pixel_format)
        .map_err(|e| format!("read_pixels failed: {e}"))?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let img = image::RgbaImage::from_raw(out_w, out_h, pixels)
        .ok_or_else(|| "buffer size mismatch".to_string())?;
    let img = if (out_w, out_h) != (WIDTH, HEIGHT) {
        // Scaled / HiDPI / letterboxed window: bring it back to panel size.
        // With a logical size set, SDL reads the whole output including any
        // letterbox bars, so crop to the centered square first.
        let side = out_w.min(out_h);
        let x0 = (out_w - side) / 2;
        let y0 = (out_h - side) / 2;
        let cropped = image::imageops::crop_imm(&img, x0, y0, side, side).to_image();
        image::imageops::resize(&cropped, WIDTH, HEIGHT, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    img.save(path).map_err(|e| format!("PNG save failed: {e}"))?;
    Ok(())
}

/// Write `screenshots/<timestamp>.png` and return its path.
pub fn save_now(canvas: &Canvas<Window>) -> Result<PathBuf, String> {
    let dir = crate::paths::screenshots_dir();
    let path = dir.join(format!("{}.png", utc_timestamp()));
    capture_frame_to_png(canvas, &path)?;
    log::info!("screenshot saved to {}", path.display());
    eprintln!("screenshot: {}", path.display());
    Ok(path)
}

/// `YYYYMMDD-HHMMSS-mmm` in UTC, without pulling in chrono.
fn utc_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}-{millis:03}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 -> (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
