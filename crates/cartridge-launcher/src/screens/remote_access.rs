//! SSH controls belong to Cartridge Settings; no EmulationStation dependency.
use super::{LauncherScreen, ScreenAction, ScreenContext};
use crate::neo::{self, Hint};
use cartridge_core::{
    input::{Button, InputAction, InputEvent},
    remote_access::{self, Status},
    screen::Screen,
};
use sdl2::rect::Rect;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
};

static WORKING: AtomicBool = AtomicBool::new(false);

pub struct RemoteAccessScreen {
    status: Option<Status>,
    job: Option<mpsc::Receiver<Result<Status, String>>>,
    waiting: bool,
    error: Option<String>,
    confirmation: Option<bool>,
    scroll: usize,
    max_scroll: usize,
}

impl RemoteAccessScreen {
    pub fn new() -> Self {
        let mut screen = Self {
            status: None,
            job: None,
            waiting: false,
            error: None,
            confirmation: None,
            scroll: 0,
            max_scroll: 0,
        };
        screen.request(None);
        screen
    }

    fn request(&mut self, change: Option<bool>) {
        if self.job.is_some() {
            return;
        }
        if WORKING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            // A change accepted by a screen that has since closed finishes first.
            self.waiting = true;
            return;
        }
        self.waiting = false;
        self.error = None;
        self.scroll = 0;
        let (reply, result) = mpsc::sync_channel(1);
        match std::thread::Builder::new()
            .name("remote-access".into())
            .spawn(move || {
                let answer = std::panic::catch_unwind(|| match change {
                    Some(enabled) => remote_access::configure(enabled),
                    None => remote_access::status(),
                })
                .unwrap_or_else(|_| Err("Remote access worker failed. Press Y to retry.".into()));
                WORKING.store(false, Ordering::Release);
                let _ = reply.send(answer);
            }) {
            Ok(_) => self.job = Some(result),
            Err(error) => {
                WORKING.store(false, Ordering::Release);
                self.error = Some(format!("Cannot start remote access check: {error}"));
            }
        }
    }

    fn busy(&self) -> bool {
        self.job.is_some() || self.waiting
    }

    fn paragraphs(&self) -> Vec<(String, bool)> {
        let mut rows = Vec::new();
        if let Some(enabled) = self.confirmation {
            rows.push((
                if enabled {
                    "Enable SSH now and at startup?"
                } else {
                    "Stop SSH and disable it at startup?"
                }
                .into(),
                false,
            ));
            rows.push((if enabled { "People on your network with a valid device login or SSH key will be able to access this handheld." } else { "Remote connections may be interrupted. You can enable access again here using the controller." }.into(), false));
            rows.push(("A confirms. B cancels.".into(), false));
        } else {
            rows.push(("Use SSH, SFTP or the wireless development tools from another computer on your network.".into(), false));
        }
        if let Some(status) = &self.status {
            rows.push((
                format!(
                    "SSH: {}   /   At startup: {}",
                    if status.running { "RUNNING" } else { "STOPPED" },
                    if status.on_boot {
                        "ENABLED"
                    } else {
                        "DISABLED"
                    }
                ),
                false,
            ));
            rows.push((
                match status.address {
                    Some(ip) => format!("Connect: ssh {}@{ip}", status.login),
                    None => "No network address. Connect to WiFi first, then refresh.".into(),
                },
                false,
            ));
            if status.simulated {
                rows.push(("SIMULATOR: these controls and the example address do not change your Mac's SSH service.".into(), false));
            }
        }
        if self.busy() {
            rows.push(("Checking or applying remote access… You can return to Settings while this finishes.".into(), false));
        }
        if let Some(error) = &self.error {
            rows.push((error.clone(), true));
        }
        rows.push(("Use your device password or an SSH key. This setting does not change credentials or enable other network services.".into(), false));
        rows
    }
}

impl LauncherScreen for RemoteAccessScreen {
    fn update(&mut self, _ctx: &mut ScreenContext) -> bool {
        if self.waiting && !WORKING.load(Ordering::Acquire) {
            self.request(None);
            return true;
        }
        let result = self.job.as_ref().map(|job| job.try_recv());
        match result {
            Some(Ok(result)) => {
                self.job = None;
                match result {
                    Ok(status) => {
                        self.status = Some(status);
                        self.error = None;
                    }
                    Err(error) => {
                        self.status = None;
                        self.error = Some(error);
                    }
                }
                true
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.job = None;
                self.error = Some("Remote access check stopped. Press Y to retry.".into());
                true
            }
            _ => false,
        }
    }
    fn is_loading(&self) -> bool {
        self.busy()
    }

    fn handle_input(&mut self, events: &[InputEvent], _ctx: &mut ScreenContext) -> ScreenAction {
        for event in events {
            if matches!(event.action, InputAction::Press | InputAction::Repeat) {
                match event.button {
                    Button::DpadDown => self.scroll = (self.scroll + 1).min(self.max_scroll),
                    Button::DpadUp => self.scroll = self.scroll.saturating_sub(1),
                    _ => {}
                }
            }
            if event.action != InputAction::Press {
                continue;
            }
            match event.button {
                Button::B => {
                    if self.confirmation.take().is_some() {
                        return ScreenAction::None;
                    }
                    return ScreenAction::Pop;
                }
                Button::Select => return ScreenAction::ShowOverlay,
                Button::A if !self.busy() => {
                    if let Some(enabled) = self.confirmation.take() {
                        self.request(Some(enabled));
                    } else if self.status.is_some() {
                        self.confirmation = Some(true);
                    }
                    return ScreenAction::None;
                }
                Button::X
                    if !self.busy() && self.confirmation.is_none() && self.status.is_some() =>
                {
                    self.confirmation = Some(false);
                    return ScreenAction::None;
                }
                Button::Y if !self.busy() && self.confirmation.is_none() => {
                    self.request(None);
                    return ScreenAction::None;
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext) {
        let theme = screen.theme;
        let neo_style = neo::is_neo(theme);
        if neo_style {
            neo::draw_header(screen, "REMOTE ACCESS", "", Some(&ctx.sysinfo));
            neo::draw_bar(screen);
        } else {
            screen.fill(Rect::new(0, 0, 720, 70), theme.card_bg);
            screen.draw_text("Remote Access", 24, 20, Some(theme.accent), 22, true, None);
        }
        let bottom = if ctx.has_background_notice() {
            neo::FOOTER_Y - 112
        } else {
            neo::FOOTER_Y - 12
        };
        screen.fill(Rect::new(18, 88, 684, (bottom - 88) as u32), theme.card_bg);
        screen.draw_outline(
            Rect::new(18, 88, 684, (bottom - 88) as u32),
            if self.confirmation.is_some() {
                theme.accent
            } else {
                theme.border
            },
            1,
        );
        let mut lines = Vec::new();
        for (text, error) in self.paragraphs() {
            lines.extend(
                super::system_update::wrap_text(&text, 640, |s| {
                    screen.get_text_width(s, 14, false)
                })
                .into_iter()
                .map(|line| (line, error)),
            );
            lines.push((String::new(), false));
        }
        let capacity = ((bottom - 105 - 25) / 23).max(1) as usize;
        self.max_scroll = lines.len().saturating_sub(capacity);
        self.scroll = self.scroll.min(self.max_scroll);
        for (row, (line, error)) in lines.iter().skip(self.scroll).take(capacity).enumerate() {
            screen.draw_text(
                line,
                32,
                105 + row as i32 * 23,
                Some(if *error { theme.text_error } else { theme.text }),
                14,
                false,
                Some(640),
            );
        }
        if self.max_scroll > 0 {
            screen.draw_text(
                "D-Pad: scroll",
                32,
                bottom - 23,
                Some(theme.text_dim),
                11,
                false,
                None,
            );
        }
        let mut hints = vec![Hint::b(if self.confirmation.is_some() {
            "Cancel"
        } else {
            "Back"
        })];
        if !self.busy() {
            if self.confirmation.is_some() {
                hints.insert(0, Hint::a("Confirm"));
            } else {
                if self.status.is_some() {
                    hints.insert(0, Hint::a("Enable"));
                    hints.push(Hint::x("Disable"));
                }
                hints.push(Hint::y("Refresh"));
            }
        }
        if neo_style {
            neo::draw_footer(screen, &hints);
        } else {
            screen.fill(Rect::new(0, neo::FOOTER_Y, 720, 48), theme.card_bg);
            let mut x = 18;
            for hint in hints {
                x += screen.draw_button_hint(
                    hint.label,
                    &hint.action,
                    x,
                    neo::FOOTER_Y + 10,
                    None,
                    12,
                ) as i32
                    + 14;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controller_requires_confirmation_and_repeat_never_applies_changes() {
        let mut ctx = super::super::test_context();
        let mut screen = RemoteAccessScreen {
            status: Some(Status {
                running: false,
                on_boot: false,
                login: "ark".into(),
                address: None,
                simulated: true,
            }),
            job: None,
            waiting: false,
            error: None,
            confirmation: None,
            scroll: 0,
            max_scroll: 0,
        };
        let event = |button, action| InputEvent { button, action };
        screen.handle_input(
            &[
                event(Button::A, InputAction::Press),
                event(Button::A, InputAction::Press),
            ],
            &mut ctx,
        );
        assert_eq!(screen.confirmation, Some(true));
        assert!(screen.job.is_none());
        screen.handle_input(&[event(Button::A, InputAction::Repeat)], &mut ctx);
        assert!(screen.job.is_none());
        screen.handle_input(&[event(Button::B, InputAction::Press)], &mut ctx);
        assert_eq!(screen.confirmation, None);
        screen.handle_input(&[event(Button::X, InputAction::Press)], &mut ctx);
        assert_eq!(screen.confirmation, Some(false));
        assert!(matches!(
            screen.handle_input(&[event(Button::B, InputAction::Press)], &mut ctx),
            ScreenAction::None
        ));
        assert!(matches!(
            screen.handle_input(&[event(Button::B, InputAction::Press)], &mut ctx),
            ScreenAction::Pop
        ));
        std::fs::remove_dir_all(ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }
}
