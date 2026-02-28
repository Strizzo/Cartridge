//! Terminal spinner loading indicator -- cycles through |, /, -, \ at 4fps, centered, overlay.

use sdl2::rect::Rect;

use crate::screen::Screen;

/// Spinner character frames (classic terminal spinner).
const SPINNER_FRAMES: &[char] = &['|', '/', '-', '\\'];

/// Loading indicator with a terminal spinner (|, /, -, \) and text label, rendered
/// centered over a semi-transparent overlay.
pub struct LoadingIndicator {
    pub text: String,
    pub visible: bool,
    /// Accumulated time in seconds for spinner animation.
    elapsed: f32,
}

impl LoadingIndicator {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            visible: false,
            elapsed: 0.0,
        }
    }

    /// Advance the spinner. Call once per frame with delta time in seconds.
    pub fn update(&mut self, dt: f32) {
        self.elapsed += dt;
    }

    /// Draw the loading overlay within `rect`. Does nothing if `visible` is false.
    ///
    /// Renders a semi-transparent darkened overlay over the rect, then a centered
    /// spinner character in accent color followed by the text label.
    pub fn draw(&self, screen: &mut Screen, rect: Rect) {
        if !self.visible {
            return;
        }

        let theme = screen.theme;

        // Semi-transparent overlay -- draw a dark filled rect.
        // We cannot do true alpha blending without SDL surfaces, so we draw a solid
        // dark rect that approximates an overlay.
        screen.draw_rect(rect, Some(theme.bg), true, 0, None);

        // Spinner frame (4fps = change every 0.25s)
        let frame_index = (self.elapsed / 0.25) as usize % SPINNER_FRAMES.len();
        let spinner_char = SPINNER_FRAMES[frame_index];

        // Build display string: "| Loading" or "/ Loading" etc.
        let display = format!("{} {}", spinner_char, self.text);

        let font_size: u16 = 16;
        let tw = screen.get_text_width(&display, font_size, false);
        let th = screen.get_line_height(font_size, false);

        let x = rect.x() + (rect.width() as i32 - tw as i32) / 2;
        let y = rect.y() + (rect.height() as i32 - th as i32) / 2;

        screen.draw_text(
            &display,
            x,
            y,
            Some(theme.text_accent),
            font_size,
            false,
            None,
        );
    }
}
