use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use sdl2::rect::Rect;

use crate::ui_constants::*;
use super::{LauncherScreen, ScreenAction, ScreenContext, ScreenId};

const CACHE_OPTIONS: &[u32] = &[15, 30, 60, 120, 360];
const SETTINGS_ROWS: usize = 5;

pub struct SettingsScreen {
    selected_row: usize,
}

impl Default for SettingsScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsScreen {
    pub fn new() -> Self {
        Self { selected_row: 0 }
    }
}

impl LauncherScreen for SettingsScreen {
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
                            return ScreenAction::Push(ScreenId::WiFi);
                        }
                        _ => {}
                    }
                }
                Button::DpadLeft => {
                    match self.selected_row {
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
                        _ => {}
                    }
                }
                Button::Select => {
                    return ScreenAction::ShowOverlay;
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext) {
        let theme = screen.theme;

        // -- Header (semi-transparent, atmosphere bleeds through) --
        screen.draw_rect(
            Rect::new(0, 0, SCREEN_WIDTH, HEADER_HEIGHT as u32),
            Some(sdl2::pixels::Color::RGBA(14, 14, 20, 220)),
            true,
            0,
            None,
        );
        screen.draw_glow_line(0, 0, SCREEN_WIDTH as i32 - 1, sdl2::pixels::Color::RGBA(100, 180, 255, 80), 3, 1);
        screen.draw_text_glow("Settings", 12, 8, theme.accent, theme.glow_primary, 20, true, None);

        // -- Settings rows as cards --
        let start_y = CONTENT_TOP + 12;
        let row_h = 56;
        let card_w = SCREEN_WIDTH - 24;

        // Row 0: Registry URL
        {
            let y = start_y;
            let is_sel = self.selected_row == 0;
            let bg = if is_sel { theme.card_highlight } else { theme.card_bg };
            let border = if is_sel { theme.accent } else { theme.card_border };

            screen.draw_card(
                Rect::new(12, y, card_w, row_h as u32),
                Some(bg),
                Some(border),
                CARD_RADIUS,
                false,
            );

            screen.draw_text("Registry URL", 24, y + 8, Some(theme.text), 14, true, None);
            screen.draw_text(
                &ctx.settings.registry_url,
                24,
                y + 30,
                Some(theme.text_dim),
                12,
                false,
                Some(card_w - 32),
            );
        }

        // Row 1: Auto Refresh toggle
        {
            let y = start_y + (row_h + MARGIN);
            let is_sel = self.selected_row == 1;
            let bg = if is_sel { theme.card_highlight } else { theme.card_bg };
            let border = if is_sel { theme.accent } else { theme.card_border };

            screen.draw_card(
                Rect::new(12, y, card_w, row_h as u32),
                Some(bg),
                Some(border),
                CARD_RADIUS,
                false,
            );

            screen.draw_text("Auto Refresh", 24, y + 8, Some(theme.text), 14, true, None);
            screen.draw_text(
                "Automatically refresh registry on launch",
                24,
                y + 30,
                Some(theme.text_dim),
                12,
                false,
                None,
            );

            // Toggle indicator
            let toggle_x = SCREEN_WIDTH as i32 - 80;
            let toggle_label = if ctx.settings.auto_refresh { "ON" } else { "OFF" };
            let toggle_color = if ctx.settings.auto_refresh {
                theme.positive
            } else {
                theme.text_dim
            };
            screen.draw_pill(
                toggle_label,
                toggle_x,
                y + 18,
                toggle_color,
                sdl2::pixels::Color::RGB(20, 20, 30),
                13,
            );
        }

        // Row 2: Cache Duration
        {
            let y = start_y + 2 * (row_h + MARGIN);
            let is_sel = self.selected_row == 2;
            let bg = if is_sel { theme.card_highlight } else { theme.card_bg };
            let border = if is_sel { theme.accent } else { theme.card_border };

            screen.draw_card(
                Rect::new(12, y, card_w, row_h as u32),
                Some(bg),
                Some(border),
                CARD_RADIUS,
                false,
            );

            screen.draw_text("Cache Duration", 24, y + 8, Some(theme.text), 14, true, None);
            screen.draw_text(
                "How long to cache registry data",
                24,
                y + 30,
                Some(theme.text_dim),
                12,
                false,
                None,
            );

            // Duration value with arrows
            let dur_str = format_cache_duration(ctx.settings.cache_duration_mins);
            let toggle_x = SCREEN_WIDTH as i32 - 110;
            if is_sel {
                screen.draw_text("<", toggle_x, y + 18, Some(theme.text_accent), 14, true, None);
            }
            let dw = screen.get_text_width(&dur_str, 13, true);
            let center_x = toggle_x + 14 + (60 - dw as i32) / 2;
            screen.draw_text(
                &dur_str,
                center_x,
                y + 19,
                Some(theme.text_accent),
                13,
                true,
                None,
            );
            if is_sel {
                screen.draw_text(">", toggle_x + 78, y + 18, Some(theme.text_accent), 14, true, None);
            }
        }

        // Row 3: WiFi
        {
            let y = start_y + 3 * (row_h + MARGIN);
            let is_sel = self.selected_row == 3;
            let bg = if is_sel { theme.card_highlight } else { theme.card_bg };
            let border = if is_sel { theme.accent } else { theme.card_border };

            screen.draw_card(
                Rect::new(12, y, card_w, row_h as u32),
                Some(bg),
                Some(border),
                CARD_RADIUS,
                false,
            );

            screen.draw_text("WiFi", 24, y + 8, Some(theme.text), 14, true, None);

            let wifi_status = match &ctx.sysinfo.wifi_ssid {
                Some(ssid) => format!("Connected to {ssid}"),
                None => "Not connected".to_string(),
            };
            screen.draw_text(
                &wifi_status,
                24,
                y + 30,
                Some(theme.text_dim),
                12,
                false,
                Some(card_w - 100),
            );

            if is_sel {
                screen.draw_text(
                    ">",
                    card_w as i32 - 4,
                    y + 18,
                    Some(theme.text_accent),
                    16,
                    true,
                    None,
                );
            }
        }

        // Row 4: About
        {
            let y = start_y + 4 * (row_h + MARGIN);
            let is_sel = self.selected_row == 4;
            let bg = if is_sel { theme.card_highlight } else { theme.card_bg };
            let border = if is_sel { theme.accent } else { theme.card_border };

            screen.draw_card(
                Rect::new(12, y, card_w, row_h as u32),
                Some(bg),
                Some(border),
                CARD_RADIUS,
                false,
            );

            screen.draw_text("About CartridgeOS", 24, y + 8, Some(theme.text), 14, true, None);
            screen.draw_text(
                "CartridgeOS v0.5.1 -- A cyberdeck OS for Linux handhelds",
                24,
                y + 30,
                Some(theme.text_dim),
                12,
                false,
                Some(card_w - 32),
            );
        }

        // -- Footer --
        draw_settings_footer(screen);
    }
}

fn draw_settings_footer(screen: &mut Screen) {
    let theme = screen.theme;
    let footer_y = SCREEN_HEIGHT as i32 - FOOTER_HEIGHT;

    screen.draw_rect(
        Rect::new(0, footer_y, SCREEN_WIDTH, FOOTER_HEIGHT as u32),
        Some(sdl2::pixels::Color::RGBA(14, 14, 20, 220)),
        true,
        0,
        None,
    );
    screen.draw_glow_line(footer_y, 0, SCREEN_WIDTH as i32 - 1, sdl2::pixels::Color::RGBA(100, 180, 255, 50), 2, -1);

    let mut fx = 12;
    let w = screen.draw_button_hint("A", "Toggle", fx, footer_y + 8, Some(theme.btn_a), 12);
    fx += w as i32 + 12;
    let w = screen.draw_button_hint("B", "Back", fx, footer_y + 8, Some(theme.btn_b), 12);
    fx += w as i32 + 12;
    screen.draw_button_hint("D-Pad", "Navigate", fx, footer_y + 8, Some(theme.btn_l), 12);
}

fn format_cache_duration(mins: u32) -> String {
    if mins < 60 {
        format!("{mins} min")
    } else {
        let h = mins / 60;
        format!("{h} hr")
    }
}
