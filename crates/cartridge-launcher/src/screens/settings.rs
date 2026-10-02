#[path = "settings_hardware.rs"]
mod hardware;

use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use cartridge_core::theme::THEME_PRESETS;
use hardware::HardwareControl;
use sdl2::rect::Rect;

use super::{LauncherScreen, ScreenAction, ScreenContext, ScreenId};
use crate::neo::{self, Chip, Hint};
use crate::ui_constants::*;

const NEO_ROW_H: i32 = 56;
const CARD_ROW_H: i32 = 52;
const NEO_LIST_Y: i32 = neo::CONTENT_Y + 12;

const CACHE_OPTIONS: &[u32] = &[15, 30, 60, 120, 360];
const SETTINGS_ROWS: usize = 11;
// Step size for brightness/volume left/right adjustments.
const HW_STEP: u8 = 10;
// Row indices.
//   0: Registry URL  (read-only)
//   1: Auto Refresh
//   2: Cache Duration
//   3: Show Process Panel
//   4: Theme
//   5: Animations
//   6: Sounds
//   7: WiFi
//   8: Brightness    (hardware)
//   9: Volume        (hardware)
//  10: About
const ROW_THEME: usize = 4;
const ROW_ANIMATIONS: usize = 5;
const ROW_SOUNDS: usize = 6;
const ROW_WIFI: usize = 7;
const ROW_BRIGHTNESS: usize = 8;
const ROW_VOLUME: usize = 9;
const ROW_ABOUT: usize = 10;

/// Move to the next/previous theme preset by id, wrapping at the ends.
fn cycle_theme(current: &str, forward: bool) -> String {
    let idx = THEME_PRESETS
        .iter()
        .position(|p| p.id == current)
        .unwrap_or(0);
    let n = THEME_PRESETS.len();
    let next = if forward {
        (idx + 1) % n
    } else if idx == 0 {
        n - 1
    } else {
        idx - 1
    };
    THEME_PRESETS[next].id.to_string()
}

/// Display name for a theme id (falls back to the id if unknown).
fn theme_display_name(id: &str) -> &'static str {
    THEME_PRESETS
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name)
        .unwrap_or("Unknown")
}

pub struct SettingsScreen {
    selected_row: usize,
    first_row: usize,
    brightness: HardwareControl,
    volume: HardwareControl,
}

impl Default for SettingsScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsScreen {
    pub fn new() -> Self {
        Self {
            selected_row: 0,
            first_row: 0,
            brightness: HardwareControl::brightness(),
            volume: HardwareControl::volume(),
        }
    }
}

impl LauncherScreen for SettingsScreen {
    fn update(&mut self, _ctx: &mut ScreenContext) -> bool {
        // Poll both controls even when the first one changes.
        let brightness = self.brightness.poll();
        let volume = self.volume.poll();
        brightness || volume
    }

    fn is_loading(&self) -> bool {
        self.brightness.is_pending() || self.volume.is_pending()
    }

    fn handle_input(&mut self, events: &[InputEvent], ctx: &mut ScreenContext) -> ScreenAction {
        for ie in events {
            if ie.action != InputAction::Press && ie.action != InputAction::Repeat {
                continue;
            }

            match ie.button {
                Button::B => {
                    return ScreenAction::Pop;
                }
                Button::DpadDown => {
                    if self.selected_row + 1 < SETTINGS_ROWS {
                        self.selected_row += 1;
                    }
                }
                Button::DpadUp => {
                    if self.selected_row > 0 {
                        self.selected_row -= 1;
                    }
                }
                Button::A | Button::DpadRight => {
                    match self.selected_row {
                        1 => {
                            // Toggle auto-refresh
                            ctx.settings.auto_refresh = !ctx.settings.auto_refresh;
                            ctx.save_settings();
                        }
                        2 => {
                            // Cycle cache duration forward
                            let current = ctx.settings.cache_duration_mins;
                            let idx = CACHE_OPTIONS
                                .iter()
                                .position(|&v| v == current)
                                .unwrap_or(0);
                            let next = (idx + 1) % CACHE_OPTIONS.len();
                            ctx.settings.cache_duration_mins = CACHE_OPTIONS[next];
                            ctx.save_settings();
                        }
                        3 => {
                            // Toggle process panel
                            ctx.settings.show_processes = !ctx.settings.show_processes;
                            ctx.save_settings();
                        }
                        ROW_THEME => {
                            ctx.settings.theme_id = cycle_theme(&ctx.settings.theme_id, true);
                            ctx.save_settings();
                        }
                        ROW_ANIMATIONS => {
                            ctx.settings.animations_enabled = !ctx.settings.animations_enabled;
                            ctx.save_settings();
                        }
                        ROW_SOUNDS => {
                            ctx.settings.sounds_enabled = !ctx.settings.sounds_enabled;
                            ctx.save_settings();
                        }
                        ROW_WIFI => {
                            return ScreenAction::Push(ScreenId::WiFi);
                        }
                        ROW_BRIGHTNESS => {
                            if ie.button == Button::A && self.brightness.has_error() {
                                self.brightness.retry();
                            } else {
                                self.brightness.adjust(i16::from(HW_STEP));
                            }
                        }
                        ROW_VOLUME => {
                            if ie.button == Button::A && self.volume.has_error() {
                                self.volume.retry();
                            } else {
                                self.volume.adjust(i16::from(HW_STEP));
                            }
                        }
                        _ => {}
                    }
                }
                Button::DpadLeft => match self.selected_row {
                    1 => {
                        ctx.settings.auto_refresh = !ctx.settings.auto_refresh;
                        ctx.save_settings();
                    }
                    2 => {
                        let current = ctx.settings.cache_duration_mins;
                        let idx = CACHE_OPTIONS
                            .iter()
                            .position(|&v| v == current)
                            .unwrap_or(0);
                        let next = if idx == 0 {
                            CACHE_OPTIONS.len() - 1
                        } else {
                            idx - 1
                        };
                        ctx.settings.cache_duration_mins = CACHE_OPTIONS[next];
                        ctx.save_settings();
                    }
                    3 => {
                        ctx.settings.show_processes = !ctx.settings.show_processes;
                        ctx.save_settings();
                    }
                    ROW_THEME => {
                        ctx.settings.theme_id = cycle_theme(&ctx.settings.theme_id, false);
                        ctx.save_settings();
                    }
                    ROW_ANIMATIONS => {
                        ctx.settings.animations_enabled = !ctx.settings.animations_enabled;
                        ctx.save_settings();
                    }
                    ROW_SOUNDS => {
                        ctx.settings.sounds_enabled = !ctx.settings.sounds_enabled;
                        ctx.save_settings();
                    }
                    ROW_BRIGHTNESS => {
                        self.brightness.adjust(-i16::from(HW_STEP));
                    }
                    ROW_VOLUME => {
                        self.volume.adjust(-i16::from(HW_STEP));
                    }
                    _ => {}
                },
                Button::Select => {
                    return ScreenAction::ShowOverlay;
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext) {
        let is_neo = neo::is_neo(screen.theme);
        let theme = screen.theme;
        let start_y = if is_neo { NEO_LIST_Y } else { CONTENT_TOP + 6 };
        let row_h = if is_neo { NEO_ROW_H } else { CARD_ROW_H };
        let pitch = if is_neo { row_h } else { row_h + MARGIN };
        let bottom = settings_list_bottom(ctx, is_neo);
        let visible = ((bottom - start_y) / pitch).max(1) as usize;
        let range = visible_rows(self.selected_row, &mut self.first_row, visible);
        let rows = self.rows(ctx);

        if is_neo {
            neo::draw_header(screen, "SETTINGS", "", Some(&ctx.sysinfo));
            neo::draw_bar(screen);
        } else {
            screen.draw_rect(
                Rect::new(0, 0, SCREEN_WIDTH, HEADER_HEIGHT as u32),
                Some(sdl2::pixels::Color::RGBA(14, 14, 20, 220)),
                true,
                0,
                None,
            );
            screen.draw_glow_line(
                0,
                0,
                SCREEN_WIDTH as i32 - 1,
                sdl2::pixels::Color::RGBA(100, 180, 255, 80),
                3,
                1,
            );
            screen.draw_text_glow(
                "Settings",
                12,
                8,
                theme.accent,
                theme.glow_primary,
                20,
                true,
                None,
            );
        }

        // Only whole, visible rows are drawn. This also reserves room for the
        // shared Store notice banner so About/hardware errors stay reachable.
        for (slot, index) in range.enumerate() {
            let row = &rows[index];
            let y = start_y + slot as i32 * pitch;
            let selected = index == self.selected_row;
            let left = if is_neo { neo::MARGIN_X } else { 12 };
            let right = SCREEN_WIDTH as i32 - left;
            let tx = left + 16;
            let width = (right - left) as u32;
            if is_neo {
                if selected {
                    screen.fill(Rect::new(left, y, width, row_h as u32), theme.card_bg);
                    screen.fill(Rect::new(left, y, 4, row_h as u32), theme.accent);
                }
                screen.fill(Rect::new(left, y + row_h - 1, width, 1), theme.border);
            } else {
                screen.draw_card(
                    Rect::new(left, y, width, row_h as u32),
                    Some(if selected {
                        theme.card_highlight
                    } else {
                        theme.card_bg
                    }),
                    Some(if selected {
                        theme.accent
                    } else {
                        theme.card_border
                    }),
                    CARD_RADIUS,
                    false,
                );
            }
            let title = if is_neo {
                row.title.to_uppercase()
            } else {
                row.title.into()
            };
            let title_color = if selected || !is_neo {
                theme.text
            } else {
                theme.text_dim
            };
            screen.draw_text(
                &title,
                tx,
                y + 8,
                Some(title_color),
                14,
                true,
                Some(width - 32),
            );
            screen.draw_text(
                &row.subtitle,
                tx,
                y + row_h - 20,
                Some(if row.error {
                    theme.text_error
                } else {
                    theme.text_dim
                }),
                12,
                false,
                Some(width - 32),
            );

            // Values occupy the title line; subtitles have the full row width.
            let value_right = right - 12;
            match &row.value {
                RowValue::Toggle(on) => {
                    let label = if *on { "ON" } else { "OFF" };
                    if is_neo {
                        let width =
                            screen.get_text_width(label, neo::LABEL_SIZE, false) as i32 + 16;
                        neo::draw_chip(
                            screen,
                            label,
                            value_right - width,
                            y + 6,
                            if *on {
                                Chip::FilledRed
                            } else {
                                Chip::OutlineDim
                            },
                        );
                    } else {
                        screen.draw_pill(
                            label,
                            value_right - 50,
                            y + 6,
                            if *on { theme.positive } else { theme.text_dim },
                            theme.card_bg,
                            13,
                        );
                    }
                }
                RowValue::Cycle(value) => {
                    let label = if selected {
                        format!("<  {value}  >")
                    } else {
                        value.clone()
                    };
                    neo::text_right(
                        screen,
                        &label,
                        value_right,
                        y + 23,
                        if selected {
                            theme.text_accent
                        } else {
                            theme.text_dim
                        },
                        13,
                        true,
                    );
                }
                RowValue::Slider(value) => {
                    let bar_w = 160;
                    let bar_x = value_right - bar_w;
                    let bar_y = y + 15;
                    screen.fill(Rect::new(bar_x, bar_y, bar_w as u32, 4), theme.card_border);
                    if let Some(value) = value {
                        let fill_w = bar_w as u32 * u32::from(*value) / 100;
                        if fill_w > 0 {
                            screen.fill(
                                Rect::new(bar_x, bar_y, fill_w, 4),
                                if row.error {
                                    theme.text_error
                                } else {
                                    theme.accent
                                },
                            );
                        }
                    }
                    let label = value.map_or_else(|| "--".into(), |v| format!("{v}%"));
                    neo::text_right(
                        screen,
                        &label,
                        bar_x - 12,
                        y + 23,
                        theme.text_dim,
                        12,
                        false,
                    );
                }
                RowValue::Chevron => {
                    screen.draw_text(
                        ">",
                        value_right - 10,
                        y + 8,
                        Some(if selected {
                            theme.accent
                        } else {
                            theme.text_dim
                        }),
                        14,
                        true,
                        None,
                    );
                }
                RowValue::None => {}
            }
        }

        let action = self.primary_action();
        if is_neo {
            let mut hints = Vec::with_capacity(3);
            if let Some(action) = action {
                hints.push(Hint::a(action));
            }
            hints.push(Hint::b("Back"));
            hints.push(Hint::wide("D-PAD", "Navigate / adjust"));
            neo::draw_footer(screen, &hints);
        } else {
            draw_settings_footer(screen, action);
        }
        let footer_y = if is_neo {
            neo::FOOTER_Y
        } else {
            SCREEN_HEIGHT as i32 - FOOTER_HEIGHT
        };
        neo::text_right(
            screen,
            &format!("{} / {SETTINGS_ROWS}", self.selected_row + 1),
            SCREEN_WIDTH as i32 - 18,
            footer_y + 26,
            theme.text_dim,
            12,
            false,
        );
    }
}

enum RowValue {
    Toggle(bool),
    Cycle(String),
    Slider(Option<u8>),
    Chevron,
    None,
}

struct SettingsRow {
    title: &'static str,
    subtitle: String,
    value: RowValue,
    error: bool,
}

impl SettingsRow {
    fn new(title: &'static str, subtitle: impl Into<String>, value: RowValue) -> Self {
        Self {
            title,
            subtitle: subtitle.into(),
            value,
            error: false,
        }
    }

    fn hardware(title: &'static str, control: &HardwareControl) -> Self {
        Self {
            title,
            subtitle: control.status(),
            value: RowValue::Slider(control.value()),
            error: control.has_error(),
        }
    }
}

impl SettingsScreen {
    fn rows(&self, ctx: &ScreenContext) -> [SettingsRow; SETTINGS_ROWS] {
        let wifi = ctx.sysinfo.wifi_ssid.as_ref().map_or_else(
            || "Not connected".into(),
            |ssid| format!("Connected to {ssid}"),
        );
        [
            SettingsRow::new("Registry URL", &ctx.settings.registry_url, RowValue::None),
            SettingsRow::new(
                "Auto Refresh",
                "Refresh the catalog when opening Store",
                RowValue::Toggle(ctx.settings.auto_refresh),
            ),
            SettingsRow::new(
                "Cache Duration",
                "How long to keep registry data",
                RowValue::Cycle(format_cache_duration(ctx.settings.cache_duration_mins)),
            ),
            SettingsRow::new(
                "Show Process Panel",
                "Show top processes on the home screen",
                RowValue::Toggle(ctx.settings.show_processes),
            ),
            SettingsRow::new(
                "Theme",
                "Visual style for the launcher",
                RowValue::Cycle(theme_display_name(&ctx.settings.theme_id).into()),
            ),
            SettingsRow::new(
                "Animations",
                "Moving theme effects",
                RowValue::Toggle(ctx.settings.animations_enabled),
            ),
            SettingsRow::new(
                "Sounds",
                "Click feedback on navigation and launch",
                RowValue::Toggle(ctx.settings.sounds_enabled),
            ),
            SettingsRow::new("WiFi", wifi, RowValue::Chevron),
            SettingsRow::hardware("Brightness", &self.brightness),
            SettingsRow::hardware("Volume", &self.volume),
            SettingsRow::new("About CartridgeOS", about_status(ctx), RowValue::None),
        ]
    }

    fn primary_action(&self) -> Option<&'static str> {
        let control = match self.selected_row {
            ROW_BRIGHTNESS => Some(&self.brightness),
            ROW_VOLUME => Some(&self.volume),
            _ => None,
        };
        if let Some(control) = control {
            Some(if control.has_error() {
                "Retry"
            } else {
                "Increase"
            })
        } else {
            match self.selected_row {
                ROW_WIFI => Some("Open"),
                2 | ROW_THEME => Some("Next"),
                0 | ROW_ABOUT => None,
                _ => Some("Toggle"),
            }
        }
    }
}

fn about_status(ctx: &ScreenContext) -> String {
    let store = if ctx.store_jobs.is_busy() {
        "busy"
    } else if ctx.store_jobs.notices.iter().any(|notice| notice.is_error) {
        "needs attention"
    } else if ctx.registry_client.is_none() {
        "unavailable"
    } else if ctx.registry.apps.iter().any(|app| app.package.is_some()) {
        "catalog ready"
    } else {
        "local catalog"
    };
    // Package version identifies this running binary, not an OS release file.
    // All remaining data is already cached by the launcher; no stat/read here.
    format!(
        "Runtime {} · {} · Store: {store}",
        env!("CARGO_PKG_VERSION"),
        ctx.sysinfo.hostname
    )
}

fn settings_list_bottom(ctx: &ScreenContext, is_neo: bool) -> i32 {
    let has_notice = ctx.store_jobs.progress.is_some() || !ctx.store_jobs.notices.is_empty();
    if has_notice {
        if is_neo { neo::FOOTER_Y - 112 } else { 572 }
    } else if is_neo {
        neo::FOOTER_Y - 8
    } else {
        CONTENT_BOTTOM - 8
    }
}

fn visible_rows(selected: usize, first: &mut usize, capacity: usize) -> std::ops::Range<usize> {
    let capacity = capacity.max(1).min(SETTINGS_ROWS);
    *first = (*first).min(SETTINGS_ROWS - capacity);
    if selected < *first {
        *first = selected;
    } else if selected >= *first + capacity {
        *first = selected + 1 - capacity;
    }
    *first..(*first + capacity).min(SETTINGS_ROWS)
}

fn draw_settings_footer(screen: &mut Screen, action: Option<&str>) {
    let theme = screen.theme;
    let footer_y = SCREEN_HEIGHT as i32 - FOOTER_HEIGHT;
    screen.draw_rect(
        Rect::new(0, footer_y, SCREEN_WIDTH, FOOTER_HEIGHT as u32),
        Some(sdl2::pixels::Color::RGBA(14, 14, 20, 220)),
        true,
        0,
        None,
    );
    screen.draw_glow_line(
        footer_y,
        0,
        SCREEN_WIDTH as i32 - 1,
        sdl2::pixels::Color::RGBA(100, 180, 255, 50),
        2,
        -1,
    );
    let mut x = 12;
    if let Some(action) = action {
        x += screen.draw_button_hint("A", action, x, footer_y + 8, Some(theme.btn_a), 12) as i32
            + 12;
    }
    x += screen.draw_button_hint("B", "Back", x, footer_y + 8, Some(theme.btn_b), 12) as i32 + 12;
    screen.draw_button_hint(
        "D-Pad",
        "Navigate / adjust",
        x,
        footer_y + 8,
        Some(theme.btn_l),
        12,
    );
}

fn format_cache_duration(mins: u32) -> String {
    if mins < 60 {
        format!("{mins} min")
    } else {
        format!("{} hr", mins / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_selected_row_is_visible_with_both_themes_and_store_banner() {
        for (start, bottom, pitch, height) in [
            (NEO_LIST_Y, neo::FOOTER_Y - 8, NEO_ROW_H, NEO_ROW_H),
            (NEO_LIST_Y, neo::FOOTER_Y - 112, NEO_ROW_H, NEO_ROW_H),
            (
                CONTENT_TOP + 6,
                CONTENT_BOTTOM - 8,
                CARD_ROW_H + MARGIN,
                CARD_ROW_H,
            ),
            (CONTENT_TOP + 6, 572, CARD_ROW_H + MARGIN, CARD_ROW_H),
        ] {
            let capacity = ((bottom - start) / pitch) as usize;
            let mut first = 0;
            for selected in (0..SETTINGS_ROWS).chain((0..SETTINGS_ROWS).rev()) {
                let range = visible_rows(selected, &mut first, capacity);
                assert!(range.contains(&selected));
                let selected_bottom = start + (selected - first) as i32 * pitch + height;
                assert!(selected_bottom <= bottom);
            }
        }
    }

    #[test]
    fn changing_visible_capacity_keeps_about_in_view() {
        let mut first = 0;
        assert!(visible_rows(ROW_ABOUT, &mut first, 10).contains(&ROW_ABOUT));
        assert!(visible_rows(ROW_ABOUT, &mut first, 8).contains(&ROW_ABOUT));
        assert!(visible_rows(ROW_ABOUT, &mut first, 10).contains(&ROW_ABOUT));
        assert!(visible_rows(0, &mut first, 10).contains(&0));
    }
}
