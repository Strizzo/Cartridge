//! Top status bar widget -- 36px height, gradient bg, title left, WiFi dot + clock right.

use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::screen::Screen;

/// WiFi connection status for display in the status bar.
pub struct WifiStatus {
    pub connected: bool,
    pub signal_strength: i32, // 0-100, -1 = unknown
}

impl Default for WifiStatus {
    fn default() -> Self {
        Self {
            connected: false,
            signal_strength: -1,
        }
    }
}

/// Top bar showing app title, WiFi status dot, and clock/right text.
///
/// Height: 36px. Draws a gradient background from `header_gradient_top` to
/// `header_gradient_bottom`, a 1px accent line at the very top (y=0), the title
/// on the left, and WiFi dot + right text on the right.
pub struct StatusBar {
    pub title: String,
    pub right_text: String,
    pub right_color: Option<Color>,
    pub wifi: WifiStatus,
}

impl StatusBar {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            right_text: String::new(),
            right_color: None,
            wifi: WifiStatus::default(),
        }
    }

    /// Draw the status bar within `rect` (expected 36px tall).
    pub fn draw(&self, screen: &mut Screen, rect: Rect) {
        let theme = screen.theme;
        let pad = 12;

        // Gradient background
        screen.draw_gradient_rect(rect, theme.header_gradient_top, theme.header_gradient_bottom);

        // 1px accent-colored line at the very top of the bar ("brand bar")
        screen.draw_line(
            (rect.x(), rect.y()),
            (rect.x() + rect.width() as i32 - 1, rect.y()),
            Some(theme.accent),
            1,
        );

        // Title (left, bold, 16px, vertically centered)
        let title_h = screen.get_line_height(16, true);
        let title_y = rect.y() + (rect.height() as i32 - title_h as i32) / 2;
        screen.draw_text(
            &self.title,
            rect.x() + pad,
            title_y,
            Some(theme.text),
            16,
            true,
            None,
        );

        // Right edge cursor -- we build from right to left
        let mut right_edge = rect.x() + rect.width() as i32 - pad;

        // Right text (clock, status, etc.) -- 13px, dim
        if !self.right_text.is_empty() {
            let color = self.right_color.unwrap_or(theme.text_dim);
            let text_w = screen.get_text_width(&self.right_text, 13, false);
            let text_h = screen.get_line_height(13, false);
            let rx = right_edge - text_w as i32;
            let ry = rect.y() + (rect.height() as i32 - text_h as i32) / 2;
            screen.draw_text(&self.right_text, rx, ry, Some(color), 13, false, None);
            right_edge = rx - 14;
        }

        // WiFi indicator dot + label
        let cy = rect.y() + rect.height() as i32 / 2;
        if self.wifi.connected {
            let dot_color = if self.wifi.signal_strength > 50 {
                theme.positive
            } else if self.wifi.signal_strength > 20 {
                theme.text_warning
            } else {
                theme.negative
            };

            let label = "WiFi";
            let label_w = screen.get_text_width(label, 11, false);
            let label_h = screen.get_line_height(11, false);
            let wifi_w = 8 + 4 + label_w as i32; // dot_diameter + gap + label
            let wx = right_edge - wifi_w;

            // Dot (radius 3)
            screen.draw_circle(wx + 4, cy, 3, dot_color);

            // "WiFi" label
            screen.draw_text(
                label,
                wx + 12,
                cy - label_h as i32 / 2,
                Some(theme.text_dim),
                11,
                false,
                None,
            );
        } else {
            let label = "No WiFi";
            let label_w = screen.get_text_width(label, 11, false);
            let label_h = screen.get_line_height(11, false);
            let wx = right_edge - label_w as i32;
            screen.draw_text(
                label,
                wx,
                cy - label_h as i32 / 2,
                Some(theme.text_dim),
                11,
                false,
                None,
            );
        }

        // Bottom border line
        screen.draw_line(
            (rect.x(), rect.y() + rect.height() as i32 - 1),
            (
                rect.x() + rect.width() as i32 - 1,
                rect.y() + rect.height() as i32 - 1,
            ),
            Some(theme.border),
            1,
        );
    }
}
