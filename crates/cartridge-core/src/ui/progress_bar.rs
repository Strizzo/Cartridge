//! Labeled progress bar -- label + percentage + bar.

use sdl2::rect::Rect;

use crate::screen::Screen;

/// Labeled progress bar widget.
///
/// Displays a text label above the bar, a percentage on the right, and a filled
/// progress bar using accent color for fill and `bg_lighter` for the track.
///
/// Total height: 12px bar + 16px label area = ~28px.
pub struct ProgressBar {
    pub label: String,
    /// Progress value from 0.0 to 1.0.
    pub progress: f32,
    /// Show percentage text on the right. Defaults to true.
    pub show_percentage: bool,
}

impl ProgressBar {
    pub fn new(label: &str, progress: f32) -> Self {
        Self {
            label: label.to_string(),
            progress: progress.clamp(0.0, 1.0),
            show_percentage: true,
        }
    }

    /// Update the progress value (clamped to 0.0 - 1.0).
    pub fn set_progress(&mut self, progress: f32) {
        self.progress = progress.clamp(0.0, 1.0);
    }

    /// Draw the progress bar within `rect`.
    ///
    /// The layout is:
    /// - Top row: label (left) and percentage (right), 16px tall
    /// - Bottom row: progress bar track and fill, 12px tall
    pub fn draw(&self, screen: &mut Screen, rect: Rect) {
        let theme = screen.theme;
        let font_size: u16 = 13;

        let label_h = screen.get_line_height(font_size, false) as i32;
        let bar_h: i32 = 12;
        let gap: i32 = 4;

        // Label (left)
        screen.draw_text(
            &self.label,
            rect.x(),
            rect.y(),
            Some(theme.text_dim),
            font_size,
            false,
            None,
        );

        // Percentage text (right)
        if self.show_percentage {
            let pct_text = format!("{}%", (self.progress * 100.0) as i32);
            let pct_w = screen.get_text_width(&pct_text, font_size, false);
            screen.draw_text(
                &pct_text,
                rect.x() + rect.width() as i32 - pct_w as i32,
                rect.y(),
                Some(theme.text),
                font_size,
                false,
                None,
            );
        }

        // Bar track
        let bar_y = rect.y() + label_h + gap;
        let bar_w = rect.width();
        let bar_rect = Rect::new(rect.x(), bar_y, bar_w, bar_h as u32);

        screen.draw_progress_bar(bar_rect, self.progress, None, None, 4);
    }
}
