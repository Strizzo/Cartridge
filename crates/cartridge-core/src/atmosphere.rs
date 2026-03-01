use crate::screen::{Screen, HEIGHT, WIDTH};

/// Atmosphere state for the cyberdeck dashboard look.
/// Static background (no animation) to minimize CPU usage.
pub struct Atmosphere;

impl Default for Atmosphere {
    fn default() -> Self {
        Self::new()
    }
}

impl Atmosphere {
    pub fn new() -> Self {
        Self
    }

    /// No-op: animations are disabled for CPU savings.
    pub fn update(&mut self, _dt: f32) {}

    /// Draw the atmospheric background: base color + grid + corner markers.
    /// Call this BEFORE drawing any screen content.
    pub fn draw_background(&self, screen: &mut Screen) {
        // Base dark background
        screen.clear(Some(screen.theme.bg));

        // Static circuit grid (single blit, no scrolling)
        screen.draw_image(
            "assets/overlays/grid_bg.png",
            0,
            0,
            Some((WIDTH, HEIGHT)),
            Some(sdl2::rect::Rect::new(0, 0, WIDTH, HEIGHT)),
        );

        // Corner markers
        let marker_color = screen.theme.corner_marker;
        screen.draw_corner_markers(marker_color, 20, 4);
    }

    /// Draw overlays ON TOP of all content: scanlines + vignette.
    pub fn draw_overlays(&self, screen: &mut Screen) {
        // Scanlines overlay
        screen.draw_image("assets/overlays/scanlines.png", 0, 0, None, None);

        // Vignette overlay
        screen.draw_image("assets/overlays/vignette.png", 0, 0, None, None);
    }
}
