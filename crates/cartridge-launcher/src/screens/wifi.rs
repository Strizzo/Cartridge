use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use cartridge_core::ui::text_input::{TextInput, TextInputResult};
use cartridge_net::wifi::{WifiNetwork, WifiStatus};
use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::ui_constants::*;
use super::{LauncherScreen, ScreenAction, ScreenContext};

pub struct WifiScreen {
    selected_row: usize,
    networks: Vec<WifiNetwork>,
    status: WifiStatus,
    status_message: Option<String>,
    message_time: Option<std::time::Instant>,
    scanned: bool,
    scroll_offset: usize,
    /// On-screen keyboard for WiFi password entry.
    password_input: TextInput,
    /// SSID we're currently trying to connect to (while password dialog is open).
    connecting_ssid: Option<String>,
}

const NET_ROW_H: i32 = 48;
const STATUS_ROW_H: i32 = 52;

impl WifiScreen {
    pub fn new() -> Self {
        Self {
            selected_row: 0,
            networks: Vec::new(),
            status: WifiStatus::Unknown,
            status_message: None,
            message_time: None,
            scanned: false,
            scroll_offset: 0,
            password_input: TextInput::new(""),
            connecting_ssid: None,
        }
    }

    fn refresh(&mut self, ctx: &ScreenContext) {
        self.status = ctx.wifi_manager.status();
        self.networks = ctx.wifi_manager.scan_networks();
    }

    fn set_message(&mut self, msg: String) {
        self.status_message = Some(msg);
        self.message_time = Some(std::time::Instant::now());
    }

    fn total_rows(&self) -> usize {
        1 + self.networks.len()
    }
}

impl LauncherScreen for WifiScreen {
    fn handle_input(&mut self, events: &[InputEvent], ctx: &mut ScreenContext) -> ScreenAction {
        if !self.scanned {
            self.scanned = true;
            self.refresh(ctx);
        }

        // If password keyboard is active, route all input there
        if self.password_input.visible {
            for ie in events {
                let result = self.password_input.handle_input(ie);
                match result {
                    TextInputResult::Submitted(password) => {
                        if let Some(ssid) = self.connecting_ssid.take() {
                            if password.is_empty() {
                                self.set_message("Password cannot be empty".to_string());
                            } else {
                                match ctx.wifi_manager.connect_with_password(&ssid, &password) {
                                    Ok(()) => {
                                        self.set_message(format!("Connected to {ssid}"));
                                    }
                                    Err(e) => {
                                        self.set_message(format!("Error: {e}"));
                                    }
                                }
                                self.refresh(ctx);
                            }
                        }
                    }
                    TextInputResult::Cancelled => {
                        self.connecting_ssid = None;
                    }
                    TextInputResult::Pending => {}
                }
            }
            return ScreenAction::None;
        }

        let total = self.total_rows();

        for ie in events {
            if ie.action != InputAction::Press && ie.action != InputAction::Repeat {
                continue;
            }

            match ie.button {
                Button::B => return ScreenAction::Pop,
                Button::DpadDown => {
                    if self.selected_row + 1 < total {
                        self.selected_row += 1;
                    }
                }
                Button::DpadUp => {
                    if self.selected_row > 0 {
                        self.selected_row -= 1;
                    }
                }
                Button::A => {
                    if self.selected_row == 0 {
                        // Status row: disconnect if connected
                        if matches!(self.status, WifiStatus::Connected { .. }) {
                            match ctx.wifi_manager.disconnect() {
                                Ok(()) => self.set_message("Disconnected".to_string()),
                                Err(e) => self.set_message(format!("Error: {e}")),
                            }
                            self.refresh(ctx);
                        }
                    } else {
                        let net_idx = self.selected_row - 1;
                        if let Some(network) = self.networks.get(net_idx) {
                            if network.is_saved {
                                let ssid = network.ssid.clone();
                                match ctx.wifi_manager.connect(&ssid) {
                                    Ok(()) => self.set_message(format!("Connected to {ssid}")),
                                    Err(e) => self.set_message(format!("Error: {e}")),
                                }
                                self.refresh(ctx);
                            } else if network.security == "--" || network.security.is_empty() {
                                // Open network, no password needed
                                let ssid = network.ssid.clone();
                                match ctx.wifi_manager.connect_with_password(&ssid, "") {
                                    Ok(()) => self.set_message(format!("Connected to {ssid}")),
                                    Err(e) => self.set_message(format!("Error: {e}")),
                                }
                                self.refresh(ctx);
                            } else {
                                // Password required — show on-screen keyboard
                                let ssid = network.ssid.clone();
                                let label = format!("Password for {}", ssid);
                                self.password_input.show(&label);
                                self.connecting_ssid = Some(ssid);
                            }
                        }
                    }
                }
                Button::Y => {
                    self.refresh(ctx);
                    self.set_message("Rescanned".to_string());
                }
                Button::Select => return ScreenAction::ShowOverlay,
                _ => {}
            }
        }

        // Keep selected row visible via scroll
        if self.selected_row > 0 {
            let net_idx = self.selected_row - 1;
            // Calculate how many network rows fit
            let list_start = CONTENT_TOP + 12 + STATUS_ROW_H + MARGIN + 20;
            let available = CONTENT_BOTTOM - 28 - list_start;
            let visible_count = (available / (NET_ROW_H + MARGIN)).max(1) as usize;

            if net_idx >= self.scroll_offset + visible_count {
                self.scroll_offset = net_idx + 1 - visible_count;
            }
            if net_idx < self.scroll_offset {
                self.scroll_offset = net_idx;
            }
        } else {
            self.scroll_offset = 0;
        }

        ScreenAction::None
    }

    fn render(&mut self, screen: &mut Screen, _ctx: &ScreenContext) {
        let theme = screen.theme;

        // Clear status message after 3 seconds
        if let Some(t) = self.message_time {
            if t.elapsed().as_secs_f32() > 3.0 {
                self.status_message = None;
                self.message_time = None;
            }
        }

        // -- Header --
        screen.draw_rect(
            Rect::new(0, 0, SCREEN_WIDTH, HEADER_HEIGHT as u32),
            Some(Color::RGBA(14, 14, 20, 220)),
            true,
            0,
            None,
        );
        screen.draw_glow_line(
            0,
            0,
            SCREEN_WIDTH as i32 - 1,
            Color::RGBA(100, 180, 255, 80),
            3,
            1,
        );
        screen.draw_text_glow(
            "WiFi",
            12,
            8,
            theme.accent,
            theme.glow_primary,
            20,
            true,
            None,
        );

        let card_w = SCREEN_WIDTH - 24;
        let start_y = CONTENT_TOP + 12;

        // -- Row 0: Status card --
        {
            let y = start_y;
            let is_sel = self.selected_row == 0;
            let bg = if is_sel {
                theme.card_highlight
            } else {
                theme.card_bg
            };
            let border = if is_sel { theme.accent } else { theme.card_border };

            screen.draw_card(
                Rect::new(12, y, card_w, STATUS_ROW_H as u32),
                Some(bg),
                Some(border),
                CARD_RADIUS,
                false,
            );

            match &self.status {
                WifiStatus::Connected { ssid, signal } => {
                    screen.draw_text(
                        &format!("Connected: {ssid}"),
                        24,
                        y + 8,
                        Some(theme.text),
                        14,
                        true,
                        Some(card_w - 60),
                    );
                    screen.draw_text(
                        &format!("Signal: {signal}%"),
                        24,
                        y + 28,
                        Some(theme.text_dim),
                        12,
                        false,
                        None,
                    );
                    screen.draw_circle(
                        card_w as i32,
                        y + STATUS_ROW_H / 2,
                        5,
                        theme.positive,
                    );
                }
                WifiStatus::Disconnected => {
                    screen.draw_text(
                        "WiFi Disconnected",
                        24,
                        y + 8,
                        Some(theme.text),
                        14,
                        true,
                        None,
                    );
                    screen.draw_text(
                        "Select a network below to connect",
                        24,
                        y + 28,
                        Some(theme.text_dim),
                        12,
                        false,
                        None,
                    );
                    screen.draw_circle(
                        card_w as i32,
                        y + STATUS_ROW_H / 2,
                        5,
                        theme.negative,
                    );
                }
                WifiStatus::Unknown => {
                    screen.draw_text(
                        "WiFi Status Unknown",
                        24,
                        y + 14,
                        Some(theme.text_dim),
                        14,
                        false,
                        None,
                    );
                }
            }
        }

        // -- Section label --
        let section_y = start_y + STATUS_ROW_H + MARGIN;
        screen.draw_text(
            "Available Networks",
            12,
            section_y,
            Some(theme.text_dim),
            12,
            true,
            None,
        );

        // -- Network list --
        let list_start_y = section_y + 20;
        let available_h = CONTENT_BOTTOM - 28 - list_start_y;
        let visible_count = (available_h / (NET_ROW_H + MARGIN)).max(1) as usize;

        for (vi, i) in (self.scroll_offset..)
            .take(visible_count)
            .enumerate()
        {
            if i >= self.networks.len() {
                break;
            }
            let network = &self.networks[i];
            let y = list_start_y + vi as i32 * (NET_ROW_H + MARGIN);

            let is_sel = self.selected_row == i + 1;
            let bg = if is_sel {
                theme.card_highlight
            } else {
                theme.card_bg
            };
            let border = if is_sel { theme.accent } else { theme.card_border };

            screen.draw_card(
                Rect::new(12, y, card_w, NET_ROW_H as u32),
                Some(bg),
                Some(border),
                CARD_RADIUS,
                false,
            );

            // SSID
            screen.draw_text(
                &network.ssid,
                24,
                y + 6,
                Some(theme.text),
                14,
                true,
                Some(300),
            );

            // Security + signal
            let info = format!("{}  Signal: {}%", network.security, network.signal);
            screen.draw_text(&info, 24, y + 26, Some(theme.text_dim), 11, false, None);

            // Signal bar
            let bar_w: u32 = 50;
            let bar_x = card_w as i32 - 16 - bar_w as i32;
            let bar_color = if network.signal > 50 {
                theme.positive
            } else {
                theme.text_warning
            };
            screen.draw_progress_bar(
                Rect::new(bar_x, y + 14, bar_w, 6),
                network.signal as f32 / 100.0,
                Some(bar_color),
                Some(Color::RGBA(40, 40, 60, 180)),
                3,
            );

            // SAVED pill
            if network.is_saved {
                let pill_x = bar_x - 64;
                screen.draw_pill(
                    "SAVED",
                    pill_x,
                    y + 8,
                    theme.positive,
                    Color::RGB(20, 20, 30),
                    11,
                );
            }
        }

        // Scroll indicators
        if self.scroll_offset > 0 {
            screen.draw_text(
                "^",
                (SCREEN_WIDTH / 2) as i32,
                list_start_y - 14,
                Some(theme.text_dim),
                12,
                false,
                None,
            );
        }
        if self.scroll_offset + visible_count < self.networks.len() {
            let bottom_y = list_start_y + visible_count as i32 * (NET_ROW_H + MARGIN) - 4;
            screen.draw_text(
                "v",
                (SCREEN_WIDTH / 2) as i32,
                bottom_y,
                Some(theme.text_dim),
                12,
                false,
                None,
            );
        }

        // -- Status message --
        if let Some(msg) = &self.status_message {
            let msg_y = SCREEN_HEIGHT as i32 - FOOTER_HEIGHT - 24;
            let mw = screen.get_text_width(msg, 13, false);
            screen.draw_text(
                msg,
                (SCREEN_WIDTH as i32 - mw as i32) / 2,
                msg_y,
                Some(theme.text_accent),
                13,
                false,
                None,
            );
        }

        // -- Footer --
        let footer_y = SCREEN_HEIGHT as i32 - FOOTER_HEIGHT;
        screen.draw_rect(
            Rect::new(0, footer_y, SCREEN_WIDTH, FOOTER_HEIGHT as u32),
            Some(Color::RGBA(14, 14, 20, 220)),
            true,
            0,
            None,
        );
        screen.draw_glow_line(
            footer_y,
            0,
            SCREEN_WIDTH as i32 - 1,
            Color::RGBA(100, 180, 255, 50),
            2,
            -1,
        );

        let mut fx = 12;
        let a_hint = if self.selected_row == 0 {
            if matches!(self.status, WifiStatus::Connected { .. }) {
                "Disconnect"
            } else {
                "---"
            }
        } else {
            "Connect"
        };
        let w = screen.draw_button_hint("A", a_hint, fx, footer_y + 8, Some(theme.btn_a), 12);
        fx += w as i32 + 12;
        let w = screen.draw_button_hint("B", "Back", fx, footer_y + 8, Some(theme.btn_b), 12);
        fx += w as i32 + 12;
        screen.draw_button_hint("Y", "Rescan", fx, footer_y + 8, Some(theme.btn_y), 12);

        // -- Password input overlay (drawn on top of everything) --
        self.password_input.draw(screen);
    }
}
