//! Modal yes/no confirmation dialog -- centered, overlay, two buttons, A confirm, B cancel.

use sdl2::rect::Rect;

use crate::input::{Button, InputAction, InputEvent};
use crate::screen::{Screen, HEIGHT, WIDTH};

/// Result of the confirm dialog interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogResult {
    /// Dialog is still active, no decision yet.
    Pending,
    /// User confirmed (pressed A on confirm button).
    Confirmed,
    /// User cancelled (pressed B, or pressed A on cancel button).
    Cancelled,
}

/// Centered modal confirmation dialog with semi-transparent overlay, title, body,
/// and two buttons (Cancel / Confirm).
///
/// D-left/D-right switches between buttons. A confirms the focused button's action.
/// B always cancels regardless of which button is focused.
pub struct ConfirmDialog {
    pub title: String,
    pub body: String,
    pub confirm_label: String,
    pub cancel_label: String,
    /// If true, the confirm button is styled with the `negative` color (for destructive actions).
    pub destructive: bool,
    /// Which button is focused: 0 = cancel, 1 = confirm.
    pub focused_button: usize,
    pub visible: bool,
    pub result: DialogResult,
}

impl ConfirmDialog {
    pub fn new(title: &str, body: &str) -> Self {
        Self {
            title: title.to_string(),
            body: body.to_string(),
            confirm_label: "Confirm".to_string(),
            cancel_label: "Cancel".to_string(),
            destructive: false,
            focused_button: 0, // cancel focused by default (safer)
            visible: false,
            result: DialogResult::Pending,
        }
    }

    /// Set custom button labels.
    pub fn with_labels(mut self, cancel: &str, confirm: &str) -> Self {
        self.cancel_label = cancel.to_string();
        self.confirm_label = confirm.to_string();
        self
    }

    /// Mark as destructive (confirm button highlighted in negative/red).
    pub fn with_destructive(mut self, destructive: bool) -> Self {
        self.destructive = destructive;
        self
    }

    /// Show the dialog and reset state.
    pub fn show(&mut self) {
        self.visible = true;
        self.result = DialogResult::Pending;
        self.focused_button = 0; // default to cancel
    }

    /// Handle input. Returns the current `DialogResult`.
    pub fn handle_input(&mut self, event: &InputEvent) -> DialogResult {
        if !self.visible || self.result != DialogResult::Pending {
            return self.result;
        }
        if event.action == InputAction::Release {
            return DialogResult::Pending;
        }

        match event.button {
            Button::DpadLeft => {
                self.focused_button = 0;
            }
            Button::DpadRight => {
                self.focused_button = 1;
            }
            Button::B => {
                // B always cancels
                self.result = DialogResult::Cancelled;
                self.visible = false;
            }
            Button::A => {
                if self.focused_button == 0 {
                    self.result = DialogResult::Cancelled;
                } else {
                    self.result = DialogResult::Confirmed;
                }
                self.visible = false;
            }
            _ => {}
        }
        self.result
    }

    /// Draw the dialog overlay on the full screen. Does nothing if not visible.
    pub fn draw(&self, screen: &mut Screen) {
        if !self.visible {
            return;
        }

        let theme = screen.theme;

        // Dark overlay covering the full screen
        let overlay_rect = Rect::new(0, 0, WIDTH, HEIGHT);
        screen.draw_rect(overlay_rect, Some(theme.bg), true, 0, None);

        // Dialog box -- centered, max 400x200
        let dialog_w: u32 = 400;
        let dialog_h: u32 = 200;
        let dx = (WIDTH as i32 - dialog_w as i32) / 2;
        let dy = (HEIGHT as i32 - dialog_h as i32) / 2;
        let dialog_rect = Rect::new(dx, dy, dialog_w, dialog_h);

        // Dialog card with shadow
        screen.draw_card(dialog_rect, Some(theme.card_bg), Some(theme.card_border), 8, true);

        // Title (bold, 16px)
        let title_y = dy + 16;
        screen.draw_text(
            &self.title,
            dx + 20,
            title_y,
            Some(theme.text),
            16,
            true,
            Some(dialog_w - 40),
        );

        // Body (normal, 14px)
        let body_y = title_y + screen.get_line_height(16, true) as i32 + 8;
        screen.draw_text(
            &self.body,
            dx + 20,
            body_y,
            Some(theme.text_dim),
            14,
            false,
            Some(dialog_w - 40),
        );

        // Buttons at the bottom
        let btn_h: u32 = 32;
        let btn_gap: i32 = 16;
        let btn_y = dy + dialog_h as i32 - btn_h as i32 - 20;

        // Measure button widths
        let cancel_tw = screen.get_text_width(&self.cancel_label, 14, true);
        let confirm_tw = screen.get_text_width(&self.confirm_label, 14, true);
        let cancel_w = cancel_tw + 24;
        let confirm_w = confirm_tw + 24;
        let total_btn_w = cancel_w + btn_gap as u32 + confirm_w;
        let btn_start_x = dx + (dialog_w as i32 - total_btn_w as i32) / 2;

        // Cancel button
        let cancel_rect = Rect::new(btn_start_x, btn_y, cancel_w, btn_h);
        let cancel_focused = self.focused_button == 0;
        let cancel_bg = if cancel_focused {
            theme.card_highlight
        } else {
            theme.card_bg
        };
        let cancel_border = if cancel_focused {
            theme.accent
        } else {
            theme.card_border
        };
        screen.draw_rounded_rect(cancel_rect, cancel_bg, 4, false);
        screen.draw_rect(cancel_rect, Some(cancel_border), false, 4, None);
        let cancel_text_x = btn_start_x + (cancel_w as i32 - cancel_tw as i32) / 2;
        let cancel_text_y = btn_y + (btn_h as i32 - screen.get_line_height(14, true) as i32) / 2;
        let cancel_color = if cancel_focused {
            theme.text
        } else {
            theme.text_dim
        };
        screen.draw_text(
            &self.cancel_label,
            cancel_text_x,
            cancel_text_y,
            Some(cancel_color),
            14,
            true,
            None,
        );

        // Confirm button
        let confirm_x = btn_start_x + cancel_w as i32 + btn_gap;
        let confirm_rect = Rect::new(confirm_x, btn_y, confirm_w, btn_h);
        let confirm_focused = self.focused_button == 1;
        let confirm_bg = if confirm_focused {
            if self.destructive {
                theme.negative
            } else {
                theme.accent
            }
        } else {
            theme.card_bg
        };
        let confirm_border = if confirm_focused {
            if self.destructive {
                theme.negative
            } else {
                theme.accent
            }
        } else {
            theme.card_border
        };
        screen.draw_rounded_rect(confirm_rect, confirm_bg, 4, false);
        screen.draw_rect(confirm_rect, Some(confirm_border), false, 4, None);
        let confirm_text_x = confirm_x + (confirm_w as i32 - confirm_tw as i32) / 2;
        let confirm_text_y =
            btn_y + (btn_h as i32 - screen.get_line_height(14, true) as i32) / 2;
        let confirm_text_color = if confirm_focused {
            // Dark text on bright bg for focused confirm
            sdl2::pixels::Color::RGB(20, 20, 30)
        } else {
            theme.text_dim
        };
        screen.draw_text(
            &self.confirm_label,
            confirm_text_x,
            confirm_text_y,
            Some(confirm_text_color),
            14,
            true,
            None,
        );
    }
}
