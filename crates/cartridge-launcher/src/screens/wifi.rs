use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use cartridge_core::theme::{UiStyle, style_of};
use cartridge_core::ui::text_input::{TextInput, TextInputResult};
use cartridge_net::wifi::{WifiNetwork, WifiStatus};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use super::{LauncherScreen, ScreenAction, ScreenContext};
use crate::neo::{self, Chip, Hint};
use crate::ui_constants::*;

const NEO_STATUS_H: i32 = 60;
const NEO_NET_ROW_H: i32 = 48;
const NEO_LIST_Y: i32 = neo::CONTENT_Y + 12 + NEO_STATUS_H + 34;
const NEO_VISIBLE: usize = ((neo::FOOTER_Y - 30 - NEO_LIST_Y) / NEO_NET_ROW_H) as usize;

pub struct WifiScreen {
    selected_row: usize,
    networks: Vec<WifiNetwork>,
    scan_error: Option<String>,
    scan_result: Option<Receiver<Result<Vec<WifiNetwork>, String>>>,
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
            scan_error: None,
            scan_result: None,
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
        if self.scan_result.is_some() {
            return;
        }
        self.scan_error = None;
        let (sender, receiver) = mpsc::channel();
        self.scan_result = Some(receiver);
        std::thread::spawn(move || {
            let result = cartridge_net::wifi::WifiManager::new().scan_networks();
            let _ = sender.send(result);
        });
    }

    fn poll_scan(&mut self, ctx: &ScreenContext) {
        let completed = self.scan_result.as_ref().map(Receiver::try_recv);
        match completed {
            Some(Ok(Ok(networks))) => {
                self.networks = networks;
                self.scan_error = None;
                self.scan_result = None;
                self.status_message = None;
                self.message_time = None;
                self.selected_row = self.selected_row.min(self.total_rows() - 1);
                self.status = ctx.wifi_manager.status();
            }
            Some(Ok(Err(error))) => {
                self.networks.clear();
                self.scan_error = Some(error);
                self.scan_result = None;
                self.status_message = None;
                self.message_time = None;
                self.selected_row = 0;
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.scan_result = None;
                self.scan_error = Some("Wi-Fi scanner stopped unexpectedly".to_string());
            }
            Some(Err(TryRecvError::Empty)) | None => {}
        }
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
    fn is_loading(&self) -> bool {
        self.scan_result.is_some()
    }

    fn handle_input(&mut self, events: &[InputEvent], ctx: &mut ScreenContext) -> ScreenAction {
        if !self.scanned {
            self.scanned = true;
            self.refresh(ctx);
        }
        self.poll_scan(ctx);

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
                            let ssid = network.ssid.clone();
                            let is_open = network.security == "--" || network.security.is_empty();

                            if network.is_saved {
                                // Try to connect with saved password
                                match ctx.wifi_manager.connect(&ssid) {
                                    Ok(()) => {
                                        self.set_message(format!("Connected to {ssid}"));
                                        self.refresh(ctx);
                                    }
                                    Err(_) => {
                                        // Saved password missing or failed — ask for password
                                        let label = format!("Password for {}", ssid);
                                        self.password_input.show(&label);
                                        self.connecting_ssid = Some(ssid);
                                    }
                                }
                            } else if is_open {
                                match ctx.wifi_manager.connect_with_password(&ssid, "") {
                                    Ok(()) => self.set_message(format!("Connected to {ssid}")),
                                    Err(e) => self.set_message(format!("Error: {e}")),
                                }
                                self.refresh(ctx);
                            } else {
                                // Password required — show on-screen keyboard
                                let label = format!("Password for {}", ssid);
                                self.password_input.show(&label);
                                self.connecting_ssid = Some(ssid);
                            }
                        }
                    }
                }
                Button::Y => {
                    self.refresh(ctx);
                    self.set_message("Scanning...".to_string());
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
            let visible_count = if style_of(&ctx.settings.theme_id) == UiStyle::Neo {
                NEO_VISIBLE.max(1)
            } else {
                (available / (NET_ROW_H + MARGIN)).max(1) as usize
            };

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

    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext) {
        let theme = screen.theme;

        // Clear status message after 3 seconds
        if let Some(t) = self.message_time {
            if t.elapsed().as_secs_f32() > 3.0 {
                self.status_message = None;
                self.message_time = None;
            }
        }

        if neo::is_neo(theme) {
            self.render_neo(screen, ctx);
            return;
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
            let border = if is_sel {
                theme.accent
            } else {
                theme.card_border
            };

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
                    screen.draw_circle(card_w as i32, y + STATUS_ROW_H / 2, 5, theme.positive);
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
                    screen.draw_circle(card_w as i32, y + STATUS_ROW_H / 2, 5, theme.negative);
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

        if self.networks.is_empty() {
            let message = if self.scan_result.is_some() {
                "Waiting for nearby networks..."
            } else {
                self.scan_error
                    .as_deref()
                    .unwrap_or("No networks found. Check the adapter and rescan.")
            };
            screen.draw_text(
                if self.scan_result.is_some() { "SCANNING" } else { "NO NETWORKS" },
                24,
                list_start_y + 14,
                Some(theme.text),
                16,
                true,
                None,
            );
            for (line, text) in neo::wrap_lines(screen, message, 12, false, card_w - 48, 3)
                .iter()
                .enumerate()
            {
                screen.draw_text(
                    text,
                    24,
                    list_start_y + 44 + line as i32 * 18,
                    Some(theme.text_dim),
                    12,
                    false,
                    None,
                );
            }
        }

        for (vi, i) in (self.scroll_offset..).take(visible_count).enumerate() {
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
            let border = if is_sel {
                theme.accent
            } else {
                theme.card_border
            };

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

impl WifiScreen {
    fn render_neo(&self, screen: &mut Screen, ctx: &ScreenContext) {
        let theme = screen.theme;
        neo::draw_header(screen, "WIFI", "", Some(&ctx.sysinfo));
        neo::draw_bar(screen);

        let right = SCREEN_WIDTH as i32 - neo::MARGIN_X;
        let width = SCREEN_WIDTH - neo::MARGIN_X as u32 * 2;

        // Status row.
        let y = neo::CONTENT_Y + 12;
        let is_sel = self.selected_row == 0;
        if is_sel {
            screen.fill(
                Rect::new(neo::MARGIN_X, y, width, NEO_STATUS_H as u32),
                theme.card_bg,
            );
            screen.fill(
                Rect::new(neo::MARGIN_X, y, 4, NEO_STATUS_H as u32),
                theme.accent,
            );
        }
        screen.fill(
            Rect::new(neo::MARGIN_X, y + NEO_STATUS_H - 1, width, 1),
            theme.border,
        );
        let tx = neo::MARGIN_X + 16;
        let (title, sub, dot) = match &self.status {
            WifiStatus::Connected { ssid, signal } => (
                ssid.to_uppercase(),
                format!("Connected · signal {signal}%"),
                Some(theme.text),
            ),
            WifiStatus::Disconnected => (
                "DISCONNECTED".to_string(),
                "Select a network below to connect".to_string(),
                Some(theme.accent),
            ),
            WifiStatus::Unknown => ("WIFI STATUS UNKNOWN".to_string(), String::new(), None),
        };
        neo::display_at_baseline(screen, &title, tx, y + 30, theme.text, 26);
        screen.draw_text(
            &sub,
            tx,
            y + 36,
            Some(theme.text_dim),
            neo::LABEL_SIZE,
            false,
            Some(width - 80),
        );
        if let Some(c) = dot {
            screen.fill(Rect::new(right - 24, y + NEO_STATUS_H / 2 - 5, 10, 10), c);
        }

        // Section label.
        let label_y = y + NEO_STATUS_H + 12;
        screen.draw_text(
            "AVAILABLE NETWORKS",
            neo::MARGIN_X,
            label_y,
            Some(theme.text_dim),
            neo::LABEL_SIZE,
            false,
            None,
        );

        if self.networks.is_empty() {
            let message = if self.scan_result.is_some() {
                "Waiting for nearby networks..."
            } else {
                self.scan_error
                    .as_deref()
                    .unwrap_or("No networks found. Check the adapter and rescan.")
            };
            let title = if self.scan_result.is_some() { "SCANNING" } else { "NO NETWORKS" };
            neo::display_at_baseline(screen, title, tx, NEO_LIST_Y + 32, theme.text, 24);
            for (line, text) in neo::wrap_lines(screen, message, 13, false, width - 32, 3)
                .iter()
                .enumerate()
            {
                screen.draw_text(
                    text,
                    tx,
                    NEO_LIST_Y + 48 + line as i32 * 20,
                    Some(theme.text_dim),
                    13,
                    false,
                    None,
                );
            }
        }

        // Network rows.
        for (vi, i) in (self.scroll_offset..).take(NEO_VISIBLE.max(1)).enumerate() {
            if i >= self.networks.len() {
                break;
            }
            let network = &self.networks[i];
            let ry = NEO_LIST_Y + vi as i32 * NEO_NET_ROW_H;
            let is_sel = self.selected_row == i + 1;
            if is_sel {
                screen.fill(
                    Rect::new(neo::MARGIN_X, ry, width, NEO_NET_ROW_H as u32),
                    theme.card_bg,
                );
                screen.fill(
                    Rect::new(neo::MARGIN_X, ry, 4, NEO_NET_ROW_H as u32),
                    theme.accent,
                );
            }
            screen.fill(
                Rect::new(neo::MARGIN_X, ry + NEO_NET_ROW_H - 1, width, 1),
                theme.border,
            );

            let color = if is_sel { theme.text } else { theme.text_dim };
            screen.draw_text(
                &network.ssid,
                tx,
                ry + 8,
                Some(color),
                14,
                is_sel,
                Some(300),
            );
            let sec = if network.security == "--" || network.security.is_empty() {
                "OPEN".to_string()
            } else {
                network.security.to_uppercase()
            };
            let info = format!("{sec} · SIGNAL {}%", network.signal);
            screen.draw_text(
                &info,
                tx,
                ry + 28,
                Some(theme.text_dim),
                neo::LABEL_SIZE,
                false,
                None,
            );

            // Signal: four bars, filled by strength.
            let bars = ((network.signal as i32 + 24) / 25).clamp(0, 4);
            for b in 0..4 {
                let bh = 4 + b * 4;
                let bx = right - 4 * 8 + b * 8;
                let c = if b < bars { theme.text } else { theme.border };
                screen.fill(
                    Rect::new(bx, ry + NEO_NET_ROW_H / 2 + 8 - bh, 5, bh as u32),
                    c,
                );
            }
            if network.is_saved {
                let w = screen.get_text_width("SAVED", neo::LABEL_SIZE, false) as i32 + 16;
                neo::draw_chip(screen, "Saved", right - 40 - w, ry + 14, Chip::OutlineDim);
            }
        }

        if self.scroll_offset + NEO_VISIBLE < self.networks.len() {
            let more = format!(
                "{} MORE",
                self.networks.len() - self.scroll_offset - NEO_VISIBLE
            );
            neo::text_right(
                screen,
                &more,
                right,
                neo::FOOTER_Y - 16,
                theme.text_muted,
                neo::LABEL_SIZE,
                false,
            );
        }

        if let Some(msg) = &self.status_message {
            screen.draw_text(
                &msg.to_uppercase(),
                neo::MARGIN_X,
                neo::FOOTER_Y - 26,
                Some(theme.accent),
                neo::LABEL_SIZE,
                false,
                Some(width - 120),
            );
        }

        let a_hint = if self.selected_row == 0 {
            if matches!(self.status, WifiStatus::Connected { .. }) {
                "Disconnect"
            } else {
                "---"
            }
        } else {
            "Connect"
        };
        neo::draw_footer(
            screen,
            &[Hint::a(a_hint), Hint::b("Back"), Hint::y("Rescan")],
        );

        self.password_input.draw(screen);
    }
}
