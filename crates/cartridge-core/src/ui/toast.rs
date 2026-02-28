//! Transient bottom-center toast notifications with auto-dismiss, colored left dot, and queue.

use sdl2::rect::Rect;

use crate::screen::{Screen, HEIGHT, WIDTH};

/// Toast severity level, determines the colored dot on the left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// A single toast message with auto-dismiss timer.
pub struct Toast {
    pub text: String,
    pub level: ToastLevel,
    pub duration: f32,
    /// Time remaining in seconds.
    pub remaining: f32,
}

impl Toast {
    pub fn new(text: &str, level: ToastLevel, duration: f32) -> Self {
        Self {
            text: text.to_string(),
            level,
            duration,
            remaining: duration,
        }
    }

    /// Returns true if this toast has expired.
    pub fn expired(&self) -> bool {
        self.remaining <= 0.0
    }

    /// Alpha for fade-out in the last 0.5 seconds.
    pub fn alpha(&self) -> f32 {
        if self.remaining > 0.5 {
            1.0
        } else {
            (self.remaining / 0.5).max(0.0)
        }
    }
}

/// Manages a queue of toast notifications.
pub struct ToastManager {
    toasts: Vec<Toast>,
    max_toasts: usize,
}

impl ToastManager {
    pub fn new(max_toasts: usize) -> Self {
        Self {
            toasts: Vec::new(),
            max_toasts,
        }
    }

    /// Push a toast with explicit level and duration.
    pub fn push(&mut self, text: &str, level: ToastLevel, duration: f32) {
        self.toasts.push(Toast::new(text, level, duration));
        if self.toasts.len() > self.max_toasts {
            self.toasts.remove(0);
        }
    }

    /// Push an info toast.
    pub fn info(&mut self, text: &str) {
        self.push(text, ToastLevel::Info, 2.5);
    }

    /// Push a success toast.
    pub fn success(&mut self, text: &str) {
        self.push(text, ToastLevel::Success, 2.5);
    }

    /// Push a warning toast.
    pub fn warn(&mut self, text: &str) {
        self.push(text, ToastLevel::Warning, 2.5);
    }

    /// Push an error toast (longer duration).
    pub fn error(&mut self, text: &str) {
        self.push(text, ToastLevel::Error, 4.0);
    }

    /// Returns true if there are active toasts.
    pub fn has_toasts(&self) -> bool {
        !self.toasts.is_empty()
    }

    /// Advance timers by `dt` seconds. Call once per frame.
    pub fn update(&mut self, dt: f32) {
        for toast in &mut self.toasts {
            toast.remaining -= dt;
        }
        self.toasts.retain(|t| !t.expired());
    }

    /// Draw active toasts at the bottom center of the screen, above the footer area.
    ///
    /// Each toast is a rounded card with a colored dot on the left indicating the
    /// severity level, and centered text.
    pub fn draw(&self, screen: &mut Screen) {
        if self.toasts.is_empty() {
            return;
        }

        let theme = screen.theme;
        let font_size: u16 = 14;
        let lh = screen.get_line_height(font_size, false) as i32;
        let toast_h = lh + 12;
        let toast_w = (WIDTH as i32) - 40;
        let base_y = HEIGHT as i32 - 46; // above footer area

        for (i, toast) in self.toasts.iter().rev().enumerate() {
            let y = base_y - (i as i32 + 1) * (toast_h + 4);
            let _alpha = toast.alpha();

            // Background card
            let bg_rect = Rect::new(20, y, toast_w as u32, toast_h as u32);
            screen.draw_rect(bg_rect, Some(theme.bg_lighter), true, 4, None);

            // Border
            screen.draw_rect(bg_rect, Some(theme.card_border), false, 4, None);

            // Colored dot on the left based on level
            let dot_color = match toast.level {
                ToastLevel::Info => theme.text_accent,
                ToastLevel::Success => theme.positive,
                ToastLevel::Warning => theme.text_warning,
                ToastLevel::Error => theme.negative,
            };
            let dot_x = 20 + 10;
            let dot_y = y + toast_h / 2;
            screen.draw_circle(dot_x, dot_y, 4, dot_color);

            // Text -- centered in the remaining space after the dot
            let text_start_x = dot_x + 12;
            let text_area_w = toast_w - (text_start_x - 20) - 10;
            let tw = screen.get_text_width(&toast.text, font_size, false) as i32;
            let tx = text_start_x + (text_area_w - tw) / 2;
            let ty = y + (toast_h - lh) / 2;
            screen.draw_text(
                &toast.text,
                tx,
                ty,
                Some(theme.text),
                font_size,
                false,
                Some(text_area_w as u32),
            );
        }
    }
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new(3)
    }
}
