//! Scrollable word-wrapped text view with D-pad scrolling.

use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::input::{Button, InputAction, InputEvent};
use crate::screen::Screen;

const PAD_X: i32 = 12;
const PAD_Y: i32 = 8;

/// Style for a wrapped line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineStyle {
    Title,
    Normal,
    Dim,
}

/// A single wrapped line with its style.
struct WrappedLine {
    text: String,
    style: LineStyle,
}

/// Scrollable text content with word wrap. D-pad Up/Down scrolls line by line,
/// L1/R1 pages.
pub struct DetailView {
    pub title: String,
    pub body: String,
    scroll: i32,
    wrapped_lines: Vec<WrappedLine>,
    needs_wrap: bool,
    last_width: i32,
}

impl DetailView {
    pub fn new(title: &str, body: &str) -> Self {
        Self {
            title: title.to_string(),
            body: body.to_string(),
            scroll: 0,
            wrapped_lines: Vec::new(),
            needs_wrap: true,
            last_width: 0,
        }
    }

    /// Replace content and reset scroll position.
    pub fn set_content(&mut self, title: &str, body: &str) {
        self.title = title.to_string();
        self.body = body.to_string();
        self.scroll = 0;
        self.needs_wrap = true;
    }

    /// Handle D-pad scroll input. Returns `true` if consumed.
    pub fn handle_input(&mut self, event: &InputEvent) -> bool {
        if event.action == InputAction::Release {
            return false;
        }
        match event.button {
            Button::DpadUp => {
                self.scroll = (self.scroll - 1).max(0);
                true
            }
            Button::DpadDown => {
                self.scroll += 1;
                true
            }
            Button::L1 => {
                self.scroll = (self.scroll - 10).max(0);
                true
            }
            Button::R1 => {
                self.scroll += 10;
                true
            }
            _ => false,
        }
    }

    /// Draw the detail view within `rect`.
    pub fn draw(&mut self, screen: &mut Screen, rect: Rect) {
        let theme = screen.theme;

        screen.draw_rect(rect, Some(theme.bg), true, 0, None);

        let content_width = rect.width() as i32 - PAD_X * 2 - 8; // scrollbar margin

        // Re-wrap if needed
        if self.needs_wrap || content_width != self.last_width {
            self.wrap_text(screen, content_width);
            self.last_width = content_width;
            self.needs_wrap = false;
        }

        let lh = screen.get_line_height(14, false) as i32;
        let lh_bold = screen.get_line_height(16, true) as i32;

        // Visible lines
        let visible_lines = ((rect.height() as i32 - PAD_Y * 2) / lh).max(1);
        let total_lines = self.wrapped_lines.len() as i32;
        let max_scroll = (total_lines - visible_lines).max(0);
        self.scroll = self.scroll.min(max_scroll);

        let mut y = rect.y() + PAD_Y;
        let end = (self.scroll + visible_lines).min(total_lines);
        for i in self.scroll..end {
            let line = &self.wrapped_lines[i as usize];
            match line.style {
                LineStyle::Title => {
                    screen.draw_text(
                        &line.text,
                        rect.x() + PAD_X,
                        y,
                        Some(theme.text),
                        16,
                        true,
                        None,
                    );
                    y += lh_bold;
                }
                LineStyle::Dim => {
                    screen.draw_text(
                        &line.text,
                        rect.x() + PAD_X,
                        y,
                        Some(theme.text_dim),
                        14,
                        false,
                        None,
                    );
                    y += lh;
                }
                LineStyle::Normal => {
                    screen.draw_text(
                        &line.text,
                        rect.x() + PAD_X,
                        y,
                        Some(theme.text),
                        14,
                        false,
                        None,
                    );
                    y += lh;
                }
            }
        }

        // Scroll indicator
        if total_lines > visible_lines {
            let ind_x = rect.x() + rect.width() as i32 - 5;
            let bar_top = rect.y() + PAD_Y;
            let bar_h = rect.height() as i32 - PAD_Y * 2;
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
            let thumb_h = ((bar_h * visible_lines) / total_lines).max(8);
            let progress = if max_scroll > 0 {
                self.scroll as f32 / max_scroll as f32
            } else {
                0.0
            };
            let thumb_y = bar_top + ((bar_h - thumb_h) as f32 * progress) as i32;
            screen.draw_rect(
                Rect::new(ind_x - 1, thumb_y, 3, thumb_h as u32),
                Some(theme.text_dim),
                true,
                0,
                None,
            );
        }
    }

    fn wrap_text(&mut self, screen: &mut Screen, max_width: i32) {
        self.wrapped_lines.clear();

        // Title lines
        if !self.title.is_empty() {
            let title_lines = word_wrap(&self.title, screen, 16, true, max_width);
            for tl in title_lines {
                self.wrapped_lines.push(WrappedLine {
                    text: tl,
                    style: LineStyle::Title,
                });
            }
            // Spacer
            self.wrapped_lines.push(WrappedLine {
                text: String::new(),
                style: LineStyle::Normal,
            });
        }

        // Body lines -- split on newlines for paragraphs.
        // Lines prefixed with "~" are rendered in dim style (secondary text).
        for paragraph in self.body.split('\n') {
            if paragraph.trim().is_empty() {
                self.wrapped_lines.push(WrappedLine {
                    text: String::new(),
                    style: LineStyle::Normal,
                });
                continue;
            }
            // Check for dim prefix
            let (text, style) = if paragraph.starts_with('~') && paragraph.ends_with('~') && paragraph.len() > 2 {
                (&paragraph[1..paragraph.len() - 1], LineStyle::Dim)
            } else {
                (paragraph, LineStyle::Normal)
            };
            let lines = word_wrap(text, screen, 14, false, max_width);
            for line in lines {
                self.wrapped_lines.push(WrappedLine {
                    text: line,
                    style,
                });
            }
        }
    }
}

/// Word-wrap text to fit within `max_width` pixels.
fn word_wrap(
    text: &str,
    screen: &mut Screen,
    font_size: u16,
    bold: bool,
    max_width: i32,
) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }

    let words: Vec<&str> = text.split(' ').collect();
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in &words {
        let test = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        let tw = screen.get_text_width(&test, font_size, bold) as i32;
        if tw <= max_width {
            current = test;
        } else {
            if !current.is_empty() {
                lines.push(current);
            }
            // Handle long words that don't fit
            let word_w = screen.get_text_width(word, font_size, bold) as i32;
            if word_w > max_width {
                let mut remaining = word.to_string();
                while !remaining.is_empty() {
                    let mut chunk = remaining.clone();
                    while screen.get_text_width(&chunk, font_size, bold) as i32 > max_width
                        && chunk.len() > 1
                    {
                        chunk.pop();
                    }
                    let chunk_len = chunk.len();
                    lines.push(chunk);
                    remaining = remaining[chunk_len..].to_string();
                }
                current = String::new();
            } else {
                current = word.to_string();
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}
