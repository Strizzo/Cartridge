//! Footer button hint bar -- 36px, colored badges for face buttons.

use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::screen::Screen;

/// A single button hint entry (e.g., "A" -> "Open").
#[derive(Clone, Debug)]
pub struct ButtonHint {
    /// Button label text (e.g., "A", "B", "L1", "START").
    pub label: String,
    /// Action description (e.g., "Open", "Back", "Refresh").
    pub action: String,
    /// Badge color for the button. If None, uses theme.accent.
    pub color: Option<Color>,
}

impl ButtonHint {
    pub fn new(label: &str, action: &str) -> Self {
        Self {
            label: label.to_string(),
            action: action.to_string(),
            color: None,
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Create an A-button hint (green badge).
    pub fn a(action: &str, theme: &crate::theme::Theme) -> Self {
        Self::new("A", action).with_color(theme.btn_a)
    }

    /// Create a B-button hint (red badge).
    pub fn b(action: &str, theme: &crate::theme::Theme) -> Self {
        Self::new("B", action).with_color(theme.btn_b)
    }

    /// Create an X-button hint (blue badge).
    pub fn x(action: &str, theme: &crate::theme::Theme) -> Self {
        Self::new("X", action).with_color(theme.btn_x)
    }

    /// Create a Y-button hint (yellow badge).
    pub fn y(action: &str, theme: &crate::theme::Theme) -> Self {
        Self::new("Y", action).with_color(theme.btn_y)
    }

    /// Create an L1/R1 hint (gray badge).
    pub fn lr(label: &str, action: &str, theme: &crate::theme::Theme) -> Self {
        Self::new(label, action).with_color(theme.btn_l)
    }

    /// Create a START hint.
    pub fn start(action: &str, theme: &crate::theme::Theme) -> Self {
        Self::new("START", action).with_color(theme.btn_l)
    }
}

/// Footer bar that displays context-sensitive button hints.
///
/// Height: 36px. Renders colored badge pills for each button followed by the
/// action label. Badges match the physical button colors on the device.
pub struct Footer {
    pub hints: Vec<ButtonHint>,
}

impl Footer {
    pub fn new(hints: Vec<ButtonHint>) -> Self {
        Self { hints }
    }

    /// Set the hints to display.
    pub fn set_hints(&mut self, hints: Vec<ButtonHint>) {
        self.hints = hints;
    }

    /// Draw the footer within `rect` (expected 36px tall).
    pub fn draw(&self, screen: &mut Screen, rect: Rect) {
        let theme = screen.theme;

        // Background
        screen.draw_rect(rect, Some(theme.bg_header), true, 0, None);

        // Top border line
        screen.draw_line(
            (rect.x(), rect.y()),
            (rect.x() + rect.width() as i32 - 1, rect.y()),
            Some(theme.border),
            1,
        );

        if self.hints.is_empty() {
            return;
        }

        let font_size: u16 = 12;
        let hint_y = rect.y() + (rect.height() as i32 - screen.get_line_height(font_size, true) as i32) / 2 - 1;
        let gap = 16i32;
        let pad = 12i32;

        let mut x = rect.x() + pad;
        for hint in &self.hints {
            let btn_color = hint.color.unwrap_or(theme.accent);
            let w = screen.draw_button_hint(
                &hint.label,
                &hint.action,
                x,
                hint_y,
                Some(btn_color),
                font_size,
            );
            x += w as i32 + gap;

            // Stop if we're running out of space
            if x >= rect.x() + rect.width() as i32 - pad {
                break;
            }
        }
    }
}
