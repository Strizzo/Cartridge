//! Horizontal tab bar switched by L1/R1 -- 30px height, colored active indicator, auto-width.

use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::input::{Button, InputAction, InputEvent};
use crate::screen::Screen;

/// A single tab definition.
#[derive(Clone, Debug)]
pub struct Tab {
    pub label: String,
    pub id: String,
}

impl Tab {
    pub fn new(label: &str, id: &str) -> Self {
        Self {
            label: label.to_string(),
            id: id.to_string(),
        }
    }
}

/// Horizontal tab bar. L1 moves left, R1 moves right.
///
/// Height: 30px. Tabs are auto-width (label width + 24px padding), left-aligned.
/// The active tab has an accent-colored indicator bar at the bottom and accent-colored text.
/// Inactive tabs use `text_dim`.
pub struct TabBar {
    pub tabs: Vec<Tab>,
    pub active_index: usize,
    /// Optional accent color override for the active indicator. Defaults to `theme.accent`.
    pub active_color: Option<Color>,
}

impl TabBar {
    pub fn new(tabs: Vec<Tab>) -> Self {
        Self {
            tabs,
            active_index: 0,
            active_color: None,
        }
    }

    /// Get the currently active tab, if any.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_index)
    }

    /// Handle L1/R1 input. Returns `true` if the active tab changed.
    pub fn handle_input(&mut self, event: &InputEvent) -> bool {
        if event.action == InputAction::Release {
            return false;
        }
        match event.button {
            Button::L1 => {
                if self.active_index > 0 {
                    self.active_index -= 1;
                    return true;
                }
            }
            Button::R1 => {
                if self.active_index + 1 < self.tabs.len() {
                    self.active_index += 1;
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    /// Draw the tab bar within `rect` (expected 30px tall).
    pub fn draw(&self, screen: &mut Screen, rect: Rect) {
        let theme = screen.theme;

        // Background
        screen.draw_rect(rect, Some(theme.bg_header), true, 0, None);

        if self.tabs.is_empty() {
            return;
        }

        let font_size: u16 = 14;
        let tab_padding: i32 = 24; // 12px on each side

        // Calculate auto-width for each tab
        let tab_widths: Vec<i32> = self
            .tabs
            .iter()
            .map(|tab| {
                let tw = screen.get_text_width(&tab.label, font_size, true) as i32;
                tw + tab_padding
            })
            .collect();

        let accent = self.active_color.unwrap_or(theme.accent);

        let mut tx = rect.x();
        for (i, tab) in self.tabs.iter().enumerate() {
            let w = tab_widths[i];
            let is_active = i == self.active_index;

            let color = if is_active { accent } else { theme.text_dim };

            let lw = screen.get_text_width(&tab.label, font_size, true);
            let lh = screen.get_line_height(font_size, true);
            let lx = tx + (w - lw as i32) / 2;
            let ly = rect.y() + (rect.height() as i32 - lh as i32) / 2 - 2;

            screen.draw_text(&tab.label, lx, ly, Some(color), font_size, true, None);

            // Active indicator bar (3px tall at the bottom)
            if is_active {
                let bar_y = rect.y() + rect.height() as i32 - 3;
                screen.draw_rect(
                    Rect::new(tx + 4, bar_y, (w - 8) as u32, 3),
                    Some(accent),
                    true,
                    0,
                    None,
                );
            }

            tx += w;
        }

        // Bottom border
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
