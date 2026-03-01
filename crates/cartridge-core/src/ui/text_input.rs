//! On-screen QWERTY keyboard for text entry (WiFi passwords, search, etc.)
//!
//! Full-screen modal overlay with a character grid, input field with blinking cursor,
//! and mode toggle between lowercase and uppercase/symbols.
//!
//! Controls:
//! - D-pad: navigate keyboard grid
//! - A: type focused character
//! - B: delete character before cursor (backspace)
//! - X: toggle shift mode (lowercase ↔ uppercase/symbols)
//! - Y: insert space
//! - L1: move cursor left
//! - R1: move cursor right
//! - START: submit input
//! - SELECT: cancel input

use std::time::Instant;

use sdl2::pixels::Color;
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
const CURSOR_BLINK_MS: u128 = 500;

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

    grid_row: usize,
    grid_col: usize,
    shifted: bool,

    show_time: Instant,
    last_type_time: Option<Instant>,
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

            grid_row: 1,
            grid_col: 0,
            shifted: false,

            show_time: Instant::now(),
            last_type_time: None,
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
        self.shifted = false;
        self.show_time = Instant::now();
        self.last_type_time = None;
    }

    /// Handle a single input event. Returns the current result.
    pub fn handle_input(&mut self, event: &InputEvent) -> TextInputResult {
        if !self.visible || self.result != TextInputResult::Pending {
            return self.result.clone();
        }
        if event.action == InputAction::Release {
            return TextInputResult::Pending;
        }

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
                if self.text.len() < MAX_INPUT_LEN {
                    let layout = if self.shifted {
                        &LAYOUT_UPPER
                    } else {
                        &LAYOUT_LOWER
                    };
                    let ch = layout[self.grid_row][self.grid_col];
                    self.text.insert(self.cursor_pos, ch);
                    self.cursor_pos += 1;
                    self.last_type_time = Some(Instant::now());
                }
            }

            // Backspace
            Button::B => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    self.text.remove(self.cursor_pos);
                    self.last_type_time = Some(Instant::now());
                } else if self.text.is_empty() {
                    // B on empty input cancels
                    self.result = TextInputResult::Cancelled;
                    self.visible = false;
                }
            }

            // Toggle shift
            Button::X => {
                self.shifted = !self.shifted;
            }

            // Insert space
            Button::Y => {
                if self.text.len() < MAX_INPUT_LEN {
                    self.text.insert(self.cursor_pos, ' ');
                    self.cursor_pos += 1;
                    self.last_type_time = Some(Instant::now());
                }
            }

            // Move cursor left
            Button::L1 => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            }

            // Move cursor right
            Button::R1 => {
                if self.cursor_pos < self.text.len() {
                    self.cursor_pos += 1;
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

    /// Draw the text input overlay. Does nothing if not visible.
    pub fn draw(&self, screen: &mut Screen) {
        if !self.visible {
            return;
        }

        let theme = screen.theme;
        let now_ms = Instant::now().duration_since(self.show_time).as_millis();
        let cursor_visible = (now_ms / CURSOR_BLINK_MS) % 2 == 0;

        // -- Full-screen dark overlay --
        screen.draw_rect(
            Rect::new(0, 0, WIDTH, HEIGHT),
            Some(Color::RGBA(12, 12, 18, 240)),
            true,
            0,
            None,
        );

        // -- Dialog card --
        let dialog_h: i32 = 470;
        let dialog_y = (HEIGHT as i32 - dialog_h) / 2;
        let dialog_rect = Rect::new(DIALOG_X, dialog_y, DIALOG_W as u32, dialog_h as u32);

        // Subtle glow behind the card
        screen.draw_card_glow(
            dialog_rect,
            Color::RGBA(100, 180, 255, 25),
            10,
            3,
        );

        screen.draw_card(
            dialog_rect,
            Some(Color::RGB(22, 22, 34)),
            Some(theme.card_border),
            10,
            true,
        );

        // Accent line at top of dialog
        screen.draw_glow_line(
            dialog_y,
            DIALOG_X + 10,
            DIALOG_X + DIALOG_W - 10,
            Color::RGBA(100, 180, 255, 80),
            3,
            1,
        );

        // -- Label with glow --
        let label_y = dialog_y + 20;
        screen.draw_text_glow(
            &self.label,
            DIALOG_X + 24,
            label_y,
            theme.accent,
            theme.glow_primary,
            18,
            true,
            Some(DIALOG_W as u32 - 48),
        );

        // -- Input field --
        let input_y = label_y + 38;
        let input_x = DIALOG_X + 20;
        let input_w = (DIALOG_W - 40) as u32;
        let input_rect = Rect::new(input_x, input_y, input_w, INPUT_FIELD_H as u32);

        // Input field background (recessed look)
        screen.draw_card(
            input_rect,
            Some(Color::RGB(14, 14, 22)),
            Some(theme.accent),
            6,
            false,
        );

        // Display text (masked or plain)
        let display_text: String = if self.masked {
            "*".repeat(self.text.len())
        } else {
            self.text.clone()
        };

        let text_x = input_x + INPUT_PAD;
        let text_y = input_y + (INPUT_FIELD_H - 20) / 2;

        // Calculate visible portion if text is too long
        let max_text_w = input_w as i32 - INPUT_PAD * 2 - 12; // leave room for cursor

        // Draw text before cursor
        let before_cursor = &display_text[..self.cursor_pos.min(display_text.len())];
        if !before_cursor.is_empty() {
            screen.draw_text(
                before_cursor,
                text_x,
                text_y,
                Some(theme.text),
                INPUT_FONT,
                false,
                Some(max_text_w as u32),
            );
        }

        // Draw cursor
        if cursor_visible {
            let cursor_text_w = if before_cursor.is_empty() {
                0
            } else {
                screen.get_text_width(before_cursor, INPUT_FONT, false) as i32
            };
            let cursor_x = text_x + cursor_text_w;

            // Blinking cursor line
            screen.draw_rect(
                Rect::new(cursor_x, input_y + 8, 2, INPUT_FIELD_H as u32 - 16),
                Some(theme.accent),
                true,
                0,
                None,
            );
        }

        // Draw text after cursor
        let after_cursor = &display_text[self.cursor_pos.min(display_text.len())..];
        if !after_cursor.is_empty() {
            let before_w = if before_cursor.is_empty() {
                0
            } else {
                screen.get_text_width(before_cursor, INPUT_FONT, false) as i32
            };
            let after_x = text_x + before_w + 4; // small gap after cursor
            screen.draw_text(
                after_cursor,
                after_x,
                text_y,
                Some(theme.text),
                INPUT_FONT,
                false,
                Some((max_text_w - before_w - 4).max(0) as u32),
            );
        }

        // Character count
        let count_text = format!("{}/{}", self.text.len(), MAX_INPUT_LEN);
        let count_w = screen.get_text_width(&count_text, 11, false) as i32;
        screen.draw_text(
            &count_text,
            input_x + input_w as i32 - count_w - 8,
            input_y - 16,
            Some(theme.text_dim),
            11,
            false,
            None,
        );

        // -- Keyboard grid --
        let kb_y = input_y + INPUT_FIELD_H + 20;
        let kb_x = DIALOG_X + KB_X_OFFSET;
        let layout = if self.shifted {
            &LAYOUT_UPPER
        } else {
            &LAYOUT_LOWER
        };

        for row in 0..ROWS {
            for col in 0..COLS {
                let kx = kb_x + col as i32 * (KEY_W + KEY_GAP);
                let ky = kb_y + row as i32 * (KEY_H + KEY_GAP);
                let key_rect = Rect::new(kx, ky, KEY_W as u32, KEY_H as u32);

                let is_focused = row == self.grid_row && col == self.grid_col;

                let (bg, border, text_color) = if is_focused {
                    (theme.card_highlight, theme.accent, theme.text)
                } else {
                    (
                        Color::RGB(32, 32, 48),
                        Color::RGB(50, 50, 68),
                        theme.text_dim,
                    )
                };

                // Focused key gets a glow
                if is_focused {
                    screen.draw_card_glow(
                        key_rect,
                        Color::RGBA(100, 180, 255, 40),
                        4,
                        2,
                    );
                }

                // Key background + border
                screen.draw_card(key_rect, Some(bg), Some(border), 4, false);

                // Key character, centered
                let ch = layout[row][col];
                let ch_str = ch.to_string();
                let ch_w = screen.get_text_width(&ch_str, KEY_FONT, true) as i32;
                let ch_h = screen.get_line_height(KEY_FONT, true) as i32;
                let ch_x = kx + (KEY_W - ch_w) / 2;
                let ch_y = ky + (KEY_H - ch_h) / 2;

                screen.draw_text(
                    &ch_str,
                    ch_x,
                    ch_y,
                    Some(text_color),
                    KEY_FONT,
                    true,
                    None,
                );
            }
        }

        // -- Mode indicator --
        let mode_y = kb_y + ROWS as i32 * (KEY_H + KEY_GAP) + 8;
        let mode_text = if self.shifted { "ABC !@#" } else { "abc 123" };
        let mode_pill_bg = if self.shifted {
            Color::RGB(167, 139, 250) // purple accent for shift mode
        } else {
            Color::RGB(60, 60, 80)
        };
        screen.draw_pill(mode_text, kb_x, mode_y, mode_pill_bg, theme.text, 11);

        // "Last typed" feedback -- briefly show the character that was just typed
        if let Some(last_time) = self.last_type_time {
            let elapsed = Instant::now().duration_since(last_time).as_millis();
            if elapsed < 600 && !self.text.is_empty() && self.cursor_pos > 0 {
                let last_ch = if self.masked {
                    '*'
                } else {
                    self.text.chars().nth(self.cursor_pos - 1).unwrap_or(' ')
                };
                let feedback = last_ch.to_string();
                let fw = screen.get_text_width(&feedback, 24, true) as i32;
                // Fade based on elapsed time
                let alpha = (255.0 * (1.0 - elapsed as f32 / 600.0)) as u8;
                let feedback_color = Color::RGBA(100, 180, 255, alpha);
                screen.draw_text(
                    &feedback,
                    DIALOG_X + DIALOG_W - 60 - fw / 2,
                    mode_y - 4,
                    Some(feedback_color),
                    24,
                    true,
                    None,
                );
            }
        }

        // -- Button hints --
        let hints_y = mode_y + 28;
        let mut hx = kb_x;

        let w = screen.draw_button_hint("A", "Type", hx, hints_y, Some(theme.btn_a), 11);
        hx += w as i32 + 10;
        let w = screen.draw_button_hint("B", "Delete", hx, hints_y, Some(theme.btn_b), 11);
        hx += w as i32 + 10;
        let w = screen.draw_button_hint("X", "Shift", hx, hints_y, Some(theme.btn_x), 11);
        hx += w as i32 + 10;
        screen.draw_button_hint("Y", "Space", hx, hints_y, Some(theme.btn_y), 11);

        let hints2_y = hints_y + 22;
        hx = kb_x;
        let w = screen.draw_button_hint("L1", "\u{2190}", hx, hints2_y, Some(theme.btn_l), 11);
        hx += w as i32 + 10;
        let w = screen.draw_button_hint("R1", "\u{2192}", hx, hints2_y, Some(theme.btn_l), 11);
        hx += w as i32 + 16;
        let w = screen.draw_button_hint("START", "Done", hx, hints2_y, Some(theme.positive), 11);
        hx += w as i32 + 10;
        screen.draw_button_hint("SEL", "Cancel", hx, hints2_y, Some(theme.btn_b), 11);
    }

}
