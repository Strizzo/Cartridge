use sdl2::pixels::Color;
use sdl2::rect::{Point, Rect};
use sdl2::render::{BlendMode, Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};

use crate::image_cache::ImageCache;
use crate::screen::{Screen, HEIGHT, WIDTH};
use crate::theme::Theme;

/// Atmosphere with pre-composited background and overlay textures.
///
/// Previously did 3 fullscreen alpha-blended image blits per frame
/// (grid_bg, scanlines, vignette). Now bakes them into 2 cached
/// textures at startup -- a single blit each.
pub struct Atmosphere {
    background: Option<Texture<'static>>,
    overlay: Option<Texture<'static>>,
    sweep_y: f32,
    sweep_color: Color,
    sweep_enabled: bool,
}

/// Vertical speed of the animated sweep line in pixels per second.
const SWEEP_PIXELS_PER_SEC: f32 = 280.0;

impl Default for Atmosphere {
    fn default() -> Self {
        Self::new()
    }
}

impl Atmosphere {
    pub fn new() -> Self {
        Self {
            background: None,
            overlay: None,
            sweep_y: 0.0,
            sweep_color: Color::RGBA(255, 255, 255, 0),
            sweep_enabled: false,
        }
    }

    /// Initialize the cached background and overlay textures by rendering
    /// the static composition into them once. Call from the main loop
    /// before the render loop starts.
    pub fn precompose(
        &mut self,
        canvas: &mut Canvas<Window>,
        texture_creator: &TextureCreator<WindowContext>,
        images: &mut ImageCache,
        theme: &Theme,
    ) {
        self.background = build_background(canvas, texture_creator, images, theme);
        self.overlay = build_overlay(canvas, texture_creator, images, theme.scanline_strength);
        self.sweep_color = theme.sweep_line;
        self.sweep_enabled = theme.animated_sweep;
    }

    /// Advance the sweep-line position. Cheap; one float add and a wrap.
    /// Call once per frame regardless of animations_enabled -- the draw
    /// step is what's gated by the toggle.
    pub fn update(&mut self, dt: f32) {
        if self.sweep_enabled {
            // ~2.5 seconds for one full top-to-bottom pass.
            self.sweep_y += SWEEP_PIXELS_PER_SEC * dt;
            if self.sweep_y >= HEIGHT as f32 {
                self.sweep_y -= HEIGHT as f32;
            }
        }
    }

    /// Whether this theme contributes per-frame animated rendering.
    /// Used by the launcher's dirty-render heuristic to keep redrawing
    /// while the sweep line is on.
    pub fn has_animation(&self) -> bool {
        self.sweep_enabled
    }

    /// Draw the slow scan-style sweep line across the screen.
    /// Cost: a single horizontal `draw_line` (≈ negligible).
    pub fn draw_animated(&self, screen: &mut Screen, animations_enabled: bool) {
        if !animations_enabled || !self.sweep_enabled {
            return;
        }
        let y = self.sweep_y as i32;
        // Bright core line.
        screen.canvas.set_blend_mode(BlendMode::Blend);
        let core = self.sweep_color;
        screen.canvas.set_draw_color(Color::RGBA(core.r, core.g, core.b, 200));
        screen.canvas
            .draw_line(Point::new(0, y), Point::new(WIDTH as i32 - 1, y))
            .ok();
        // Soft trail above and below for a phosphor-glow effect.
        let trail = Color::RGBA(core.r, core.g, core.b, 70);
        screen.canvas.set_draw_color(trail);
        screen.canvas
            .draw_line(Point::new(0, y - 1), Point::new(WIDTH as i32 - 1, y - 1))
            .ok();
        screen.canvas
            .draw_line(Point::new(0, y + 1), Point::new(WIDTH as i32 - 1, y + 1))
            .ok();
    }

    /// Draw the atmospheric background: a single cached blit.
    /// Falls back to immediate-mode rendering if pre-composition failed.
    pub fn draw_background(&self, screen: &mut Screen) {
        if let Some(tex) = &self.background {
            screen.canvas.copy(tex, None, None).ok();
        } else {
            draw_background_immediate(screen);
        }
    }

    /// Draw overlays on top of content: a single cached blit.
    pub fn draw_overlays(&self, screen: &mut Screen) {
        if let Some(tex) = &self.overlay {
            screen.canvas.copy(tex, None, None).ok();
        } else {
            screen.draw_image("assets/overlays/scanlines.png", 0, 0, None, None);
            screen.draw_image("assets/overlays/vignette.png", 0, 0, None, None);
        }
    }
}

fn build_background(
    canvas: &mut Canvas<Window>,
    texture_creator: &TextureCreator<WindowContext>,
    images: &mut ImageCache,
    theme: &Theme,
) -> Option<Texture<'static>> {
    let mut target = texture_creator
        .create_texture_target(None, WIDTH, HEIGHT)
        .ok()?;
    target.set_blend_mode(BlendMode::Blend);

    // Pre-load grid_bg before switching the canvas target.
    // ImageCache borrow ends here; we copy via canvas inside the closure.
    let _grid_loaded = images.get("assets/overlays/grid_bg.png").is_some();
    let bg_color = theme.bg;
    let marker_color = theme.corner_marker;

    // Re-fetch grid_bg inside the closure scope (it's a fresh &mut images borrow).
    // We need a stable borrow that survives the with_texture_canvas closure.
    let grid_tex_ptr: *const Texture<'static> =
        images.get("assets/overlays/grid_bg.png").map(|t| t as *const _).unwrap_or(std::ptr::null());

    canvas
        .with_texture_canvas(&mut target, |c| {
            c.set_draw_color(bg_color);
            c.clear();

            // SAFETY: grid_tex_ptr was obtained from ImageCache which outlives this scope.
            if !grid_tex_ptr.is_null() {
                let grid_tex = unsafe { &*grid_tex_ptr };
                c.copy(
                    grid_tex,
                    Some(Rect::new(0, 0, WIDTH, HEIGHT)),
                    Some(Rect::new(0, 0, WIDTH, HEIGHT)),
                )
                .ok();
            }

            draw_corner_markers_canvas(c, marker_color, 20, 4);
        })
        .ok()?;

    // Transmute to 'static. Same lifetime trick as ImageCache: the texture's
    // owning TextureCreator outlives Atmosphere in the main loop.
    Some(unsafe { std::mem::transmute(target) })
}

fn build_overlay(
    canvas: &mut Canvas<Window>,
    texture_creator: &TextureCreator<WindowContext>,
    images: &mut ImageCache,
    scanline_strength: u8,
) -> Option<Texture<'static>> {
    let mut target = texture_creator
        .create_texture_target(None, WIDTH, HEIGHT)
        .ok()?;
    target.set_blend_mode(BlendMode::Blend);

    // Ensure both overlay images are loaded into the cache.
    let _ = images.get("assets/overlays/scanlines.png");
    let _ = images.get("assets/overlays/vignette.png");

    let scan_ptr: *mut Texture<'static> =
        images.get_mut("assets/overlays/scanlines.png").map(|t| t as *mut _).unwrap_or(std::ptr::null_mut());
    let vig_ptr: *const Texture<'static> =
        images.get("assets/overlays/vignette.png").map(|t| t as *const _).unwrap_or(std::ptr::null());

    canvas
        .with_texture_canvas(&mut target, |c| {
            // Clear with transparent so the underlying content shows through.
            c.set_draw_color(Color::RGBA(0, 0, 0, 0));
            c.clear();

            if !scan_ptr.is_null() {
                // SAFETY: ImageCache outlives this scope; we modulate alpha
                // briefly while baking the overlay, then leave the texture's
                // alpha unchanged in the cache (set back to 255 below).
                let t = unsafe { &mut *scan_ptr };
                t.set_alpha_mod(scanline_strength);
                c.copy(t, None, None).ok();
                t.set_alpha_mod(255);
            }
            if !vig_ptr.is_null() {
                let t = unsafe { &*vig_ptr };
                c.copy(t, None, None).ok();
            }
        })
        .ok()?;

    Some(unsafe { std::mem::transmute(target) })
}

/// Fallback path when pre-composition failed.
fn draw_background_immediate(screen: &mut Screen) {
    screen.clear(Some(screen.theme.bg));
    screen.draw_image(
        "assets/overlays/grid_bg.png",
        0,
        0,
        Some((WIDTH, HEIGHT)),
        Some(Rect::new(0, 0, WIDTH, HEIGHT)),
    );
    let marker_color = screen.theme.corner_marker;
    screen.draw_corner_markers(marker_color, 20, 4);
}

/// Inlined corner marker rendering for use inside with_texture_canvas closure.
fn draw_corner_markers_canvas(canvas: &mut Canvas<Window>, color: Color, inset: i32, size: i32) {
    canvas.set_draw_color(color);
    let w = WIDTH as i32;
    let h = HEIGHT as i32;
    // Top-left
    canvas.draw_line(Point::new(inset, inset), Point::new(inset + size, inset)).ok();
    canvas.draw_line(Point::new(inset, inset), Point::new(inset, inset + size)).ok();
    // Top-right
    canvas.draw_line(Point::new(w - inset - size, inset), Point::new(w - inset, inset)).ok();
    canvas.draw_line(Point::new(w - inset, inset), Point::new(w - inset, inset + size)).ok();
    // Bottom-left
    canvas.draw_line(Point::new(inset, h - inset - size), Point::new(inset, h - inset)).ok();
    canvas.draw_line(Point::new(inset, h - inset), Point::new(inset + size, h - inset)).ok();
    // Bottom-right
    canvas.draw_line(Point::new(w - inset - size, h - inset), Point::new(w - inset, h - inset)).ok();
    canvas.draw_line(Point::new(w - inset, h - inset - size), Point::new(w - inset, h - inset)).ok();
}
