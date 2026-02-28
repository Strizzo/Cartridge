//! Scrollable focus list with D-pad navigation, custom row heights, L1/R1 page, scroll indicator.

use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::input::{Button, InputAction, InputEvent};
use crate::screen::Screen;

const PAD_X: i32 = 10;
const PAD_Y: i32 = 6;
const CARD_MARGIN: i32 = 3;
const CARD_RADIUS: i16 = 6;

/// An item in a ListView.
#[derive(Clone, Debug)]
pub struct ListItem {
    pub id: String,
    pub primary_text: String,
    pub secondary_text: String,
    pub right_text: String,
    /// Application-defined tag for custom rendering or data association.
    pub tag: i64,
}

impl ListItem {
    pub fn new(id: &str, primary: &str) -> Self {
        Self {
            id: id.to_string(),
            primary_text: primary.to_string(),
            secondary_text: String::new(),
            right_text: String::new(),
            tag: 0,
        }
    }

    pub fn with_secondary(mut self, text: &str) -> Self {
        self.secondary_text = text.to_string();
        self
    }

    pub fn with_right(mut self, text: &str) -> Self {
        self.right_text = text.to_string();
        self
    }

    pub fn with_tag(mut self, tag: i64) -> Self {
        self.tag = tag;
        self
    }
}

/// Scrollable list with card-style items and D-pad navigation.
///
/// Default item height is 48px; set `item_height` to 32 for compact mode.
/// D-pad Up/Down moves cursor. L1/R1 pages. A selects.
pub struct ListView {
    pub items: Vec<ListItem>,
    pub cursor: usize,
    pub item_height: i32,
    last_visible_count: usize,
}

impl ListView {
    pub fn new(items: Vec<ListItem>, item_height: i32) -> Self {
        Self {
            items,
            cursor: 0,
            item_height,
            last_visible_count: 10,
        }
    }

    /// Create with default 48px row height.
    pub fn default_height(items: Vec<ListItem>) -> Self {
        Self::new(items, 48)
    }

    /// Create with compact 32px row height.
    pub fn compact(items: Vec<ListItem>) -> Self {
        Self::new(items, 32)
    }

    /// Get the currently focused item, if any.
    pub fn focused_item(&self) -> Option<&ListItem> {
        self.items.get(self.cursor)
    }

    /// Handle D-pad and paging input. Returns `true` if consumed.
    pub fn handle_input(&mut self, event: &InputEvent) -> bool {
        if self.items.is_empty() {
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
                if self.cursor + 1 < self.items.len() {
                    self.cursor += 1;
                }
                true
            }
            Button::L1 => {
                let page = self.last_visible_count.max(1);
                self.cursor = self.cursor.saturating_sub(page);
                true
            }
            Button::R1 => {
                let page = self.last_visible_count.max(1);
                self.cursor = (self.cursor + page).min(self.items.len().saturating_sub(1));
                true
            }
            _ => false,
        }
    }

    /// Returns true if the event is an A press on the current item.
    pub fn is_select_event(&self, event: &InputEvent) -> bool {
        event.button == Button::A
            && event.action == InputAction::Press
            && !self.items.is_empty()
    }

    /// Draw the list view within `rect`.
    pub fn draw(&mut self, screen: &mut Screen, rect: Rect) {
        let n = self.items.len();
        let visible_count = (rect.height() as i32 / self.item_height).max(1) as usize;
        self.last_visible_count = visible_count;

        let theme = screen.theme;

        // Background
        screen.draw_rect(rect, Some(theme.bg), true, 0, None);

        if n == 0 {
            let text = "No items";
            let tw = screen.get_text_width(text, 14, false);
            let th = screen.get_line_height(14, false);
            let mx = rect.x() + (rect.width() as i32 - tw as i32) / 2;
            let my = rect.y() + (rect.height() as i32 - th as i32) / 2;
            screen.draw_text(text, mx, my, Some(theme.text_dim), 14, false, None);
            return;
        }

        // Clamp cursor
        self.cursor = self.cursor.min(n - 1);

        // Sliding window
        let window_start = window_start(self.cursor, n, visible_count);
        let window_end = (window_start + visible_count).min(n);

        let mut y = rect.y();
        for i in window_start..window_end {
            let item = &self.items[i];
            let is_selected = i == self.cursor;
            self.draw_default_item(screen, rect, item, is_selected, y);
            y += self.item_height;
        }

        // Scroll indicator
        if n > visible_count {
            draw_scroll_indicator(screen, rect, self.cursor, n);
        }
    }

    fn draw_default_item(
        &self,
        screen: &mut Screen,
        rect: Rect,
        item: &ListItem,
        is_selected: bool,
        y: i32,
    ) {
        let theme = screen.theme;

        let card_x = rect.x() + CARD_MARGIN;
        let card_w = (rect.width() as i32 - CARD_MARGIN * 2 - 8) as u32;
        let card_h = (self.item_height - CARD_MARGIN) as u32;

        let card_rect = Rect::new(card_x, y + 1, card_w, card_h);

        if is_selected {
            screen.draw_rect(card_rect, Some(theme.card_highlight), true, CARD_RADIUS, None);
            screen.draw_rect(card_rect, Some(theme.accent), false, CARD_RADIUS, None);
        } else {
            screen.draw_rect(card_rect, Some(theme.card_bg), true, CARD_RADIUS, None);
        }

        let max_text_w = card_w - PAD_X as u32 * 2 - 70;

        // Primary text
        let text_x = card_x + PAD_X + 4;
        let primary_color = if is_selected {
            theme.text
        } else {
            theme.text_dim
        };

        let primary_h = screen.get_line_height(15, false);
        let text_y = if !item.secondary_text.is_empty() {
            y + 5
        } else {
            y + (card_h as i32 - primary_h as i32) / 2
        };

        screen.draw_text(
            &item.primary_text,
            text_x,
            text_y,
            Some(primary_color),
            15,
            false,
            Some(max_text_w),
        );

        // Secondary text
        if !item.secondary_text.is_empty() {
            let primary_lh = screen.get_line_height(15, false);
            screen.draw_text(
                &item.secondary_text,
                text_x,
                text_y + primary_lh as i32 + 1,
                Some(theme.text_dim),
                12,
                false,
                Some(max_text_w),
            );
        }

        // Right-aligned text
        if !item.right_text.is_empty() {
            let rw = screen.get_text_width(&item.right_text, 13, false);
            let rh = screen.get_line_height(13, false);
            let rx = card_x + card_w as i32 - PAD_X - rw as i32;
            let ry = y + (card_h as i32 - rh as i32) / 2 + 1;
            screen.draw_text(
                &item.right_text,
                rx,
                ry,
                Some(theme.text_dim),
                13,
                false,
                None,
            );
        }
    }
}

fn window_start(cursor: usize, total: usize, visible: usize) -> usize {
    if total <= visible {
        return 0;
    }
    let start = cursor.saturating_sub(1);
    start.min(total - visible)
}

fn draw_scroll_indicator(screen: &mut Screen, rect: Rect, cursor: usize, total: usize) {
    let theme = screen.theme;
    let ind_x = rect.x() + rect.width() as i32 - 5;
    let bar_top = rect.y() + PAD_Y;
    let bar_h = rect.height() as i32 - PAD_Y * 2;

    if total <= 1 || bar_h <= 0 {
        return;
    }

    // Track
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

    // Thumb
    let thumb_h = (bar_h / total as i32).max(8);
    let progress = cursor as f32 / (total - 1) as f32;
    let thumb_y = bar_top + ((bar_h - thumb_h) as f32 * progress) as i32;
    screen.draw_rect(
        Rect::new(ind_x - 1, thumb_y, 3, thumb_h as u32),
        Some(theme.text_dim),
        true,
        1,
        None,
    );
}
