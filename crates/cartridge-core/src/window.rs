//! Single window/canvas creation helper shared by every SDL entry point
//! (launcher, Lua runner, boot selector, demo runner).
//!
//! Honors these environment variables so the desktop simulator can be
//! driven without touching call sites:
//!
//! - `CARTRIDGE_HIDDEN=1`      hidden window (headless capture / benches)
//! - `CARTRIDGE_SOFTWARE=1`    software renderer (read_pixels is reliable)
//! - `CARTRIDGE_SCALE=<f>`     window is 720*f points, logical size stays 720
//! - `CARTRIDGE_FULLSCREEN=1`  desktop fullscreen, logical size 720 (letterboxed)
//!
//! Vsync is deliberately never enabled: `present_vsync()` is unreliable on
//! the RK3326 fbdev/DRM path and compounds with the sleep-based frame caps
//! every loop already uses.

use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::VideoSubsystem;

use crate::screen::{HEIGHT, WIDTH};

/// Options a call site can force regardless of the environment. Everything
/// left at its default is resolved from the env vars documented above.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowOptions {
    /// Force a hidden window (in addition to `CARTRIDGE_HIDDEN=1`).
    pub hidden: bool,
    /// Force the software renderer (in addition to `CARTRIDGE_SOFTWARE=1`).
    pub software: bool,
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).as_deref() == Ok("1")
}

/// `CARTRIDGE_SCALE` parsed as a positive float; `None` when unset/invalid.
pub fn env_scale() -> Option<f32> {
    std::env::var("CARTRIDGE_SCALE")
        .ok()
        .and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|s| s.is_finite() && *s > 0.05 && *s < 20.0)
}

/// Create the CartridgeOS window and canvas. The canvas always renders in
/// 720x720 logical coordinates; scaling/fullscreen are handled by SDL's
/// logical-size mapping so no drawing code needs to know about them.
pub fn create_canvas(
    video: &VideoSubsystem,
    title: &str,
    opts: WindowOptions,
) -> Result<Canvas<Window>, String> {
    let hidden = opts.hidden || env_flag("CARTRIDGE_HIDDEN");
    let software = opts.software || env_flag("CARTRIDGE_SOFTWARE");
    let fullscreen = env_flag("CARTRIDGE_FULLSCREEN");
    let scale = env_scale();

    let (win_w, win_h) = match scale {
        Some(s) => (
            ((WIDTH as f32) * s).round().max(1.0) as u32,
            ((HEIGHT as f32) * s).round().max(1.0) as u32,
        ),
        None => (WIDTH, HEIGHT),
    };

    let mut builder = video.window(title, win_w, win_h);
    builder.position_centered();
    if hidden {
        builder.hidden();
    }
    // Only opt into HiDPI when the simulator asked for scaling or
    // fullscreen; the default 1:1 window keeps output_size == 720x720 so
    // headless captures and snapshot baselines stay byte-identical.
    let scaled = scale.is_some() || fullscreen;
    if scaled {
        builder.allow_highdpi();
    }
    if fullscreen {
        builder.fullscreen_desktop();
    }
    let window = builder.build().map_err(|e| e.to_string())?;

    let mut canvas_builder = window.into_canvas();
    canvas_builder = if software {
        canvas_builder.software()
    } else {
        canvas_builder.accelerated()
    };
    let mut canvas = canvas_builder.build().map_err(|e| e.to_string())?;

    if scaled {
        // Fullscreen on a non-square display: letterbox instead of stretching.
        // The hint is read when the logical size is applied, so set it first.
        if fullscreen {
            let _ = sdl2::hint::set("SDL_RENDER_LOGICAL_SIZE_MODE", "letterbox");
        }
        canvas
            .set_logical_size(WIDTH, HEIGHT)
            .map_err(|e| e.to_string())?;
        log::info!(
            "window: {}x{} points (scale {:?}, fullscreen {}), logical {}x{}, output {:?}",
            win_w, win_h, scale, fullscreen, WIDTH, HEIGHT,
            canvas.output_size().ok()
        );
    }

    Ok(canvas)
}
