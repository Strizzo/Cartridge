//! Shared chrome for the Neo-Tokyo launcher screens: header with the big
//! clock, the red hazard bar, the one-line system readout, the footer with
//! square button capsules, chips, and text helpers.
//!
//! Every screen draws the same four bands so the OS reads as one surface:
//!
//!   0..64    header      title (display face) · status · clock
//!   64..70   bar         red, with a hazard-stripe segment on the right
//!   ...      content
//!   640..672 readout     CPU · RAM · NET · DISK · UP · WIFI   (home only)
//!   672..720 footer      A OPEN  Y STORE  X REMOVE       START SETTINGS

use cartridge_core::screen::Screen;
use cartridge_core::sysinfo::SystemInfo;
use cartridge_core::theme::{Theme, UiStyle};
use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::ui_constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

pub const MARGIN_X: i32 = 18;
pub const HEADER_H: i32 = 64;
pub const BAR_H: i32 = 6;
pub const CONTENT_Y: i32 = HEADER_H + BAR_H;
pub const FOOTER_H: i32 = 48;
pub const FOOTER_Y: i32 = SCREEN_HEIGHT as i32 - FOOTER_H;
pub const READOUT_H: i32 = 32;
pub const READOUT_Y: i32 = FOOTER_Y - READOUT_H;
/// Width of the hazard-stripe segment at the right end of the red bar.
const HAZARD_W: u32 = 150;
/// Smallest text drawn on the device. The mockups used 10px tracked
/// labels; at 255 ppi that is a 1 mm em, one step below anything the
/// launcher ships today, so labels are 11px here.
pub const LABEL_SIZE: u16 = 11;

pub fn is_neo(theme: &Theme) -> bool {
    theme.ui == UiStyle::Neo
}

/// The OS version shown in the header and boot selector.
pub fn os_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// ── Text helpers ────────────────────────────────────────────────────────

/// Draw display-face text with its baseline at `baseline_y`. Returns width.
pub fn display_at_baseline(screen: &mut Screen, text: &str, x: i32, baseline_y: i32, color: Color, size: u16) -> u32 {
    let ascent = screen.display_ascent(size);
    screen.draw_display_text(text, x, baseline_y - ascent, color, size)
}

/// Draw body text with its baseline at `baseline_y`. Returns width.
pub fn text_at_baseline(screen: &mut Screen, text: &str, x: i32, baseline_y: i32, color: Color, size: u16, bold: bool) -> u32 {
    let ascent = screen.text_ascent(size, bold);
    screen.draw_text(text, x, baseline_y - ascent, Some(color), size, bold, None)
}

/// Right-aligned display text (right edge at `right_x`, baseline at `baseline_y`).
pub fn display_right(screen: &mut Screen, text: &str, right_x: i32, baseline_y: i32, color: Color, size: u16) -> u32 {
    let w = screen.display_text_width(text, size);
    display_at_baseline(screen, text, right_x - w as i32, baseline_y, color, size)
}

/// Right-aligned body text.
pub fn text_right(screen: &mut Screen, text: &str, right_x: i32, baseline_y: i32, color: Color, size: u16, bold: bool) -> u32 {
    let w = screen.get_text_width(text, size, bold);
    text_at_baseline(screen, text, right_x - w as i32, baseline_y, color, size, bold)
}

/// Greedy word wrap into at most `max_lines` lines that fit `max_w`.
/// The last line is truncated with ".." if the text continues.
pub fn wrap_lines(screen: &mut Screen, text: &str, size: u16, bold: bool, max_w: u32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut i = 0;
    while i < words.len() {
        let word = words[i];
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if screen.get_text_width(&candidate, size, bold) <= max_w || line.is_empty() {
            line = candidate;
            i += 1;
        } else {
            lines.push(std::mem::take(&mut line));
            if lines.len() == max_lines {
                break;
            }
        }
    }
    if lines.len() < max_lines && !line.is_empty() {
        lines.push(line);
    } else if lines.len() == max_lines && i < words.len() {
        // Out of lines with words left: mark the last line as continued.
        if let Some(last) = lines.last_mut() {
            while !last.is_empty() && screen.get_text_width(&format!("{last}.."), size, bold) > max_w {
                last.pop();
            }
            last.push_str("..");
        }
    }
    lines
}

// ── Chips and capsules ──────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Chip {
    /// Red fill, black text (UPDATE, active category).
    FilledRed,
    /// Black fill, warm-white text (chips on top of a red surface).
    FilledBlack,
    /// 1px warm-white outline, warm-white text (category, INSTALLED).
    OutlineWhite,
    /// 1px muted outline, dim text (author, permissions).
    OutlineDim,
    /// 1px black outline, black text (chips on top of a red surface).
    OutlineBlack,
}

/// Draw a small uppercase chip at (x, y). Returns its width.
pub fn draw_chip(screen: &mut Screen, text: &str, x: i32, y: i32, kind: Chip) -> u32 {
    let theme = screen.theme;
    let label = text.to_uppercase();
    let size = LABEL_SIZE;
    let tw = screen.get_text_width(&label, size, false);
    let pad = 8;
    let w = tw + pad as u32 * 2;
    let h = 20u32;
    let rect = Rect::new(x, y, w, h);
    let (fill, outline, fg) = match kind {
        Chip::FilledRed => (Some(theme.accent), None, theme.bg),
        Chip::FilledBlack => (Some(theme.bg), None, theme.text),
        Chip::OutlineWhite => (None, Some(theme.text), theme.text),
        Chip::OutlineDim => (None, Some(theme.text_muted), theme.text_dim),
        Chip::OutlineBlack => (None, Some(theme.bg), theme.bg),
    };
    if let Some(c) = fill {
        screen.fill(rect, c);
    }
    if let Some(c) = outline {
        screen.draw_outline(rect, c, 1);
    }
    let ascent = screen.text_ascent(size, false);
    let lh = screen.get_line_height(size, false) as i32;
    let ty = y + (h as i32 - lh) / 2 + lh - ascent - (lh - ascent);
    screen.draw_text(&label, x + pad, ty, Some(fg), size, false, None);
    w
}

/// Draw a row of chips separated by `gap`. Returns the x after the last one.
pub fn draw_chip_row(screen: &mut Screen, chips: &[(String, Chip)], x: i32, y: i32, gap: i32, max_right: i32) -> i32 {
    let mut cx = x;
    for (text, kind) in chips {
        let w = screen.get_text_width(&text.to_uppercase(), LABEL_SIZE, false) as i32 + 16;
        if cx + w > max_right {
            break;
        }
        draw_chip(screen, text, cx, y, *kind);
        cx += w + gap;
    }
    cx
}

/// Footer hint kinds.
#[derive(Clone, Copy)]
pub enum Cap {
    /// 26px square, warm-white fill, black letter (A, Y).
    White,
    /// 26px square, red fill, black letter (B, X).
    Red,
    /// Outlined wide box with small uppercase text (START, L1/R1, SELECT).
    Wide,
}

/// Draw one button capsule at (x, y). Returns its width.
pub fn draw_capsule(screen: &mut Screen, label: &str, x: i32, y: i32, kind: Cap) -> u32 {
    let theme = screen.theme;
    match kind {
        Cap::White | Cap::Red => {
            let rect = Rect::new(x, y, 26, 26);
            let fill = if matches!(kind, Cap::Red) { theme.accent } else { theme.text };
            screen.fill(rect, fill);
            let size = 19;
            let w = screen.display_text_width(label, size) as i32;
            let ascent = screen.display_ascent(size);
            // Bebas caps sit on the baseline; centre the cap height (~ascent) in the box.
            let ty = y + (26 - ascent) / 2 + 1;
            screen.draw_display_text(label, x + (26 - w) / 2, ty, theme.bg, size);
            26
        }
        Cap::Wide => {
            let text = label.to_uppercase();
            let tw = screen.get_text_width(&text, LABEL_SIZE, false);
            let w = tw + 18;
            let rect = Rect::new(x, y + 1, w, 24);
            screen.draw_outline(rect, theme.text_dim, 1);
            let lh = screen.get_line_height(LABEL_SIZE, false) as i32;
            screen.draw_text(&text, x + 9, y + 1 + (24 - lh) / 2, Some(theme.text), LABEL_SIZE, false, None);
            w
        }
    }
}

/// A footer hint: capsule + uppercase label.
pub struct Hint {
    pub label: &'static str,
    pub action: String,
    pub kind: Cap,
    /// Right-aligned group (START SETTINGS sits at the right edge).
    pub right: bool,
}

impl Hint {
    pub fn new(label: &'static str, action: &str, kind: Cap) -> Self {
        Self { label, action: action.to_string(), kind, right: false }
    }
    pub fn right(mut self) -> Self {
        self.right = true;
        self
    }
    pub fn a(action: &str) -> Self { Self::new("A", action, Cap::White) }
    pub fn b(action: &str) -> Self { Self::new("B", action, Cap::Red) }
    pub fn x(action: &str) -> Self { Self::new("X", action, Cap::Red) }
    pub fn y(action: &str) -> Self { Self::new("Y", action, Cap::White) }
    pub fn start(action: &str) -> Self { Self::new("START", action, Cap::Wide).right() }
    pub fn wide(label: &'static str, action: &str) -> Self { Self::new(label, action, Cap::Wide) }
}

/// Draw the footer band: top rule, left-aligned hints, right-aligned hints.
pub fn draw_footer(screen: &mut Screen, hints: &[Hint]) {
    let theme = screen.theme;
    screen.fill(Rect::new(0, FOOTER_Y, SCREEN_WIDTH, FOOTER_H as u32), theme.bg);
    screen.fill(Rect::new(0, FOOTER_Y, SCREEN_WIDTH, 1), theme.border);
    let cy = FOOTER_Y + (FOOTER_H - 26) / 2;
    let lh = screen.get_line_height(LABEL_SIZE, false) as i32;
    let ty = FOOTER_Y + (FOOTER_H - lh) / 2;

    let mut x = MARGIN_X;
    for h in hints.iter().filter(|h| !h.right) {
        let w = draw_capsule(screen, h.label, x, cy, h.kind) as i32;
        let label = h.action.to_uppercase();
        let lw = screen.draw_text(&label, x + w + 9, ty, Some(theme.text), LABEL_SIZE, false, None);
        x += w + 9 + lw as i32 + 26;
    }

    let mut rx = SCREEN_WIDTH as i32 - MARGIN_X;
    for h in hints.iter().filter(|h| h.right).rev() {
        let label = h.action.to_uppercase();
        let lw = screen.get_text_width(&label, LABEL_SIZE, false) as i32;
        let cw = match h.kind {
            Cap::Wide => screen.get_text_width(&h.label.to_uppercase(), LABEL_SIZE, false) as i32 + 18,
            _ => 26,
        };
        let start = rx - lw - 9 - cw;
        draw_capsule(screen, h.label, start, cy, h.kind);
        screen.draw_text(&label, start + cw + 9, ty, Some(theme.text_dim), LABEL_SIZE, false, None);
        rx = start - 26;
    }
}

// ── Header, bar, readout ────────────────────────────────────────────────

/// Draw the header band. `title` is set in the display face; `subtitle`
/// (already uppercase-able) sits to its right in small dim text. The
/// right side shows wifi + battery and the big red clock.
pub fn draw_header(screen: &mut Screen, title: &str, subtitle: &str, sysinfo: Option<&SystemInfo>) {
    let theme = screen.theme;
    screen.fill(Rect::new(0, 0, SCREEN_WIDTH, HEADER_H as u32), theme.bg);
    let baseline = HEADER_H - 10;

    let tw = display_at_baseline(screen, title, MARGIN_X, baseline, theme.text, 36) as i32;
    if !subtitle.is_empty() {
        text_at_baseline(screen, &subtitle.to_uppercase(), MARGIN_X + tw + 12, baseline - 2, theme.text_dim, LABEL_SIZE, false);
    }

    // Clock, right-aligned.
    let clock = clock_string();
    let cw = display_right(screen, &clock, SCREEN_WIDTH as i32 - MARGIN_X, baseline + 2, theme.accent, 48) as i32;

    if let Some(info) = sysinfo {
        let mut rx = SCREEN_WIDTH as i32 - MARGIN_X - cw - 16;
        let status_baseline = baseline - 2;

        // Battery: percentage then a small glyph to its left.
        if info.battery_percent >= 0 {
            let pct = format!("{}%", info.battery_percent);
            let pw = text_right(screen, &pct, rx, status_baseline, theme.text, LABEL_SIZE, false) as i32;
            rx -= pw + 6;
            let gx = rx - 18;
            let gy = status_baseline - 10;
            screen.draw_outline(Rect::new(gx, gy, 16, 10), theme.text, 1);
            screen.fill(Rect::new(gx + 16, gy + 3, 2, 4), theme.text);
            let fill_w = ((info.battery_percent as u32).min(100) * 10 / 100).max(1);
            screen.fill(Rect::new(gx + 3, gy + 3, fill_w, 4), theme.text);
            if info.battery_charging {
                screen.fill(Rect::new(gx + 7, gy - 3, 2, 2), theme.accent);
            }
            rx = gx - 14;
        }

        // WiFi: filled square + SSID when connected, outline + NO WIFI otherwise.
        let (label, color, filled) = match &info.wifi_ssid {
            Some(ssid) => (ssid.to_uppercase(), theme.text, true),
            None => ("NO WIFI".to_string(), theme.text_dim, false),
        };
        let ww = text_right(screen, &label, rx, status_baseline, color, LABEL_SIZE, false) as i32;
        rx -= ww + 6;
        let sq = Rect::new(rx - 7, status_baseline - 8, 7, 7);
        if filled {
            screen.fill(sq, color);
        } else {
            screen.draw_outline(sq, color, 1);
        }
    }
}

/// The red bar under the header, with the hazard-stripe segment.
pub fn draw_bar(screen: &mut Screen) {
    let theme = screen.theme;
    let y = HEADER_H;
    screen.fill(Rect::new(0, y, SCREEN_WIDTH - HAZARD_W, BAR_H as u32), theme.accent);
    screen.draw_hazard_stripes(
        Rect::new(SCREEN_WIDTH as i32 - HAZARD_W as i32, y, HAZARD_W, BAR_H as u32),
        theme.accent,
        theme.bg,
        20,
    );
}

/// One-line system readout above the footer.
pub fn draw_readout(screen: &mut Screen, sysinfo: &SystemInfo) {
    let theme = screen.theme;
    screen.fill(Rect::new(MARGIN_X, READOUT_Y, SCREEN_WIDTH - MARGIN_X as u32 * 2, 1), theme.border);
    let lh = screen.get_line_height(LABEL_SIZE, false) as i32;
    let y = READOUT_Y + (READOUT_H - lh) / 2;
    let items: [(&str, String, bool); 6] = [
        ("CPU", format!("{:.0}%", sysinfo.cpu_percent), false),
        ("RAM", format!("{}/{}M", sysinfo.mem_used_mb, sysinfo.mem_total_mb), false),
        (
            "NET",
            format!(
                "{} DN · {} UP",
                SystemInfo::format_rate(sysinfo.net_rx_rate),
                SystemInfo::format_rate(sysinfo.net_tx_rate)
            ),
            false,
        ),
        ("DISK", format!("{:.0}/{:.0} GB", sysinfo.disk_used_gb, sysinfo.disk_total_gb), false),
        ("UP", sysinfo.format_uptime().to_uppercase(), false),
        (
            "",
            match &sysinfo.wifi_ssid {
                Some(_) => "WIFI ON".to_string(),
                None => "WIFI OFF".to_string(),
            },
            sysinfo.wifi_ssid.is_none(),
        ),
    ];
    let mut x = MARGIN_X;
    for (key, value, alert) in items.iter() {
        if !key.is_empty() {
            let kw = screen.draw_text(key, x, y, Some(theme.text), LABEL_SIZE, false, None);
            x += kw as i32 + 6;
        }
        let color = if *alert { theme.accent } else { theme.text_dim };
        let vw = screen.draw_text(value, x, y, Some(color), LABEL_SIZE, false, None);
        x += vw as i32 + 22;
        if x > SCREEN_WIDTH as i32 - MARGIN_X - 60 {
            break;
        }
    }
}

/// Horizontal rule across the content width at `y`.
pub fn rule(screen: &mut Screen, y: i32) {
    let c = screen.theme.border;
    screen.fill(Rect::new(MARGIN_X, y, SCREEN_WIDTH - MARGIN_X as u32 * 2, 1), c);
}

/// A full-screen scrim for overlays (system menu, dialogs).
pub fn scrim(screen: &mut Screen, alpha: u8) {
    screen.canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
    screen.fill(Rect::new(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT), Color::RGBA(12, 12, 14, alpha));
}

/// "HH:MM" in local time.
pub fn clock_string() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let local = now as i64 + crate::screens::home::local_tz_offset_secs();
    let secs_today = local.rem_euclid(86400);
    format!("{:02}:{:02}", secs_today / 3600, (secs_today % 3600) / 60)
}

/// Two-character index label ("01", "12").
pub fn index_label(i: usize) -> String {
    format!("{:02}", i + 1)
}
