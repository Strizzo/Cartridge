use sdl2::gfx::primitives::DrawRenderer;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator, TextureQuery};
use sdl2::video::{Window, WindowContext};

use crate::font::{FontCache, FontStyle};
use crate::image_cache::ImageCache;
use crate::text_cache::TextCache;
use crate::theme::Theme;

pub const WIDTH: u32 = 720;
pub const HEIGHT: u32 = 720;

/// High-level drawing surface for Cartridge apps.
pub struct Screen<'a> {
    pub canvas: &'a mut Canvas<Window>,
    pub theme: &'a Theme,
    pub fonts: &'a mut FontCache,
    pub images: &'a mut ImageCache,
    pub text_cache: &'a mut TextCache,
    pub texture_creator: &'a TextureCreator<WindowContext>,
}

impl<'a> Screen<'a> {
    pub fn clear(&mut self, color: Option<Color>) {
        self.canvas.set_draw_color(color.unwrap_or(self.theme.bg));
        self.canvas.clear();
    }

    /// Render text. Returns rendered width.
    /// Uses the text cache to avoid re-rasterizing the same string every frame.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &mut self,
        text: &str,
        x: i32,
        y: i32,
        color: Option<Color>,
        font_size: u16,
        bold: bool,
        max_width: Option<u32>,
    ) -> u32 {
        if text.is_empty() {
            return 0;
        }

        let color = color.unwrap_or(self.theme.text);

        // Truncate to max_width if needed. We use the cache's measure() which
        // checks any cached entry first to avoid font metric lookups.
        let display_text = if let Some(max_w) = max_width {
            let full_w = self.text_cache.measure(self.fonts, text, font_size, bold);
            if full_w > max_w {
                let mut s = text.to_string();
                while !s.is_empty() {
                    let candidate = format!("{s}..");
                    let w = self.text_cache.measure(self.fonts, &candidate, font_size, bold);
                    if w <= max_w {
                        s = candidate;
                        break;
                    }
                    s.pop();
                }
                if s.is_empty() {
                    s = "..".to_string();
                }
                s
            } else {
                // No truncation needed; avoid the allocation.
                return self.text_cache.render(
                    self.canvas, self.fonts, text, x, y, color, font_size, bold,
                );
            }
        } else {
            return self.text_cache.render(
                self.canvas, self.fonts, text, x, y, color, font_size, bold,
            );
        };

        self.text_cache.render(
            self.canvas, self.fonts, &display_text, x, y, color, font_size, bold,
        )
    }

    pub fn draw_rect(
        &mut self,
        rect: Rect,
        color: Option<Color>,
        filled: bool,
        border_radius: i16,
        line_width: Option<u32>,
    ) {
        let color = color.unwrap_or(self.theme.border);

        if border_radius > 0 {
            let (x1, y1) = (rect.x() as i16, rect.y() as i16);
            let (x2, y2) = (
                (rect.x() + rect.width() as i32 - 1) as i16,
                (rect.y() + rect.height() as i32 - 1) as i16,
            );
            if filled && line_width.is_none() {
                self.canvas
                    .rounded_box(x1, y1, x2, y2, border_radius, color)
                    .ok();
            } else {
                self.canvas
                    .rounded_rectangle(x1, y1, x2, y2, border_radius, color)
                    .ok();
            }
        } else if filled && line_width.is_none() {
            self.canvas.set_draw_color(color);
            self.canvas.fill_rect(rect).ok();
        } else {
            self.canvas.set_draw_color(color);
            self.canvas.draw_rect(rect).ok();
        }
    }

    pub fn draw_line(
        &mut self,
        start: (i32, i32),
        end: (i32, i32),
        color: Option<Color>,
        _width: u32,
    ) {
        let color = color.unwrap_or(self.theme.border);
        self.canvas.set_draw_color(color);
        self.canvas
            .draw_line(
                sdl2::rect::Point::new(start.0, start.1),
                sdl2::rect::Point::new(end.0, end.1),
            )
            .ok();
    }

    pub fn draw_rounded_rect(
        &mut self,
        rect: Rect,
        color: Color,
        radius: i16,
        shadow: bool,
    ) {
        let (x1, y1) = (rect.x() as i16, rect.y() as i16);
        let (x2, y2) = (
            (rect.x() + rect.width() as i32 - 1) as i16,
            (rect.y() + rect.height() as i32 - 1) as i16,
        );

        if shadow {
            let off = self.theme.shadow_offset as i16;
            self.canvas
                .rounded_box(x1 + off, y1 + off, x2 + off, y2 + off, radius, self.theme.shadow)
                .ok();
        }

        self.canvas
            .rounded_box(x1, y1, x2, y2, radius, color)
            .ok();
    }

    pub fn draw_card(
        &mut self,
        rect: Rect,
        bg: Option<Color>,
        border: Option<Color>,
        radius: i16,
        shadow: bool,
    ) {
        let bg = bg.unwrap_or(self.theme.card_bg);
        let border = border.unwrap_or(self.theme.card_border);

        let (x1, y1) = (rect.x() as i16, rect.y() as i16);
        let (x2, y2) = (
            (rect.x() + rect.width() as i32 - 1) as i16,
            (rect.y() + rect.height() as i32 - 1) as i16,
        );

        if shadow {
            let off = self.theme.shadow_offset as i16;
            self.canvas
                .rounded_box(x1 + off, y1 + off, x2 + off, y2 + off, radius, self.theme.shadow)
                .ok();
        }

        self.canvas.rounded_box(x1, y1, x2, y2, radius, bg).ok();
        self.canvas
            .rounded_rectangle(x1, y1, x2, y2, radius, border)
            .ok();
    }

    pub fn draw_gradient_rect(
        &mut self,
        rect: Rect,
        color_top: Color,
        color_bottom: Color,
    ) {
        let h = rect.height() as i32;
        if h <= 0 {
            return;
        }

        for y_off in 0..h {
            let t = y_off as f32 / (h - 1).max(1) as f32;
            let r = (color_top.r as f32 + (color_bottom.r as f32 - color_top.r as f32) * t) as u8;
            let g = (color_top.g as f32 + (color_bottom.g as f32 - color_top.g as f32) * t) as u8;
            let b = (color_top.b as f32 + (color_bottom.b as f32 - color_top.b as f32) * t) as u8;

            self.canvas.set_draw_color(Color::RGB(r, g, b));
            self.canvas
                .draw_line(
                    sdl2::rect::Point::new(rect.x(), rect.y() + y_off),
                    sdl2::rect::Point::new(rect.x() + rect.width() as i32 - 1, rect.y() + y_off),
                )
                .ok();
        }
    }

    /// Draw a rounded pill/badge with text. Returns total width.
    pub fn draw_pill(
        &mut self,
        text: &str,
        x: i32,
        y: i32,
        bg_color: Color,
        text_color: Color,
        font_size: u16,
    ) -> u32 {
        let style = FontStyle::MonoBold;
        let font = self.fonts.get(style, font_size);
        let (text_w, text_h) = font.size_of(text).unwrap_or((0, 0));

        let pill_w = text_w + 12;
        let pill_h = text_h + 4;
        let radius = (pill_h / 2) as i16;

        let x1 = x as i16;
        let y1 = y as i16;
        let x2 = (x + pill_w as i32 - 1) as i16;
        let y2 = (y + pill_h as i32 - 1) as i16;

        self.canvas
            .rounded_box(x1, y1, x2, y2, radius, bg_color)
            .ok();

        self.draw_text(text, x + 6, y + 2, Some(text_color), font_size, true, None);

        pill_w
    }

    /// Draw a styled button hint like [A] Open. Returns total width.
    pub fn draw_button_hint(
        &mut self,
        label: &str,
        action: &str,
        x: i32,
        y: i32,
        btn_color: Option<Color>,
        font_size: u16,
    ) -> u32 {
        let btn_color = btn_color.unwrap_or(self.theme.accent);
        let dark_text = Color::RGB(20, 20, 30);

        // Button badge
        let font = self.fonts.get(FontStyle::MonoBold, font_size);
        let (label_w, label_h) = font.size_of(label).unwrap_or((0, 0));

        let badge_w = label_w + 10;
        let badge_h = label_h + 4;

        let x1 = x as i16;
        let y1 = y as i16;
        let x2 = (x + badge_w as i32 - 1) as i16;
        let y2 = (y + badge_h as i32 - 1) as i16;

        self.canvas
            .rounded_box(x1, y1, x2, y2, 4, btn_color)
            .ok();

        self.draw_text(label, x + 5, y + 2, Some(dark_text), font_size, true, None);

        // Action text
        let font = self.fonts.get(FontStyle::Mono, font_size);
        let (action_w, _) = font.size_of(action).unwrap_or((0, 0));

        self.draw_text(
            action,
            x + badge_w as i32 + 5,
            y + 2,
            Some(self.theme.text_dim),
            font_size,
            false,
            None,
        );

        badge_w + 5 + action_w
    }

    pub fn draw_progress_bar(
        &mut self,
        rect: Rect,
        progress: f32,
        fill_color: Option<Color>,
        bg_color: Option<Color>,
        radius: i16,
    ) {
        let bg_color = bg_color.unwrap_or(self.theme.bg_lighter);
        let fill_color = fill_color.unwrap_or(self.theme.accent);

        let (x1, y1) = (rect.x() as i16, rect.y() as i16);
        let (x2, y2) = (
            (rect.x() + rect.width() as i32 - 1) as i16,
            (rect.y() + rect.height() as i32 - 1) as i16,
        );

        // Background track
        self.canvas
            .rounded_box(x1, y1, x2, y2, radius, bg_color)
            .ok();

        // Fill
        let fill_w = (rect.width() as f32 * progress.clamp(0.0, 1.0)) as i32;
        if fill_w > 0 {
            let fx2 = (rect.x() + fill_w - 1) as i16;
            self.canvas
                .rounded_box(x1, y1, fx2.max(x1), y2, radius, fill_color)
                .ok();
        }
    }

    pub fn draw_sparkline(
        &mut self,
        data: &[f32],
        rect: Rect,
        color: Option<Color>,
        baseline_color: Option<Color>,
    ) {
        if data.len() < 2 || rect.width() < 4 || rect.height() < 4 {
            return;
        }

        let color = color.unwrap_or(self.theme.accent);
        let mn = data.iter().copied().fold(f32::INFINITY, f32::min);
        let mx = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let rng = if (mx - mn).abs() > f32::EPSILON {
            mx - mn
        } else {
            1.0
        };

        let points: Vec<(i32, i32)> = data
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let px =
                    rect.x() + (i as f32 / (data.len() - 1) as f32 * (rect.width() - 1) as f32) as i32;
                let py = rect.y() + rect.height() as i32 - 1
                    - (((v - mn) / rng) * (rect.height() - 1) as f32) as i32;
                (px, py)
            })
            .collect();

        if let Some(bl_color) = baseline_color {
            let mid_y = rect.y() + rect.height() as i32 / 2;
            self.canvas.set_draw_color(bl_color);
            self.canvas
                .draw_line(
                    sdl2::rect::Point::new(rect.x(), mid_y),
                    sdl2::rect::Point::new(rect.x() + rect.width() as i32 - 1, mid_y),
                )
                .ok();
        }

        for window in points.windows(2) {
            let (x1, y1) = window[0];
            let (x2, y2) = window[1];
            self.canvas.set_draw_color(color);
            self.canvas
                .draw_line(
                    sdl2::rect::Point::new(x1, y1),
                    sdl2::rect::Point::new(x2, y2),
                )
                .ok();
        }
    }

    /// Draw text with a 4-offset glow halo behind it.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_glow(
        &mut self,
        text: &str,
        x: i32,
        y: i32,
        color: Color,
        glow_color: Color,
        font_size: u16,
        bold: bool,
        max_width: Option<u32>,
    ) -> u32 {
        // Glow pass: draw text offset by 1px in each cardinal direction
        self.draw_text(text, x - 1, y, Some(glow_color), font_size, bold, max_width);
        self.draw_text(text, x + 1, y, Some(glow_color), font_size, bold, max_width);
        self.draw_text(text, x, y - 1, Some(glow_color), font_size, bold, max_width);
        self.draw_text(text, x, y + 1, Some(glow_color), font_size, bold, max_width);
        // Main pass on top
        self.draw_text(text, x, y, Some(color), font_size, bold, max_width)
    }

    /// Draw a horizontal glow line with fading spread above or below.
    /// `direction`: positive = glow downward, negative = glow upward.
    pub fn draw_glow_line(
        &mut self,
        y: i32,
        x_start: i32,
        x_end: i32,
        color: Color,
        spread: i32,
        direction: i32,
    ) {
        // Main line
        self.canvas.set_draw_color(color);
        self.canvas
            .draw_line(
                sdl2::rect::Point::new(x_start, y),
                sdl2::rect::Point::new(x_end, y),
            )
            .ok();

        // Spread lines with decreasing alpha
        let base_a = color.a as f32;
        let sign = if direction >= 0 { 1 } else { -1 };
        for i in 1..=spread {
            let t = i as f32 / (spread + 1) as f32;
            let alpha = (base_a * (1.0 - t)) as u8;
            if alpha == 0 {
                continue;
            }
            let c = Color::RGBA(color.r, color.g, color.b, alpha);
            self.canvas.set_draw_color(c);
            let ly = y + i * sign;
            self.canvas
                .draw_line(
                    sdl2::rect::Point::new(x_start, ly),
                    sdl2::rect::Point::new(x_end, ly),
                )
                .ok();
        }
    }

    /// Draw a multi-layer expanding glow border around a rect.
    pub fn draw_card_glow(
        &mut self,
        rect: Rect,
        glow_color: Color,
        radius: i16,
        layers: u8,
    ) {
        for i in 1..=layers {
            let expand = i as i32 * 2;
            let alpha = glow_color.a.saturating_sub(i * (glow_color.a / (layers + 1)));
            let c = Color::RGBA(glow_color.r, glow_color.g, glow_color.b, alpha);
            let glow_rect = Rect::new(
                rect.x() - expand,
                rect.y() - expand,
                rect.width() + expand as u32 * 2,
                rect.height() + expand as u32 * 2,
            );
            let (x1, y1) = (glow_rect.x() as i16, glow_rect.y() as i16);
            let (x2, y2) = (
                (glow_rect.x() + glow_rect.width() as i32 - 1) as i16,
                (glow_rect.y() + glow_rect.height() as i32 - 1) as i16,
            );
            self.canvas.rounded_rectangle(x1, y1, x2, y2, radius + i as i16, c).ok();
        }
    }

    /// Draw HUD-style L-bracket corner markers at the screen corners.
    pub fn draw_corner_markers(&mut self, color: Color, size: i32, inset: i32) {
        let w = WIDTH as i32;
        let h = HEIGHT as i32;
        self.canvas.set_draw_color(color);

        // Top-left
        self.canvas.draw_line(sdl2::rect::Point::new(inset, inset), sdl2::rect::Point::new(inset + size, inset)).ok();
        self.canvas.draw_line(sdl2::rect::Point::new(inset, inset), sdl2::rect::Point::new(inset, inset + size)).ok();

        // Top-right
        self.canvas.draw_line(sdl2::rect::Point::new(w - inset - size, inset), sdl2::rect::Point::new(w - inset, inset)).ok();
        self.canvas.draw_line(sdl2::rect::Point::new(w - inset, inset), sdl2::rect::Point::new(w - inset, inset + size)).ok();

        // Bottom-left
        self.canvas.draw_line(sdl2::rect::Point::new(inset, h - inset), sdl2::rect::Point::new(inset + size, h - inset)).ok();
        self.canvas.draw_line(sdl2::rect::Point::new(inset, h - inset - size), sdl2::rect::Point::new(inset, h - inset)).ok();

        // Bottom-right
        self.canvas.draw_line(sdl2::rect::Point::new(w - inset - size, h - inset), sdl2::rect::Point::new(w - inset, h - inset)).ok();
        self.canvas.draw_line(sdl2::rect::Point::new(w - inset, h - inset - size), sdl2::rect::Point::new(w - inset, h - inset)).ok();
    }

    pub fn get_text_width(&mut self, text: &str, font_size: u16, bold: bool) -> u32 {
        // Hit the text cache first; it has cached width if the text was drawn.
        self.text_cache.measure(self.fonts, text, font_size, bold)
    }

    pub fn get_line_height(&mut self, font_size: u16, bold: bool) -> u32 {
        let style = if bold {
            FontStyle::MonoBold
        } else {
            FontStyle::Mono
        };
        let font = self.fonts.get(style, font_size);
        font.height() as u32
    }

    /// Draw a filled circle (used for WiFi indicator dot, etc.)
    pub fn draw_circle(&mut self, cx: i32, cy: i32, radius: i16, color: Color) {
        self.canvas
            .filled_circle(cx as i16, cy as i16, radius, color)
            .ok();
    }

    /// Draw an image from the cache. If `dst_size` is Some, scales to that size.
    /// If `src_rect` is Some, clips from the source image (sprite sheet).
    /// Silently logs a warning and returns false if the image is not found.
    pub fn draw_image(
        &mut self,
        path: &str,
        x: i32,
        y: i32,
        dst_size: Option<(u32, u32)>,
        src_rect: Option<Rect>,
    ) -> bool {
        let texture = match self.images.get(path) {
            Some(t) => t,
            None => return false,
        };

        let query = texture.query();
        let dst = match dst_size {
            Some((w, h)) => Rect::new(x, y, w, h),
            None => Rect::new(x, y, query.width, query.height),
        };

        self.canvas.copy(texture, src_rect, dst).ok();
        true
    }
}
