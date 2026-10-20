//! On-screen QWERTY keyboard for text entry (WiFi passwords, search, etc.)
//!
//! Full-screen modal overlay with a character grid, input field with a visible cursor,
//! and mode toggle between lowercase and uppercase/symbols.
//!
//! Controls:
//! - D-pad: navigate keyboard grid
//! - A: type focused character
//! - B: delete character before cursor (backspace)
//! - X: toggle shift mode (lowercase → uppercase → symbols)
//! - Y: insert space
//! - L1: move cursor left
//! - R1: move cursor right
//! - START: submit input
//! - SELECT: cancel input

use sdl2::rect::Rect;

use crate::input::{Button, InputAction, InputEvent};
use crate::screen::{Screen, HEIGHT, WIDTH};

// ---------------------------------------------------------------------------
// Keyboard layouts
// ---------------------------------------------------------------------------

const COLS: usize = 10;
const ROWS: usize = 4;

const LAYOUT_LOWER: [[char; COLS]; ROWS] = [
    ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'],
    ['q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p'],
    ['a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', '@'],
    ['z', 'x', 'c', 'v', 'b', 'n', 'm', '.', '-', '_'],
];

const LAYOUT_UPPER: [[char; COLS]; ROWS] = [
    ['!', '#', '$', '%', '&', '*', '(', ')', '+', '='],
    ['Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P'],
    ['A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', ':'],
    ['Z', 'X', 'C', 'V', 'B', 'N', 'M', ';', '?', '/'],
];

const LAYOUT_SYMBOLS: [[char; COLS]; ROWS] = [
    ['[', ']', '{', '}', '<', '>', '\'', '"', '`', '~'],
    ['!', '@', '#', '$', '%', '^', '&', '*', '(', ')'],
    ['-', '_', '=', '+', '/', '\\', '|', ':', ';', '?'],
    ['.', ',', '0', '1', '2', '3', '4', '5', '6', '7'],
];

// ---------------------------------------------------------------------------
// Layout constants (for 720x720 screen)
// ---------------------------------------------------------------------------

const DIALOG_W: i32 = 680;
const DIALOG_X: i32 = (WIDTH as i32 - DIALOG_W) / 2;

const KEY_W: i32 = 62;
const KEY_H: i32 = 44;
const KEY_GAP: i32 = 4;
const KEY_FONT: u16 = 16;

const KB_TOTAL_W: i32 = COLS as i32 * KEY_W + (COLS as i32 - 1) * KEY_GAP;
const KB_X_OFFSET: i32 = (DIALOG_W - KB_TOTAL_W) / 2;

const INPUT_FIELD_H: i32 = 44;
const INPUT_FONT: u16 = 16;
const INPUT_PAD: i32 = 12;

const MAX_INPUT_LEN: usize = 64;

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputResult {
    Pending,
    Submitted(String),
    Cancelled,
}

// ---------------------------------------------------------------------------
// TextInput widget
// ---------------------------------------------------------------------------

pub struct TextInput {
    pub label: String,
    pub text: String,
    pub cursor_pos: usize,
    pub visible: bool,
    pub result: TextInputResult,
    /// If true, display characters as '*' in the input field.
    pub masked: bool,
    /// Maximum Unicode characters accepted (64 by default).
    pub max_len: usize,

    grid_row: usize,
    grid_col: usize,
    mode: usize,
}

impl TextInput {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            text: String::new(),
            cursor_pos: 0,
            visible: false,
            result: TextInputResult::Pending,
            masked: false,
            max_len: MAX_INPUT_LEN,

            grid_row: 1,
            grid_col: 0,
            mode: 0,
        }
    }

    /// Show the input dialog and reset all state.
    pub fn show(&mut self, label: &str) {
        self.label = label.to_string();
        self.text.clear();
        self.cursor_pos = 0;
        self.visible = true;
        self.result = TextInputResult::Pending;
        self.grid_row = 1;
        self.grid_col = 0;
        self.mode = 0;
        self.max_len = MAX_INPUT_LEN;
    }

    /// Handle a single input event. Returns the current result.
    pub fn handle_input(&mut self, event: &InputEvent) -> TextInputResult {
        if !self.visible || self.result != TextInputResult::Pending {
            return self.result.clone();
        }
        if event.action == InputAction::Release {
            return TextInputResult::Pending;
        }

        self.cursor_pos = self.cursor_boundary();
        match event.button {
            // Grid navigation
            Button::DpadUp => {
                if self.grid_row > 0 {
                    self.grid_row -= 1;
                }
            }
            Button::DpadDown => {
                if self.grid_row < ROWS - 1 {
                    self.grid_row += 1;
                }
            }
            Button::DpadLeft => {
                if self.grid_col > 0 {
                    self.grid_col -= 1;
                }
            }
            Button::DpadRight => {
                if self.grid_col < COLS - 1 {
                    self.grid_col += 1;
                }
            }

            // Type the focused character
            Button::A => {
                if self.text.chars().count() < self.max_len {
                    let layout = self.layout();
                    let ch = layout[self.grid_row][self.grid_col];
                    self.text.insert(self.cursor_pos, ch);
                    self.cursor_pos += 1;
                }
            }

            // Backspace
            Button::B => {
                if self.cursor_pos > 0 {
                    self.cursor_pos = self.previous_boundary();
                    self.text.remove(self.cursor_pos);
                } else if self.text.is_empty() {
                    // B on empty input cancels
                    self.result = TextInputResult::Cancelled;
                    self.visible = false;
                }
            }

            // Toggle shift
            Button::X => {
                self.mode = (self.mode + 1) % 3;
            }

            // Insert space
            Button::Y => {
                if self.text.chars().count() < self.max_len {
                    self.text.insert(self.cursor_pos, ' ');
                    self.cursor_pos += 1;
                }
            }

            // Move cursor left
            Button::L1 => {
                if self.cursor_pos > 0 {
                    self.cursor_pos = self.previous_boundary();
                }
            }

            // Move cursor right
            Button::R1 => {
                if self.cursor_pos < self.text.len() {
                    self.cursor_pos += self.text[self.cursor_pos..]
                        .chars()
                        .next()
                        .unwrap()
                        .len_utf8();
                }
            }

            // Submit
            Button::Start => {
                self.result = TextInputResult::Submitted(self.text.clone());
                self.visible = false;
            }

            // Cancel
            Button::Select => {
                self.result = TextInputResult::Cancelled;
                self.visible = false;
            }

            _ => {}
        }

        self.result.clone()
    }

    fn layout(&self) -> &[[char; COLS]; ROWS] {
        match self.mode {
            1 => &LAYOUT_UPPER,
            2 => &LAYOUT_SYMBOLS,
            _ => &LAYOUT_LOWER,
        }
    }

    /// Set defaults without splitting a Unicode character or exceeding the limit.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.chars().take(self.max_len).collect();
        self.cursor_pos = self.text.len();
    }

    fn cursor_boundary(&self) -> usize {
        let mut pos = self.cursor_pos.min(self.text.len());
        while !self.text.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    }

    fn previous_boundary(&self) -> usize {
        self.text[..self.cursor_boundary()]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    /// Draw an opaque, static keyboard using the selected OS palette.
    pub fn draw(&self, screen: &mut Screen) {
        if !self.visible {
            return;
        }
        let t = screen.theme;
        let x = DIALOG_X;
        let y = 108;
        let width = DIALOG_W as u32;
        screen.fill(Rect::new(0, 0, WIDTH, HEIGHT), t.bg);
        screen.fill(Rect::new(x, y, width, 4), t.accent);
        screen.draw_text(
            &self.label,
            x + 14,
            y + 20,
            Some(t.text),
            22,
            true,
            Some(width - 28),
        );

        let input_x = x + 14;
        let input_y = y + 76;
        let input_w = width - 28;
        screen.fill(
            Rect::new(input_x, input_y, input_w, INPUT_FIELD_H as u32),
            t.card_bg,
        );
        screen.draw_rect(
            Rect::new(input_x, input_y, input_w, INPUT_FIELD_H as u32),
            Some(t.accent),
            false,
            0,
            None,
        );
        let cursor = self.cursor_boundary();
        let (display, cursor) = if self.masked {
            (
                "*".repeat(self.text.chars().count()),
                self.text[..cursor].chars().count(),
            )
        } else {
            (self.text.clone(), cursor)
        };
        let before = &display[..cursor];
        let max_w = input_w - (INPUT_PAD as u32 * 2) - 8;
        // Keep the cursor visible when editing long URLs/commands. Binary search
        // whole-character suffixes; measuring a long prefix never moves it offscreen.
        let boundaries: Vec<_> = before
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(before.len()))
            .collect();
        let (mut low, mut high) = (0, boundaries.len() - 1);
        while low < high {
            let mid = (low + high) / 2;
            if screen.get_text_width(&before[boundaries[mid]..], INPUT_FONT, false) <= max_w {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        let visible = &before[boundaries[low]..];
        let text_x = input_x + INPUT_PAD;
        let text_y = input_y + 11;
        let before_w = screen.get_text_width(visible, INPUT_FONT, false);
        screen.draw_text(
            visible,
            text_x,
            text_y,
            Some(t.text),
            INPUT_FONT,
            false,
            Some(max_w),
        );
        screen.fill(
            Rect::new(text_x + before_w as i32, input_y + 9, 2, 26),
            t.accent,
        );
        if max_w > before_w + 5 {
            screen.draw_text(
                &display[cursor..],
                text_x + before_w as i32 + 5,
                text_y,
                Some(t.text),
                INPUT_FONT,
                false,
                Some(max_w - before_w - 5),
            );
        }
        let count = format!("{} / {}", self.text.chars().count(), self.max_len);
        screen.draw_text(
            &count,
            input_x,
            input_y + INPUT_FIELD_H + 8,
            Some(t.text_dim),
            12,
            false,
            None,
        );

        let kb_x = x + KB_X_OFFSET;
        let kb_y = input_y + INPUT_FIELD_H + 38;
        let layout = self.layout();
        for (row, keys) in layout.iter().enumerate() {
            for (col, ch) in keys.iter().enumerate() {
                let kx = kb_x + col as i32 * (KEY_W + KEY_GAP);
                let ky = kb_y + row as i32 * (KEY_H + KEY_GAP);
                let focused = row == self.grid_row && col == self.grid_col;
                let rect = Rect::new(kx, ky, KEY_W as u32, KEY_H as u32);
                screen.fill(rect, if focused { t.accent } else { t.card_bg });
                screen.draw_rect(
                    rect,
                    Some(if focused { t.text } else { t.card_border }),
                    false,
                    0,
                    None,
                );
                let letter = ch.to_string();
                let w = screen.get_text_width(&letter, KEY_FONT, true) as i32;
                let h = screen.get_line_height(KEY_FONT, true) as i32;
                screen.draw_text(
                    &letter,
                    kx + (KEY_W - w) / 2,
                    ky + (KEY_H - h) / 2,
                    Some(t.text),
                    KEY_FONT,
                    true,
                    None,
                );
            }
        }
        let mode_y = kb_y + ROWS as i32 * (KEY_H + KEY_GAP) + 14;
        screen.draw_text(
            ["abc / NUMBERS", "ABC / PUNCTUATION", "SYMBOLS"][self.mode],
            kb_x,
            mode_y,
            Some(t.accent),
            13,
            true,
            None,
        );
        screen.draw_text(
            "A TYPE   B DELETE   X SHIFT   Y SPACE",
            kb_x,
            mode_y + 32,
            Some(t.text),
            14,
            false,
            None,
        );
        screen.draw_text(
            "L1/R1 CURSOR   START DONE   SELECT CANCEL",
            kb_x,
            mode_y + 57,
            Some(t.text_dim),
            13,
            false,
            None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn press(input: &mut TextInput, button: Button) -> TextInputResult {
        input.handle_input(&InputEvent {
            button,
            action: InputAction::Press,
        })
    }
    #[test]
    fn editing_unicode_defaults_keeps_character_boundaries() {
        let mut input = TextInput::new("Place");
        input.show("Place");
        input.set_text("São 東京");
        press(&mut input, Button::L1);
        press(&mut input, Button::B);
        assert_eq!(input.text, "São 京");
        press(&mut input, Button::R1);
        press(&mut input, Button::A);
        assert_eq!(input.text, "São 京q");
        assert_eq!(
            press(&mut input, Button::Start),
            TextInputResult::Submitted("São 京q".into())
        );
    }
    #[test]
    fn long_defaults_limits_and_cancel() {
        let mut input = TextInput::new("Stream");
        input.show("Stream");
        input.max_len = 1024;
        input.set_text(&format!("https://example.test/{}", "a".repeat(200)));
        press(&mut input, Button::A);
        assert!(input.text.len() > 200);
        assert_eq!(
            press(&mut input, Button::Select),
            TextInputResult::Cancelled
        );
        assert!(!input.visible);
        input.show("Short");
        input.max_len = 3;
        input.set_text("東京AB");
        press(&mut input, Button::A);
        assert_eq!(input.text, "東京A");
    }
}
