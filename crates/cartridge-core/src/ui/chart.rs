//! Chart widgets: SparkLine (mini inline) and LineChart (full with axes).

use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::screen::Screen;

/// Mini inline sparkline chart for embedding in rows or small spaces.
///
/// Draws a simple polyline of data points scaled to fit `rect`.
pub struct SparkLine {
    pub data: Vec<f32>,
    pub color: Option<Color>,
}

impl SparkLine {
    pub fn new(data: Vec<f32>) -> Self {
        Self { data, color: None }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Draw the sparkline within `rect`. Uses `screen.draw_sparkline` internally.
    pub fn draw(&self, screen: &mut Screen, rect: Rect) {
        if self.data.len() < 2 {
            return;
        }
        let baseline = Some(Color::RGB(
            screen.theme.border.r.saturating_add(8),
            screen.theme.border.g.saturating_add(8),
            screen.theme.border.b.saturating_add(8),
        ));
        screen.draw_sparkline(&self.data, rect, self.color, baseline);
    }
}

/// Full chart with Y-axis labels, grid lines, X-axis labels, and a data line.
///
/// Designed for financial data or system metrics. Draws grid, axis labels,
/// the data polyline, and an end-point dot.
pub struct LineChart {
    pub data: Vec<f32>,
    pub labels: Vec<String>,
    pub color: Option<Color>,
    pub title: String,
    /// Format function for Y-axis labels. Defaults to "$X.XX" style.
    /// Set to `None` for the default formatter.
    pub y_format: Option<fn(f32) -> String>,
}

impl LineChart {
    pub fn new(data: Vec<f32>) -> Self {
        Self {
            data,
            labels: Vec::new(),
            color: None,
            title: String::new(),
            y_format: None,
        }
    }

    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.labels = labels;
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    pub fn with_y_format(mut self, f: fn(f32) -> String) -> Self {
        self.y_format = Some(f);
        self
    }

    /// Draw the chart within `rect`.
    pub fn draw(&self, screen: &mut Screen, rect: Rect) {
        let theme = screen.theme;
        let data = &self.data;

        if data.len() < 2 {
            let text = "No chart data";
            let tw = screen.get_text_width(text, 13, false);
            let th = screen.get_line_height(13, false);
            screen.draw_text(
                text,
                rect.x() + (rect.width() as i32 - tw as i32) / 2,
                rect.y() + (rect.height() as i32 - th as i32) / 2,
                Some(theme.text_dim),
                13,
                false,
                None,
            );
            return;
        }

        let color = self.color.unwrap_or(theme.accent);

        // Margins
        let left_margin = 60i32;
        let right_margin = 12i32;
        let mut top_margin = 8i32;
        let bottom_margin = 24i32;

        // Title
        if !self.title.is_empty() {
            let title_lh = screen.get_line_height(13, true);
            screen.draw_text(
                &self.title,
                rect.x() + left_margin,
                rect.y() + 2,
                Some(theme.text_dim),
                13,
                true,
                None,
            );
            top_margin += title_lh as i32;
        }

        let chart_x = rect.x() + left_margin;
        let chart_y = rect.y() + top_margin;
        let chart_w = rect.width() as i32 - left_margin - right_margin;
        let chart_h = rect.height() as i32 - top_margin - bottom_margin;

        if chart_w <= 0 || chart_h <= 0 {
            return;
        }

        let mn = data.iter().copied().fold(f32::INFINITY, f32::min);
        let mx = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let rng = if (mx - mn).abs() > f32::EPSILON {
            mx - mn
        } else {
            1.0
        };

        // Grid lines and Y labels (5 horizontal lines)
        let grid_color = Color::RGB(
            theme.border.r.saturating_add(8),
            theme.border.g.saturating_add(8),
            theme.border.b.saturating_add(8),
        );
        let num_grid: i32 = 4;

        for i in 0..=num_grid {
            let gy = chart_y + (i as f32 / num_grid as f32 * chart_h as f32) as i32;
            screen.draw_line(
                (chart_x, gy),
                (chart_x + chart_w, gy),
                Some(grid_color),
                1,
            );

            let val = mx - (i as f32 / num_grid as f32) * rng;
            let label_text = if let Some(fmt) = self.y_format {
                fmt(val)
            } else {
                default_y_format(val)
            };

            let lh = screen.get_line_height(11, false);
            screen.draw_text(
                &label_text,
                rect.x() + 2,
                gy - lh as i32 / 2,
                Some(theme.text_dim),
                11,
                false,
                None,
            );
        }

        // Data points
        let points: Vec<(i32, i32)> = data
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let px =
                    chart_x + (i as f32 / (data.len() - 1) as f32 * (chart_w - 1) as f32) as i32;
                let py = chart_y + chart_h - 1
                    - (((v - mn) / rng) * (chart_h - 1) as f32) as i32;
                (px, py)
            })
            .collect();

        // Draw the line segments
        if points.len() >= 2 {
            for window in points.windows(2) {
                screen.draw_line(window[0], window[1], Some(color), 2);
            }

            // End dot
            let last = *points.last().unwrap();
            screen.draw_circle(last.0, last.1, 3, color);
        }

        // X-axis labels
        if !self.labels.is_empty() {
            let n_labels = 5.min(self.labels.len());
            let step = (self.labels.len() / n_labels).max(1);
            let mut i = 0;
            while i < self.labels.len() {
                let lx = chart_x
                    + (i as f32 / (self.labels.len() - 1).max(1) as f32 * (chart_w - 1) as f32)
                        as i32;
                let lw = screen.get_text_width(&self.labels[i], 11, false);
                screen.draw_text(
                    &self.labels[i],
                    lx - lw as i32 / 2,
                    chart_y + chart_h + 4,
                    Some(theme.text_dim),
                    11,
                    false,
                    None,
                );
                i += step;
            }
        }

        // Chart border
        screen.draw_rect(
            Rect::new(chart_x, chart_y, chart_w as u32, chart_h as u32),
            Some(grid_color),
            false,
            0,
            None,
        );
    }
}

fn default_y_format(val: f32) -> String {
    if val.abs() >= 1000.0 {
        // Rust doesn't have a built-in comma formatter; format without commas.
        format!("${:.0}", val)
    } else if val.abs() >= 1.0 {
        format!("${:.2}", val)
    } else {
        format!("${:.4}", val)
    }
}
