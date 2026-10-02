use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use cartridge_core::theme::{UiStyle, style_of};
use cartridge_core::ui::text_input::{TextInput, TextInputResult};
use cartridge_net::wifi::WifiStatus;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
#[path = "wifi_jobs.rs"]
mod jobs;
use jobs::{Operation, SharedJobs, View};

use super::{LauncherScreen, ScreenAction, ScreenContext};
use crate::neo::{self, Chip, Hint};
use crate::ui_constants::*;

const NEO_STATUS_H: i32 = 60;
const NEO_NET_ROW_H: i32 = 48;
const NEO_LIST_Y: i32 = neo::CONTENT_Y + 12 + NEO_STATUS_H + 34;
const NEO_DIAGNOSTIC_H: i32 = 76;

pub struct WifiScreen {
    selected_row: usize,
    jobs: SharedJobs,
    view: View,
    revision: Option<u64>,
    initialized: bool,
    input_error: Option<String>,
    scroll_offset: usize,
    password_input: TextInput,
    connecting_ssid: Option<String>,
}

const NET_ROW_H: i32 = 48;
const STATUS_ROW_H: i32 = 52;

impl WifiScreen {
    pub fn new() -> Self {
        Self::with_jobs(jobs::shared_jobs())
    }

    fn with_jobs(jobs: SharedJobs) -> Self {
        let mut password_input = TextInput::new("");
        password_input.masked = true;
        Self {
            selected_row: 0,
            jobs,
            view: View::default(),
            revision: None,
            initialized: false,
            input_error: None,
            scroll_offset: 0,
            password_input,
            connecting_ssid: None,
        }
    }

    fn poll_jobs(&mut self) -> bool {
        // Never wait for another screen. Workers themselves never hold this
        // mutex; they only send events through the retained channel.
        let Ok(mut jobs) = self.jobs.try_lock() else {
            return false;
        };
        let inherited = jobs.is_busy();
        jobs.poll();
        if !self.initialized {
            self.initialized = true;
            if !inherited {
                jobs.start(Operation::Refresh);
            }
        }
        if self.revision == Some(jobs.revision) {
            return false;
        }
        self.revision = Some(jobs.revision);
        // Signal changes can reorder the list. Keep the selected SSID, leave
        // the status row at zero, and clamp only if the network disappeared.
        self.selected_row = self
            .selected_row
            .checked_sub(1)
            .and_then(|index| self.view.networks.get(index))
            .and_then(|selected| {
                jobs.view
                    .networks
                    .iter()
                    .position(|network| network.ssid == selected.ssid)
            })
            .map(|index| index + 1)
            .unwrap_or_else(|| self.selected_row.min(jobs.view.networks.len()));
        self.view = jobs.view.clone();
        true
    }

    fn start_operation(&mut self, operation: Operation) {
        let Ok(mut jobs) = self.jobs.try_lock() else {
            return;
        };
        if jobs.start(operation) {
            self.input_error = None;
        }
        self.revision = Some(jobs.revision);
        self.view = jobs.view.clone();
    }

    fn pending_label(&self) -> Option<&'static str> {
        if !self.initialized {
            Some("Scanning Wi-Fi...")
        } else {
            self.view.pending.map(|pending| pending.label())
        }
    }

    fn diagnostic(&self) -> Option<&str> {
        self.input_error
            .as_deref()
            .or(self.view.message.as_deref())
            .or(self.view.scan_error.as_deref())
    }

    fn total_rows(&self) -> usize {
        1 + self.view.networks.len()
    }

    fn visible_neo_rows(&self) -> usize {
        let diagnostic_height = if self.diagnostic().is_some() {
            NEO_DIAGNOSTIC_H
        } else {
            0
        };
        ((neo::FOOTER_Y - 30 - NEO_LIST_Y - diagnostic_height) / NEO_NET_ROW_H).max(1) as usize
    }

    fn visible_legacy_rows(&self) -> usize {
        let list_start = CONTENT_TOP + 12 + STATUS_ROW_H + MARGIN + 20;
        let diagnostic_height = if self.diagnostic().is_some() {
            NEO_DIAGNOSTIC_H
        } else {
            0
        };
        ((CONTENT_BOTTOM - 28 - list_start - diagnostic_height) / (NET_ROW_H + MARGIN)).max(1)
            as usize
    }

    fn keep_selection_visible(&mut self, ctx: &ScreenContext) {
        if self.selected_row == 0 {
            self.scroll_offset = 0;
            return;
        }
        let net_idx = self.selected_row - 1;
        let visible = if style_of(&ctx.settings.theme_id) == UiStyle::Neo {
            self.visible_neo_rows()
        } else {
            self.visible_legacy_rows()
        };
        self.scroll_offset = self.scroll_offset.min(net_idx);
        if net_idx >= self.scroll_offset + visible {
            self.scroll_offset = net_idx + 1 - visible;
        }
    }

    fn show_password(&mut self, ssid: String) {
        self.password_input.show(&format!("Password for {ssid}"));
        self.connecting_ssid = Some(ssid);
    }

    fn activate_selection(&mut self) {
        if self.is_loading() {
            return;
        }
        if self.selected_row == 0 {
            if matches!(self.view.status, WifiStatus::Connected { .. }) {
                self.start_operation(Operation::Disconnect);
            }
            return;
        }
        let Some(network) = self.view.networks.get(self.selected_row - 1) else {
            return;
        };
        let ssid = network.ssid.clone();
        let is_open = network.security == "--" || network.security.is_empty();
        if !network.is_saved_only
            && !is_open
            && self.view.password_retry_ssid.as_ref() == Some(&ssid)
        {
            self.show_password(ssid);
        } else if network.is_saved {
            self.start_operation(Operation::ConnectSaved {
                ssid,
                offer_password: !network.is_saved_only && !is_open,
            });
        } else if is_open {
            self.start_operation(Operation::ConnectPassword {
                ssid,
                password: String::new(),
            });
        } else {
            self.show_password(ssid);
        }
    }
}

impl LauncherScreen for WifiScreen {
    fn update(&mut self, ctx: &mut ScreenContext) -> bool {
        let changed = self.poll_jobs();
        if changed {
            self.keep_selection_visible(ctx);
        }
        changed
    }

    fn is_loading(&self) -> bool {
        !self.initialized || self.view.pending.is_some()
    }

    fn handle_input(&mut self, events: &[InputEvent], ctx: &mut ScreenContext) -> ScreenAction {
        if self.password_input.visible {
            for ie in events {
                match self.password_input.handle_input(ie) {
                    TextInputResult::Submitted(password) => {
                        // TextInput retains a submitted copy; discard it along
                        // with its text as soon as the password enters the job.
                        self.password_input = TextInput::new("");
                        self.password_input.masked = true;
                        if let Some(ssid) = self.connecting_ssid.take() {
                            if password.is_empty() {
                                self.input_error = Some("Password cannot be empty".into());
                                self.show_password(ssid);
                            } else {
                                self.start_operation(Operation::ConnectPassword { ssid, password });
                            }
                        }
                        break;
                    }
                    TextInputResult::Cancelled => {
                        self.connecting_ssid = None;
                        self.password_input = TextInput::new("");
                        self.password_input.masked = true;
                        break;
                    }
                    TextInputResult::Pending => {}
                }
            }
            self.keep_selection_visible(ctx);
            return ScreenAction::None;
        }
        for ie in events {
            if !matches!(ie.action, InputAction::Press | InputAction::Repeat) {
                continue;
            }
            match ie.button {
                Button::B => return ScreenAction::Pop,
                Button::DpadDown if self.selected_row + 1 < self.total_rows() => {
                    self.selected_row += 1
                }
                Button::DpadUp if self.selected_row > 0 => self.selected_row -= 1,
                Button::A if ie.action == InputAction::Press => {
                    self.activate_selection();
                    if self.password_input.visible {
                        break;
                    }
                }
                Button::Y if ie.action == InputAction::Press && !self.is_loading() => {
                    self.start_operation(Operation::Refresh);
                }
                Button::Select => return ScreenAction::ShowOverlay,
                _ => {}
            }
        }
        self.keep_selection_visible(ctx);
        ScreenAction::None
    }

    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext) {
        let theme = screen.theme;

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

            match &self.view.status {
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
                        self.pending_label()
                            .unwrap_or(&format!("Signal: {signal}%")),
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
                        self.pending_label()
                            .unwrap_or("Select a network below to connect"),
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
                        self.pending_label().unwrap_or("WiFi Status Unknown"),
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
            if self.view.scan_error.is_some() && !self.view.networks.is_empty() {
                "Saved Networks (scan unavailable)"
            } else {
                "Available Networks"
            },
            12,
            section_y,
            Some(theme.text_dim),
            12,
            true,
            None,
        );

        // -- Network list --
        let list_start_y = section_y + 20;
        let visible_count = self.visible_legacy_rows();

        if self.view.networks.is_empty() {
            let message = if self.is_loading() {
                self.pending_label().unwrap_or("Waiting for Wi-Fi...")
            } else {
                self.view
                    .scan_error
                    .as_deref()
                    .unwrap_or("No networks found. Check the adapter and rescan.")
            };
            screen.draw_text(
                if self.is_loading() {
                    self.pending_label().unwrap_or("Wi-Fi")
                } else {
                    "NO NETWORKS"
                },
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
            if i >= self.view.networks.len() {
                break;
            }
            let network = &self.view.networks[i];
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
            let info = if network.is_saved_only {
                "Saved profile - not in scan".to_string()
            } else {
                format!("{}  Signal: {}%", network.security, network.signal)
            };
            screen.draw_text(&info, 24, y + 26, Some(theme.text_dim), 11, false, None);

            // Signal bar
            let bar_w: u32 = 50;
            let bar_x = card_w as i32 - 16 - bar_w as i32;
            let bar_color = if network.signal > 50 {
                theme.positive
            } else {
                theme.text_warning
            };
            if !network.is_saved_only {
                screen.draw_progress_bar(
                    Rect::new(bar_x, y + 14, bar_w, 6),
                    network.signal as f32 / 100.0,
                    Some(bar_color),
                    Some(Color::RGBA(40, 40, 60, 180)),
                    3,
                );
            }

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
        if self.scroll_offset + visible_count < self.view.networks.len() {
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

        // -- Persistent connection/scan diagnostics --
        if let Some(msg) = self.diagnostic() {
            for (line, text) in neo::wrap_lines(screen, msg, 11, false, card_w - 24, 3)
                .iter()
                .enumerate()
            {
                screen.draw_text(
                    text,
                    24,
                    SCREEN_HEIGHT as i32 - FOOTER_HEIGHT - 62 + line as i32 * 17,
                    Some(theme.text_accent),
                    11,
                    false,
                    Some(card_w - 24),
                );
            }
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
        let a_hint = if self.is_loading() {
            "Wait"
        } else if self.selected_row == 0 {
            if matches!(self.view.status, WifiStatus::Connected { .. }) {
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
        screen.draw_button_hint(
            "Y",
            if self.is_loading() { "Wait" } else { "Rescan" },
            fx,
            footer_y + 8,
            Some(theme.btn_y),
            12,
        );

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
        let (title, sub, dot) = match &self.view.status {
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
            self.pending_label().unwrap_or(&sub),
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
            if self.view.scan_error.is_some() && !self.view.networks.is_empty() {
                "SAVED NETWORKS · SCAN UNAVAILABLE"
            } else {
                "AVAILABLE NETWORKS"
            },
            neo::MARGIN_X,
            label_y,
            Some(theme.text_dim),
            neo::LABEL_SIZE,
            false,
            None,
        );

        if self.view.networks.is_empty() {
            let message = if self.is_loading() {
                self.pending_label().unwrap_or("Waiting for Wi-Fi...")
            } else {
                self.view
                    .scan_error
                    .as_deref()
                    .unwrap_or("No networks found. Check the adapter and rescan.")
            };
            let title = if self.is_loading() {
                self.pending_label().unwrap_or("Wi-Fi")
            } else {
                "NO NETWORKS"
            };
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
        let visible_count = self.visible_neo_rows();
        for (vi, i) in (self.scroll_offset..).take(visible_count).enumerate() {
            if i >= self.view.networks.len() {
                break;
            }
            let network = &self.view.networks[i];
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
            let info = if network.is_saved_only {
                "SAVED PROFILE · NOT IN SCAN".to_string()
            } else {
                format!("{sec} · SIGNAL {}%", network.signal)
            };
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
            if !network.is_saved_only {
                for b in 0..4 {
                    let bh = 4 + b * 4;
                    let bx = right - 4 * 8 + b * 8;
                    let c = if b < bars { theme.text } else { theme.border };
                    screen.fill(
                        Rect::new(bx, ry + NEO_NET_ROW_H / 2 + 8 - bh, 5, bh as u32),
                        c,
                    );
                }
            }
            if network.is_saved {
                let w = screen.get_text_width("SAVED", neo::LABEL_SIZE, false) as i32 + 16;
                neo::draw_chip(screen, "Saved", right - 40 - w, ry + 14, Chip::OutlineDim);
            }
        }

        if self.scroll_offset + visible_count < self.view.networks.len() {
            let more = format!(
                "{} MORE",
                self.view.networks.len() - self.scroll_offset - visible_count
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

        if let Some(msg) = self.diagnostic() {
            let label = if self.view.message.is_some() || self.input_error.is_some() {
                "CONNECTION"
            } else {
                "SCAN"
            };
            screen.draw_text(
                label,
                neo::MARGIN_X,
                neo::FOOTER_Y - 68,
                Some(theme.accent),
                neo::LABEL_SIZE,
                true,
                None,
            );
            for (line, text) in neo::wrap_lines(screen, msg, 11, false, width, 3)
                .iter()
                .enumerate()
            {
                screen.draw_text(
                    text,
                    neo::MARGIN_X,
                    neo::FOOTER_Y - 51 + line as i32 * 15,
                    Some(theme.accent),
                    11,
                    false,
                    None,
                );
            }
        }

        let a_hint = if self.is_loading() {
            "Wait"
        } else if self.selected_row == 0 {
            if matches!(self.view.status, WifiStatus::Connected { .. }) {
                "Disconnect"
            } else {
                "---"
            }
        } else {
            "Connect"
        };
        neo::draw_footer(
            screen,
            &[
                Hint::a(a_hint),
                Hint::b("Back"),
                Hint::y(if self.is_loading() { "Wait" } else { "Rescan" }),
            ],
        );

        self.password_input.draw(screen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cartridge_net::wifi::WifiNetwork;
    use jobs::{Event, WifiJobs};
    use std::sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    };
    use std::time::{Duration, Instant};

    enum Command {
        Emit(Event, Sender<()>),
        Stop(Sender<()>),
    }
    struct Ticket {
        operation: Operation,
        thread: std::thread::ThreadId,
        commands: Sender<Command>,
    }
    impl Ticket {
        fn emit(&self, event: Event) {
            let (tx, rx) = mpsc::channel();
            self.commands.send(Command::Emit(event, tx)).unwrap();
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        fn stop(self) {
            let (tx, rx) = mpsc::channel();
            self.commands.send(Command::Stop(tx)).unwrap();
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
        }
    }
    fn controlled_jobs() -> (SharedJobs, Receiver<Ticket>) {
        let (started, receiver) = mpsc::channel();
        let worker = Arc::new(move |operation, events: Sender<Event>| {
            let (commands, controls) = mpsc::channel();
            started
                .send(Ticket {
                    operation,
                    thread: std::thread::current().id(),
                    commands,
                })
                .unwrap();
            // The worker remains blocked until the test releases it. UI calls
            // must return while it is still waiting, including across reopen.
            while let Ok(command) = controls.recv_timeout(Duration::from_secs(5)) {
                match command {
                    Command::Emit(event, ack) => {
                        let done = matches!(event, Event::Snapshot { .. });
                        events.send(event).unwrap();
                        let _ = ack.send(());
                        if done {
                            break;
                        }
                    }
                    Command::Stop(ack) => {
                        drop(events);
                        let _ = ack.send(());
                        return;
                    }
                }
            }
        });
        (Arc::new(Mutex::new(WifiJobs::new(worker))), receiver)
    }
    fn next(receiver: &Receiver<Ticket>) -> Ticket {
        let ticket = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_ne!(ticket.thread, std::thread::current().id());
        ticket
    }
    fn network(saved_only: bool) -> WifiNetwork {
        WifiNetwork {
            ssid: "Home".into(),
            signal: 75,
            security: "WPA2".into(),
            is_saved: true,
            is_saved_only: saved_only,
        }
    }
    fn snapshot() -> Event {
        Event::Snapshot {
            status: WifiStatus::Disconnected,
            networks: vec![network(false)],
            scan_error: None,
        }
    }
    fn outcome(result: Result<(), String>, offer_password: bool) -> Event {
        Event::Outcome {
            result,
            success: "Connected to Home".into(),
            password_retry_ssid: offer_password.then(|| "Home".into()),
        }
    }
    fn press(button: Button) -> InputEvent {
        InputEvent {
            button,
            action: InputAction::Press,
        }
    }

    // Cleanup the existing test_context helper's isolated storage.
    struct Context(ScreenContext);
    impl Context {
        fn new() -> Self {
            Self(super::super::test_context())
        }
    }
    impl Drop for Context {
        fn drop(&mut self) {
            let root = self.0.storage.data_dir.parent().unwrap().parent().unwrap();
            let _ = std::fs::remove_dir_all(root);
        }
    }
    fn ready(jobs: SharedJobs, rx: &Receiver<Ticket>, ctx: &mut ScreenContext) -> WifiScreen {
        let mut screen = WifiScreen::with_jobs(jobs);
        assert!(screen.update(ctx));
        let scan = next(rx);
        assert!(matches!(scan.operation, Operation::Refresh));
        scan.emit(snapshot());
        assert!(screen.update(ctx));
        assert!(!screen.is_loading());
        screen
    }

    #[test]
    fn delayed_scan_is_nonblocking_and_survives_close_reopen_without_duplicates() {
        let (jobs, rx) = controlled_jobs();
        let mut ctx = Context::new();
        let mut screen = WifiScreen::with_jobs(jobs.clone());
        assert!(screen.update(&mut ctx.0));
        let scan = next(&rx);
        assert!(matches!(scan.operation, Operation::Refresh));
        let start = Instant::now();
        for _ in 0..1000 {
            assert!(!screen.update(&mut ctx.0));
            screen.handle_input(&[press(Button::A), press(Button::Y)], &mut ctx.0);
        }
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(screen.pending_label(), Some("Scanning Wi-Fi..."));
        assert!(matches!(
            screen.handle_input(&[press(Button::Select)], &mut ctx.0),
            ScreenAction::ShowOverlay
        ));
        assert!(matches!(
            screen.handle_input(&[press(Button::B)], &mut ctx.0),
            ScreenAction::Pop
        ));
        drop(screen);
        let mut reopened = WifiScreen::with_jobs(jobs);
        assert!(reopened.update(&mut ctx.0));
        assert!(reopened.is_loading());
        assert!(rx.try_recv().is_err());
        scan.emit(snapshot());
        // No input is needed to apply a completion.
        assert!(reopened.update(&mut ctx.0));
        assert!(!reopened.is_loading());
        assert_eq!(reopened.view.networks[0].ssid, "Home");
        for _ in 0..1000 {
            assert!(!reopened.update(&mut ctx.0));
        }
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn every_operation_remains_single_flight_until_final_refresh() {
        let (jobs, rx) = controlled_jobs();
        let mut ctx = Context::new();
        let mut screen = ready(jobs.clone(), &rx, &mut ctx.0);
        let operations = [
            Operation::Refresh,
            Operation::ConnectSaved {
                ssid: "Home".into(),
                offer_password: true,
            },
            Operation::ConnectPassword {
                ssid: "Home".into(),
                password: "test-secret".into(),
            },
            Operation::ConnectPassword {
                ssid: "Cafe".into(),
                password: String::new(),
            },
            Operation::Disconnect,
        ];
        for operation in operations {
            screen.start_operation(operation);
            let ticket = next(&rx);
            assert!(screen.is_loading());
            for _ in 0..100 {
                screen.start_operation(Operation::Refresh);
                screen.start_operation(Operation::Disconnect);
                assert!(!screen.update(&mut ctx.0));
            }
            assert!(rx.try_recv().is_err());
            if !matches!(ticket.operation, Operation::Refresh) {
                ticket.emit(outcome(Ok(()), false));
                assert!(screen.update(&mut ctx.0));
                assert_eq!(screen.pending_label(), Some("Refreshing Wi-Fi..."));
                assert!(!jobs.lock().unwrap().start(Operation::Refresh));
            }
            ticket.emit(snapshot());
            // Even an unconsumed completion owns the slot.
            assert!(!jobs.lock().unwrap().start(Operation::Disconnect));
            assert!(screen.update(&mut ctx.0));
            assert!(!screen.is_loading());
        }
    }

    #[test]
    fn late_saved_failure_survives_refresh_and_password_retry_is_explicit() {
        let (jobs, rx) = controlled_jobs();
        let mut ctx = Context::new();
        let mut screen = ready(jobs.clone(), &rx, &mut ctx.0);
        screen.selected_row = 1;
        screen.handle_input(&[press(Button::A), press(Button::A)], &mut ctx.0);
        let connect = next(&rx);
        assert!(
            matches!(&connect.operation, Operation::ConnectSaved { ssid, offer_password: true } if ssid == "Home")
        );
        assert_eq!(screen.pending_label(), Some("Connecting..."));
        drop(screen);
        connect.emit(outcome(Err("Authentication failed".into()), true));
        let mut screen = WifiScreen::with_jobs(jobs);
        assert!(screen.update(&mut ctx.0));
        assert!(
            screen
                .diagnostic()
                .unwrap()
                .contains("Authentication failed")
        );
        assert!(!screen.password_input.visible);
        assert!(screen.is_loading());
        connect.emit(snapshot());
        assert!(screen.update(&mut ctx.0));
        assert!(!screen.is_loading());
        assert!(
            screen
                .diagnostic()
                .unwrap()
                .contains("Authentication failed")
        );
        assert!(rx.try_recv().is_err());
        screen.handle_input(&[press(Button::Y)], &mut ctx.0);
        let scan = next(&rx);
        assert!(
            screen
                .diagnostic()
                .unwrap()
                .contains("Authentication failed")
        );
        scan.emit(snapshot());
        screen.update(&mut ctx.0);
        assert!(
            screen
                .diagnostic()
                .unwrap()
                .contains("Authentication failed")
        );
        screen.selected_row = 1;
        screen.handle_input(&[press(Button::A)], &mut ctx.0);
        assert!(screen.password_input.visible && screen.password_input.masked);
        // Empty passwords leave a usable retry dialog.
        screen.handle_input(&[press(Button::Start)], &mut ctx.0);
        assert!(screen.password_input.visible);
        assert_eq!(
            screen.input_error.as_deref(),
            Some("Password cannot be empty")
        );
        screen.password_input.text = "test-secret".into();
        screen.handle_input(&[press(Button::Start), press(Button::Start)], &mut ctx.0);
        assert!(!screen.password_input.visible);
        assert!(screen.password_input.text.is_empty());
        assert_eq!(screen.password_input.result, TextInputResult::Pending);
        let retry = next(&rx);
        assert!(
            matches!(&retry.operation, Operation::ConnectPassword { ssid, password } if ssid == "Home" && password == "test-secret")
        );
        assert!(rx.try_recv().is_err());
        retry.emit(outcome(Ok(()), false));
        retry.emit(snapshot());
        screen.update(&mut ctx.0);
        assert_eq!(screen.diagnostic(), Some("Connected to Home"));
        assert!(screen.view.password_retry_ssid.is_none());
    }

    #[test]
    fn worker_failure_keeps_cached_networks_and_allows_rescan() {
        let (jobs, rx) = controlled_jobs();
        let mut ctx = Context::new();
        let mut screen = ready(jobs, &rx, &mut ctx.0);
        screen.start_operation(Operation::Disconnect);
        let disconnect = next(&rx);
        disconnect.emit(outcome(Err("Device is busy".into()), false));
        screen.update(&mut ctx.0);
        disconnect.stop();
        assert!(screen.update(&mut ctx.0));
        assert!(!screen.is_loading());
        assert_eq!(screen.view.networks.len(), 1);
        assert!(screen.diagnostic().unwrap().contains("Device is busy"));
        assert!(screen.diagnostic().unwrap().contains("Press Y to retry"));
        screen.handle_input(&[press(Button::Y)], &mut ctx.0);
        let scan = next(&rx);
        scan.emit(snapshot());
        screen.update(&mut ctx.0);
        assert!(!screen.is_loading());
        assert!(screen.diagnostic().unwrap().contains("Device is busy"));
    }

    #[test]
    fn saved_only_network_failure_can_be_retried_without_a_password_dialog() {
        let (jobs, rx) = controlled_jobs();
        let mut ctx = Context::new();
        let mut screen = WifiScreen::with_jobs(jobs);
        screen.update(&mut ctx.0);
        let scan = next(&rx);
        scan.emit(Event::Snapshot {
            status: WifiStatus::Disconnected,
            networks: vec![network(true)],
            scan_error: Some("Scan unavailable".into()),
        });
        screen.update(&mut ctx.0);
        screen.selected_row = 1;
        screen.handle_input(
            &[InputEvent {
                button: Button::A,
                action: InputAction::Repeat,
            }],
            &mut ctx.0,
        );
        assert!(rx.try_recv().is_err());
        screen.handle_input(&[press(Button::A)], &mut ctx.0);
        let connect = next(&rx);
        assert!(matches!(
            connect.operation,
            Operation::ConnectSaved {
                offer_password: false,
                ..
            }
        ));
        connect.emit(outcome(Err("Out of range".into()), false));
        connect.emit(Event::Snapshot {
            status: WifiStatus::Disconnected,
            networks: vec![network(true)],
            scan_error: Some("Scan unavailable".into()),
        });
        screen.update(&mut ctx.0);
        assert_eq!(screen.diagnostic(), Some("Error: Out of range"));
        assert!(!screen.password_input.visible);
        screen.handle_input(&[press(Button::A)], &mut ctx.0);
        let retry = next(&rx);
        assert!(matches!(retry.operation, Operation::ConnectSaved { .. }));
        retry.emit(snapshot());
        screen.update(&mut ctx.0);
    }

    #[test]
    fn refresh_preserves_selected_ssid_status_row_and_clamps_removed_networks() {
        let (jobs, rx) = controlled_jobs();
        let mut ctx = Context::new();
        let mut screen = ready(jobs, &rx, &mut ctx.0);
        let refresh = |screen: &mut WifiScreen, ctx: &mut ScreenContext, entries: &[(&str, u8)]| {
            screen.handle_input(&[press(Button::Y)], ctx);
            let scan = next(&rx);
            assert!(matches!(scan.operation, Operation::Refresh));
            scan.emit(Event::Snapshot {
                status: WifiStatus::Disconnected,
                networks: entries
                    .iter()
                    .map(|(ssid, signal)| WifiNetwork {
                        ssid: (*ssid).into(),
                        signal: *signal,
                        ..network(false)
                    })
                    .collect(),
                scan_error: None,
            });
            assert!(screen.update(ctx));
            assert!(!screen.is_loading());
        };

        screen.selected_row = 1; // Home moves from the first to the last row.
        refresh(
            &mut screen,
            &mut ctx.0,
            &[("Cafe", 95), ("Other", 80), ("Home", 30)],
        );
        assert_eq!(screen.selected_row, 3);
        assert_eq!(screen.view.networks[screen.selected_row - 1].ssid, "Home");
        refresh(
            &mut screen,
            &mut ctx.0,
            &[("Home", 95), ("Other", 80), ("Cafe", 30)],
        );
        assert_eq!(screen.selected_row, 1);

        screen.selected_row = 0;
        refresh(
            &mut screen,
            &mut ctx.0,
            &[("Other", 95), ("Cafe", 80), ("Home", 30)],
        );
        assert_eq!(screen.selected_row, 0);

        screen.selected_row = 3;
        refresh(&mut screen, &mut ctx.0, &[("Other", 95), ("Cafe", 80)]);
        assert_eq!(screen.selected_row, 2);
        assert_eq!(screen.view.networks[screen.selected_row - 1].ssid, "Cafe");
        refresh(&mut screen, &mut ctx.0, &[]);
        assert_eq!(screen.selected_row, 0);
        assert_eq!(screen.scroll_offset, 0);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn contended_coordinator_never_blocks_update_or_navigation() {
        let (jobs, rx) = controlled_jobs();
        let mut ctx = Context::new();
        let mut screen = ready(jobs.clone(), &rx, &mut ctx.0);
        let _guard = jobs.lock().unwrap();
        assert!(!screen.update(&mut ctx.0));
        screen.handle_input(&[press(Button::Y)], &mut ctx.0);
        assert!(matches!(
            screen.handle_input(&[press(Button::B)], &mut ctx.0),
            ScreenAction::Pop
        ));
        assert!(rx.try_recv().is_err());
    }
}
