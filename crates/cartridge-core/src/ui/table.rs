//! Columnar data table with headers, scrollable rows, alternating backgrounds, right-aligned numbers.

use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::input::{Button, InputAction, InputEvent};
use crate::screen::Screen;

const PAD_X: i32 = 10;
const PAD_Y: i32 = 6;
const CARD_MARGIN: i32 = 3;
const CARD_RADIUS: i16 = 6;

/// Column alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Center,
}

/// Table column definition.
#[derive(Clone, Debug)]
pub struct Column {
    pub header: String,
    /// Width as fraction of available width (0.0 - 1.0).
    pub width_pct: f32,
    pub align: Align,
}

impl Column {
    pub fn new(header: &str, width_pct: f32) -> Self {
        Self {
            header: header.to_string(),
            width_pct,
            align: Align::Left,
        }
    }

    pub fn right(header: &str, width_pct: f32) -> Self {
        Self {
            header: header.to_string(),
            width_pct,
            align: Align::Right,
        }
    }

    pub fn center(header: &str, width_pct: f32) -> Self {
        Self {
            header: header.to_string(),
            width_pct,
            align: Align::Center,
        }
    }
}

/// Row color override callback type -- given a cell value, returns an optional color.
pub type CellColorFn = fn(&str) -> Option<Color>;

/// Columnar data display with card-style rows, selection, and scroll indicator.
///
/// D-pad Up/Down moves cursor. A selects. Alternating row backgrounds for readability.
pub struct Table {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<String>>,
    pub cursor: usize,
    /// Optional per-column color function for cell text coloring.
    pub color_fns: Vec<Option<CellColorFn>>,
}

impl Table {
    pub fn new(columns: Vec<Column>, rows: Vec<Vec<String>>) -> Self {
        let ncols = columns.len();
        Self {
            columns,
            rows,
            cursor: 0,
            color_fns: vec![None; ncols],
        }
    }

    /// Set a color function for a specific column.
    pub fn set_color_fn(&mut self, col_index: usize, f: CellColorFn) {
        if col_index < self.color_fns.len() {
            self.color_fns[col_index] = Some(f);
        }
    }

    /// Get the currently focused row, if any.
    pub fn focused_row(&self) -> Option<&Vec<String>> {
        self.rows.get(self.cursor)
    }

    /// Handle D-pad input. Returns `true` if consumed.
    pub fn handle_input(&mut self, event: &InputEvent) -> bool {
        if self.rows.is_empty() {
            return false;
        }
        if event.action == InputAction::Release {
            return false;
        }

        match event.button {
            Button::DpadUp => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                true
            }
            Button::DpadDown => {
                if self.cursor + 1 < self.rows.len() {
                    self.cursor += 1;
                }
                true
            }
            _ => false,
        }
    }

    /// Returns true if the event is an A press on the current row.
    pub fn is_select_event(&self, event: &InputEvent) -> bool {
        event.button == Button::A
            && event.action == InputAction::Press
            && !self.rows.is_empty()
    }

    /// Draw the table within `rect`.
    pub fn draw(&mut self, screen: &mut Screen, rect: Rect) {
        let theme = screen.theme;

        screen.draw_rect(rect, Some(theme.bg), true, 0, None);

        let header_lh = screen.get_line_height(13, true) as i32;
        let cell_lh = screen.get_line_height(14, false) as i32;
        let header_h = header_lh + PAD_Y * 2;
        let row_h = cell_lh + PAD_Y * 2 + CARD_MARGIN;

        // Column pixel widths
        let available_w = rect.width() as i32 - PAD_X * 2;
        let col_widths: Vec<i32> = self
            .columns
            .iter()
            .map(|c| (c.width_pct * available_w as f32) as i32)
            .collect();

        // Draw header card
        let header_card = Rect::new(
            rect.x() + CARD_MARGIN,
            rect.y() + 2,
            (rect.width() as i32 - CARD_MARGIN * 2 - 8) as u32,
            header_h as u32,
        );
        screen.draw_rect(header_card, Some(theme.bg_lighter), true, CARD_RADIUS, None);

        let mut hx = rect.x() + PAD_X + CARD_MARGIN;
        let hy = rect.y() + PAD_Y + 2;
        for (i, col) in self.columns.iter().enumerate() {
            draw_cell(
                screen,
                &col.header,
                hx,
                hy,
                col_widths[i],
                col.align,
                13,
                true,
                theme.text_accent,
            );
            hx += col_widths[i];
        }

        // Rows
        if self.rows.is_empty() {
            let text = "No data";
            let tw = screen.get_text_width(text, 14, false);
            let mx = rect.x() + (rect.width() as i32 - tw as i32) / 2;
            let my = rect.y() + header_h + 20;
            screen.draw_text(text, mx, my, Some(theme.text_dim), 14, false, None);
            return;
        }

        self.cursor = self.cursor.min(self.rows.len() - 1);

        let visible_rows = ((rect.height() as i32 - header_h - PAD_Y) / row_h).max(1) as usize;
        let start = if self.rows.len() <= visible_rows {
            0
        } else {
            self.cursor.saturating_sub(1).min(self.rows.len() - visible_rows)
        };
        let end = (start + visible_rows).min(self.rows.len());

        let mut y = rect.y() + header_h + 4;
        for row_idx in start..end {
            let row = &self.rows[row_idx];
            let is_selected = row_idx == self.cursor;

            let card_x = rect.x() + CARD_MARGIN;
            let card_w = (rect.width() as i32 - CARD_MARGIN * 2 - 8) as u32;
            let card_h = (row_h - CARD_MARGIN) as u32;
            let card_rect = Rect::new(card_x, y, card_w, card_h);

            if is_selected {
                screen.draw_rect(card_rect, Some(theme.card_highlight), true, CARD_RADIUS, None);
                screen.draw_rect(card_rect, Some(theme.accent), false, CARD_RADIUS, None);
            } else {
                // Alternating row backgrounds
                let bg = if row_idx % 2 == 0 {
                    theme.card_bg
                } else {
                    theme.bg_lighter
                };
                screen.draw_rect(card_rect, Some(bg), true, CARD_RADIUS, None);
            }

            let mut rx = rect.x() + PAD_X + CARD_MARGIN;
            for (i, col) in self.columns.iter().enumerate() {
                let cell_text = row.get(i).map(|s| s.as_str()).unwrap_or("");
                let cell_color = if i < self.color_fns.len() {
                    self.color_fns[i]
                        .and_then(|f| f(cell_text))
                        .unwrap_or(theme.text)
                } else {
                    theme.text
                };

                draw_cell(
                    screen,
                    cell_text,
                    rx,
                    y + PAD_Y,
                    col_widths[i],
                    col.align,
                    14,
                    false,
                    cell_color,
                );
                rx += col_widths[i];
            }

            y += row_h;
        }

        // Scroll indicator
        if self.rows.len() > visible_rows {
            let ind_x = rect.x() + rect.width() as i32 - 5;
            let bar_top = rect.y() + header_h + 4;
            let bar_h = rect.height() as i32 - header_h - 8;
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
            let thumb_h = ((bar_h * visible_rows as i32) / self.rows.len() as i32).max(8);
            let progress = self.cursor as f32 / (self.rows.len() - 1) as f32;
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

#[allow(clippy::too_many_arguments)]
fn draw_cell(
    screen: &mut Screen,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    align: Align,
    font_size: u16,
    bold: bool,
    color: Color,
) {
    if text.is_empty() {
        return;
    }

    // Truncate if needed
    let tw = screen.get_text_width(text, font_size, bold) as i32;
    let max_w = width - 4;

    let display: String;
    let final_w: i32;

    if tw > max_w {
        let mut truncated = text.to_string();
        while truncated.len() > 1 {
            truncated.pop();
            let candidate = format!("{truncated}..");
            let cw = screen.get_text_width(&candidate, font_size, bold) as i32;
            if cw <= max_w {
                display = candidate;
                final_w = cw;
                screen.draw_text(&display, aligned_x(x, width, final_w, align), y, Some(color), font_size, bold, None);
                return;
            }
        }
        display = "..".to_string();
        final_w = screen.get_text_width(&display, font_size, bold) as i32;
    } else {
        display = text.to_string();
        final_w = tw;
    }

    screen.draw_text(
        &display,
        aligned_x(x, width, final_w, align),
        y,
        Some(color),
        font_size,
        bold,
        None,
    );
}

fn aligned_x(x: i32, width: i32, text_w: i32, align: Align) -> i32 {
    match align {
        Align::Left => x,
        Align::Right => x + width - text_w - 4,
        Align::Center => x + (width - text_w) / 2,
    }
}
