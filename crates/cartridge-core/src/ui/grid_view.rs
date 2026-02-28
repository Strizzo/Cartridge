//! 2D navigable grid of cards with D-pad navigation, focus state, and vertical scroll.

use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::input::{Button, InputAction, InputEvent};
use crate::screen::Screen;

/// An item in the GridView.
#[derive(Clone, Debug)]
pub struct GridItem {
    pub id: String,
    pub label: String,
    /// Optional secondary text (e.g., subtitle).
    pub secondary: String,
    /// Application-defined tag.
    pub tag: i64,
}

impl GridItem {
    pub fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            secondary: String::new(),
            tag: 0,
        }
    }

    pub fn with_secondary(mut self, text: &str) -> Self {
        self.secondary = text.to_string();
        self
    }

    pub fn with_tag(mut self, tag: i64) -> Self {
        self.tag = tag;
        self
    }
}

/// 2D navigable card grid. D-pad moves in 4 directions. L1/R1 pages vertically.
/// A selects, B goes back.
///
/// Cell size is configurable (default 120x100). Columns are auto-calculated from
/// available width. Focus state uses accent border + card_highlight background.
pub struct GridView {
    pub items: Vec<GridItem>,
    pub cursor: usize,
    pub cell_width: i32,
    pub cell_height: i32,
    pub padding: i32,
    /// Number of columns, computed from available width.
    columns: usize,
    /// Scroll offset in rows.
    scroll_row: usize,
    /// Visible rows, computed from available height.
    visible_rows: usize,
}

impl GridView {
    pub fn new(items: Vec<GridItem>) -> Self {
        Self {
            items,
            cursor: 0,
            cell_width: 120,
            cell_height: 100,
            padding: 8,
            columns: 5,
            scroll_row: 0,
            visible_rows: 4,
        }
    }

    pub fn with_cell_size(mut self, width: i32, height: i32) -> Self {
        self.cell_width = width;
        self.cell_height = height;
        self
    }

    /// Get the currently focused item, if any.
    pub fn focused_item(&self) -> Option<&GridItem> {
        self.items.get(self.cursor)
    }

    /// Returns true if the event is an A press.
    pub fn is_select_event(&self, event: &InputEvent) -> bool {
        event.button == Button::A
            && event.action == InputAction::Press
            && !self.items.is_empty()
    }

    /// Handle D-pad navigation. Returns `true` if consumed.
    pub fn handle_input(&mut self, event: &InputEvent) -> bool {
        if self.items.is_empty() || self.columns == 0 {
            return false;
        }
        if event.action == InputAction::Release {
            return false;
        }

        let n = self.items.len();
        let row = self.cursor / self.columns;
        let col = self.cursor % self.columns;
        let total_rows = n.div_ceil(self.columns);

        match event.button {
            Button::DpadLeft => {
                if col > 0 {
                    self.cursor -= 1;
                }
                self.ensure_visible();
                true
            }
            Button::DpadRight => {
                if col + 1 < self.columns && self.cursor + 1 < n {
                    self.cursor += 1;
                }
                self.ensure_visible();
                true
            }
            Button::DpadUp => {
                if row > 0 {
                    self.cursor -= self.columns;
                }
                self.ensure_visible();
                true
            }
            Button::DpadDown => {
                if row + 1 < total_rows {
                    let new_cursor = self.cursor + self.columns;
                    self.cursor = new_cursor.min(n - 1);
                }
                self.ensure_visible();
                true
            }
            Button::L1 => {
                // Page up -- jump one screen of rows
                let jump = self.visible_rows.max(1) * self.columns;
                self.cursor = self.cursor.saturating_sub(jump);
                self.ensure_visible();
                true
            }
            Button::R1 => {
                // Page down -- jump one screen of rows
                let jump = self.visible_rows.max(1) * self.columns;
                self.cursor = (self.cursor + jump).min(n.saturating_sub(1));
                self.ensure_visible();
                true
            }
            _ => false,
        }
    }

    /// Ensure the cursor row is visible by adjusting scroll_row.
    fn ensure_visible(&mut self) {
        if self.columns == 0 {
            return;
        }
        let cursor_row = self.cursor / self.columns;
        if cursor_row < self.scroll_row {
            self.scroll_row = cursor_row;
        }
        if cursor_row >= self.scroll_row + self.visible_rows {
            self.scroll_row = cursor_row + 1 - self.visible_rows;
        }
    }

    /// Draw the grid within `rect`.
    pub fn draw(&mut self, screen: &mut Screen, rect: Rect) {
        let theme = screen.theme;

        // Background
        screen.draw_rect(rect, Some(theme.bg), true, 0, None);

        if self.items.is_empty() {
            let text = "No items";
            let tw = screen.get_text_width(text, 14, false);
            let th = screen.get_line_height(14, false);
            let mx = rect.x() + (rect.width() as i32 - tw as i32) / 2;
            let my = rect.y() + (rect.height() as i32 - th as i32) / 2;
            screen.draw_text(text, mx, my, Some(theme.text_dim), 14, false, None);
            return;
        }

        // Calculate columns from available width
        let avail_w = rect.width() as i32;
        self.columns = ((avail_w + self.padding) / (self.cell_width + self.padding)).max(1) as usize;

        // Calculate visible rows from available height
        self.visible_rows =
            ((rect.height() as i32 + self.padding) / (self.cell_height + self.padding)).max(1)
                as usize;

        // Clamp cursor
        self.cursor = self.cursor.min(self.items.len() - 1);
        self.ensure_visible();

        let n = self.items.len();
        let total_rows = n.div_ceil(self.columns);

        // Horizontal centering offset
        let total_grid_w = self.columns as i32 * (self.cell_width + self.padding) - self.padding;
        let x_offset = (avail_w - total_grid_w) / 2;

        // Draw visible rows
        for vr in 0..self.visible_rows {
            let row = self.scroll_row + vr;
            if row >= total_rows {
                break;
            }
            for col in 0..self.columns {
                let idx = row * self.columns + col;
                if idx >= n {
                    break;
                }

                let item = &self.items[idx];
                let is_focused = idx == self.cursor;

                let cx = rect.x() + x_offset + col as i32 * (self.cell_width + self.padding);
                let cy = rect.y() + vr as i32 * (self.cell_height + self.padding);

                let cell_rect = Rect::new(cx, cy, self.cell_width as u32, self.cell_height as u32);

                // Card background
                if is_focused {
                    screen.draw_card(
                        cell_rect,
                        Some(theme.card_highlight),
                        Some(theme.accent),
                        6,
                        true,
                    );
                } else {
                    screen.draw_card(
                        cell_rect,
                        Some(theme.card_bg),
                        Some(theme.card_border),
                        6,
                        false,
                    );
                }

                // Label (centered, bottom portion of the card)
                let label_font_size: u16 = 13;
                let label_h = screen.get_line_height(label_font_size, false);
                let label_y = cy + self.cell_height - label_h as i32 - 8;
                let label_color = if is_focused {
                    theme.text
                } else {
                    theme.text_dim
                };

                // Center the label text
                let lw = screen.get_text_width(&item.label, label_font_size, false);
                let max_label_w = (self.cell_width - 8) as u32;
                let lx = if lw < max_label_w {
                    cx + (self.cell_width - lw as i32) / 2
                } else {
                    cx + 4
                };

                screen.draw_text(
                    &item.label,
                    lx,
                    label_y,
                    Some(label_color),
                    label_font_size,
                    false,
                    Some(max_label_w),
                );

                // Secondary text if present
                if !item.secondary.is_empty() {
                    let sec_font_size: u16 = 11;
                    let sec_h = screen.get_line_height(sec_font_size, false);
                    let sec_y = label_y - sec_h as i32 - 2;
                    let sec_w = screen.get_text_width(&item.secondary, sec_font_size, false);
                    let max_sec_w = (self.cell_width - 8) as u32;
                    let sec_x = if sec_w < max_sec_w {
                        cx + (self.cell_width - sec_w as i32) / 2
                    } else {
                        cx + 4
                    };
                    screen.draw_text(
                        &item.secondary,
                        sec_x,
                        sec_y,
                        Some(theme.text_dim),
                        sec_font_size,
                        false,
                        Some(max_sec_w),
                    );
                }
            }
        }

        // Scroll indicator (right edge)
        if total_rows > self.visible_rows {
            let ind_x = rect.x() + rect.width() as i32 - 5;
            let bar_top = rect.y() + 6;
            let bar_h = rect.height() as i32 - 12;

            let track_color = Color::RGB(
                theme.border.r.saturating_add(10),
                theme.border.g.saturating_add(10),
                theme.border.b.saturating_add(10),
            );
            screen.draw_line(
                (ind_x, bar_top),
                (ind_x, bar_top + bar_h),
                Some(track_color),
                1,
            );

            let thumb_h = ((bar_h * self.visible_rows as i32) / total_rows as i32).max(8);
            let progress = self.scroll_row as f32 / (total_rows - self.visible_rows) as f32;
            let thumb_y = bar_top + ((bar_h - thumb_h) as f32 * progress) as i32;
            screen.draw_rect(
                Rect::new(ind_x - 1, thumb_y, 3, thumb_h as u32),
                Some(theme.text_dim),
                true,
                1,
                None,
            );
        }
    }
}
