use sdl2::rect::Rect;

use crate::screen::{Screen, HEIGHT, WIDTH};

const GRID_SCROLL_SPEED: f32 = 12.0; // pixels per second
const GRID_TILE_HEIGHT: u32 = 1440;
const SWEEP_SPEED: f32 = 40.0; // pixels per second
const PULSE_PERIOD: f32 = 3.0; // seconds

/// Animated atmosphere state for the cyberdeck dashboard look.
pub struct Atmosphere {
    grid_scroll_offset: f32,
    sweep_y: f32,
    pulse_timer: f32,
}

impl Default for Atmosphere {
    fn default() -> Self {
        Self::new()
    }
}

impl Atmosphere {
    pub fn new() -> Self {
        Self {
            grid_scroll_offset: 0.0,
            sweep_y: 0.0,
            pulse_timer: 0.0,
        }
    }

    /// Advance all animation timers by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        // Scroll grid downward
        self.grid_scroll_offset += GRID_SCROLL_SPEED * dt;
        if self.grid_scroll_offset >= GRID_TILE_HEIGHT as f32 / 2.0 {
            self.grid_scroll_offset -= GRID_TILE_HEIGHT as f32 / 2.0;
        }

        // Sweep line descends
        self.sweep_y += SWEEP_SPEED * dt;
        if self.sweep_y > HEIGHT as f32 {
            self.sweep_y = 0.0;
        }

        // Pulse oscillation
        self.pulse_timer += dt;
        if self.pulse_timer > PULSE_PERIOD {
            self.pulse_timer -= PULSE_PERIOD;
        }
    }

    /// Returns 0.0..1.0 sine-wave pulse value (period ~3s).
    pub fn pulse(&self) -> f32 {
        let t = self.pulse_timer / PULSE_PERIOD;
        (t * std::f32::consts::TAU).sin() * 0.5 + 0.5
    }

    /// Draw the atmospheric background: base color + scrolling grid + corner markers.
    /// Call this BEFORE drawing any screen content.
    pub fn draw_background(&self, screen: &mut Screen) {
        // Base dark background
        screen.clear(Some(screen.theme.bg));

        // Scrolling circuit grid (draw two tiles for seamless scroll)
        let grid_path = "assets/overlays/grid_bg.png";
        let offset = self.grid_scroll_offset as i32;
        // The grid image is 640x960 (double-height). We show a 640x480 window
        // into it, scrolling downward. Draw the image offset up by scroll amount,
        // and a second copy below for seamless wrapping.
        let src_y = offset;
        let remaining = GRID_TILE_HEIGHT as i32 - src_y;

        if remaining >= HEIGHT as i32 {
            // Single blit covers the screen
            screen.draw_image(
                grid_path,
                0,
                0,
                Some((WIDTH, HEIGHT)),
                Some(Rect::new(0, src_y, WIDTH, HEIGHT)),
            );
        } else {
            // Need two blits to tile
            if remaining > 0 {
                screen.draw_image(
                    grid_path,
                    0,
                    0,
                    Some((WIDTH, remaining as u32)),
                    Some(Rect::new(0, src_y, WIDTH, remaining as u32)),
                );
            }
            let second_h = HEIGHT as i32 - remaining;
            if second_h > 0 {
                screen.draw_image(
                    grid_path,
                    0,
                    remaining,
                    Some((WIDTH, second_h as u32)),
                    Some(Rect::new(0, 0, WIDTH, second_h as u32)),
                );
            }
        }

        // Corner markers
        let marker_color = screen.theme.corner_marker;
        screen.draw_corner_markers(marker_color, 20, 4);
    }

    /// Draw overlays ON TOP of all content: sweep line + scanlines + vignette.
    pub fn draw_overlays(&self, screen: &mut Screen) {
        // Horizontal sweep line
        let sweep_y = self.sweep_y as i32;
        let sweep_color = screen.theme.sweep_line;
        screen.draw_glow_line(sweep_y, 0, WIDTH as i32 - 1, sweep_color, 3, 1);

        // Scanlines overlay
        screen.draw_image("assets/overlays/scanlines.png", 0, 0, None, None);

        // Vignette overlay
        screen.draw_image("assets/overlays/vignette.png", 0, 0, None, None);
    }
}
