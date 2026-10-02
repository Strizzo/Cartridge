//! System updates are controlled here, but their lifetime belongs to ScreenContext.
use std::sync::Arc;

use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use cartridge_net::system_update::Release;
use sdl2::rect::Rect;

use super::{LauncherScreen, ScreenAction, ScreenContext};
use crate::neo::{self, Hint};

#[derive(Default)]
pub struct SystemUpdateScreen {
    confirmation: Option<Arc<Release>>,
    scroll: usize,
    max_scroll: usize,
    revision: u64,
}

impl SystemUpdateScreen {
    pub fn new() -> Self {
        Self::default()
    }

    fn primary_action(&self, ctx: &ScreenContext) -> Option<&'static str> {
        let jobs = &ctx.system_update_jobs;
        if jobs.is_busy() {
            None
        } else if self.confirmation.is_some() {
            Some("Confirm")
        } else if jobs.can_restart() {
            Some("Restart")
        } else if jobs.release.is_some() {
            Some("Download")
        } else if jobs.is_error {
            Some("Retry")
        } else {
            Some("Check")
        }
    }

    fn paragraphs(&self, ctx: &ScreenContext) -> Vec<(String, bool)> {
        let jobs = &ctx.system_update_jobs;
        let mut text = Vec::new();
        if let Some(release) = &self.confirmation {
            text.push((
                format!(
                    "Download and stage version {} ({})?",
                    release.version,
                    release_size(release.archive.size)
                ),
                false,
            ));
            text.push(("A confirms the download. The update is applied on the next CartridgeOS restart. B cancels.".into(), false));
            text.push(("Keep the device charged. You can browse while it downloads; app/game launches and Store changes wait until staging finishes.".into(), false));
        } else {
            text.push((jobs.message.clone(), jobs.is_error));
            if jobs.is_staging() {
                text.push(("B returns to browsing. The download continues in the background; launches and Store changes are paused.".into(), false));
            }
        }
        if jobs.can_restart() && !ctx.system_update_power_ready() {
            text.push(("Connect the charger or charge to at least 30% before restarting. An unknown battery level requires charging.".into(), true));
        }
        if ctx.store_jobs.is_busy() {
            text.push((
                "A Store task is running. Wait for it to finish before downloading or restarting."
                    .into(),
                false,
            ));
        }
        if let Some(release) = self.confirmation.as_ref().or(jobs.release.as_ref()) {
            text.push((
                format!(
                    "Available: {}  /  Download: {}",
                    release.version,
                    release_size(release.archive.size)
                ),
                false,
            ));
            text.push((format!("Revision: {}", release.revision), false));
            text.push(("RELEASE NOTES".into(), false));
            text.push((
                if release.notes.trim().is_empty() {
                    "No release notes provided.".into()
                } else {
                    release.notes.clone()
                },
                false,
            ));
        }
        if let Some(status) = &jobs.status {
            if let Some(pending) = &status.pending {
                text.push((format!("Staged for next restart: {pending}"), false));
            }
            if let Some(trial) = &status.trial {
                text.push((format!("Trial release: {trial}"), false));
            }
            if !status.last_result.is_empty() {
                text.push((format!("Last result: {}", status.last_result), false));
            }
            text.push((
                format!(
                    "Active: {}",
                    status.active.as_deref().unwrap_or("Original installation")
                ),
                false,
            ));
            if let Some(previous) = &status.previous {
                text.push((format!("Previous: {previous}"), false));
            }
        }
        text
    }
}

impl LauncherScreen for SystemUpdateScreen {
    fn update(&mut self, ctx: &mut ScreenContext) -> bool {
        ctx.system_update_jobs.ensure_status();
        if self.revision != ctx.system_update_jobs.revision {
            self.revision = ctx.system_update_jobs.revision;
            self.scroll = 0;
            self.confirmation = None;
            return true;
        }
        false
    }

    fn handle_input(&mut self, events: &[InputEvent], ctx: &mut ScreenContext) -> ScreenAction {
        for event in events {
            if matches!(event.action, InputAction::Press | InputAction::Repeat) {
                match event.button {
                    Button::DpadDown => self.scroll = (self.scroll + 1).min(self.max_scroll),
                    Button::DpadUp => self.scroll = self.scroll.saturating_sub(1),
                    _ => {}
                }
            }
            // Holding A, or multiple A events in one input batch, cannot confirm a download.
            if event.action != InputAction::Press {
                continue;
            }
            match event.button {
                Button::B => {
                    if self.confirmation.take().is_some() {
                        self.scroll = 0;
                        return ScreenAction::None;
                    }
                    return ScreenAction::Pop;
                }
                Button::Select => return ScreenAction::ShowOverlay,
                Button::A if !ctx.system_update_jobs.is_busy() => {
                    if let Some(release) = self.confirmation.take() {
                        ctx.stage_system_update(release);
                    } else if ctx.system_update_jobs.can_restart() {
                        return ScreenAction::RestartForUpdate;
                    } else if let Some(release) = &ctx.system_update_jobs.release {
                        self.confirmation = Some(Arc::clone(release));
                    } else {
                        ctx.system_update_jobs.check();
                    }
                    self.scroll = 0;
                    return ScreenAction::None;
                }
                Button::Y
                    if self.confirmation.is_none()
                        && !ctx.system_update_jobs.is_busy()
                        && !ctx.system_update_jobs.can_restart() =>
                {
                    ctx.system_update_jobs.check();
                    self.scroll = 0;
                    return ScreenAction::None;
                }
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext) {
        let theme = screen.theme;
        let is_neo = neo::is_neo(theme);
        if is_neo {
            neo::draw_header(screen, "SYSTEM UPDATE", "", Some(&ctx.sysinfo));
            neo::draw_bar(screen);
        } else {
            screen.fill(Rect::new(0, 0, 720, 64), theme.card_bg);
            screen.draw_text(
                "System Update",
                24,
                18,
                Some(theme.accent),
                22,
                true,
                Some(672),
            );
        }
        screen.draw_text(
            &format!("Current version: {}", env!("CARGO_PKG_VERSION")),
            28,
            91,
            Some(theme.text),
            18,
            true,
            Some(664),
        );
        screen.draw_text(
            "Stable channel",
            28,
            120,
            Some(theme.text_dim),
            12,
            false,
            Some(664),
        );

        // Reserve the global banner, including existing Store results, on both themes.
        let bottom = if ctx.has_background_notice() {
            if is_neo { neo::FOOTER_Y - 112 } else { 572 }
        } else {
            neo::FOOTER_Y - 12
        };
        let top = 155;
        let line_height = (screen.get_line_height(14, false) as i32 + 5).max(23);
        let capacity = ((bottom - top - 40) / line_height).max(1) as usize;
        let mut lines = Vec::new();
        for (paragraph, error) in self.paragraphs(ctx) {
            lines.extend(
                wrap_text(&paragraph, 640, |s| screen.get_text_width(s, 14, false))
                    .into_iter()
                    .map(|line| (line, error)),
            );
            lines.push((String::new(), false));
        }
        self.max_scroll = lines.len().saturating_sub(capacity);
        self.scroll = self.scroll.min(self.max_scroll);
        screen.fill(
            Rect::new(18, top, 684, (bottom - top) as u32),
            theme.card_bg,
        );
        screen.draw_outline(
            Rect::new(18, top, 684, (bottom - top) as u32),
            if self.confirmation.is_some() {
                theme.accent
            } else {
                theme.border
            },
            1,
        );
        for (row, (line, error)) in lines.iter().skip(self.scroll).take(capacity).enumerate() {
            screen.draw_text(
                line,
                32,
                top + 12 + row as i32 * line_height,
                Some(if *error { theme.text_error } else { theme.text }),
                14,
                false,
                Some(640),
            );
        }
        if self.max_scroll > 0 {
            screen.draw_text(
                &format!(
                    "D-Pad: scroll  /  {}-{} of {}",
                    self.scroll + 1,
                    (self.scroll + capacity).min(lines.len()),
                    lines.len()
                ),
                32,
                bottom - 24,
                Some(theme.text_dim),
                11,
                false,
                Some(640),
            );
        }

        let mut hints = Vec::new();
        if let Some(action) = self.primary_action(ctx) {
            hints.push(Hint::a(action));
        }
        hints.push(Hint::b(if self.confirmation.is_some() {
            "Cancel"
        } else {
            "Back"
        }));
        if self.confirmation.is_none()
            && !ctx.system_update_jobs.is_busy()
            && !ctx.system_update_jobs.can_restart()
        {
            hints.push(Hint::y("Check"));
        }
        hints.push(Hint::wide("SELECT", "Power"));
        if is_neo {
            neo::draw_footer(screen, &hints);
        } else {
            // Card themes reserve their 36px footer below the shared notice banner.
            let footer_y = crate::ui_constants::CONTENT_BOTTOM;
            screen.fill(Rect::new(0, footer_y, 720, 36), theme.card_bg);
            let mut x = 18;
            for hint in hints {
                let color = match hint.label {
                    "A" => theme.btn_a,
                    "B" => theme.btn_b,
                    _ => theme.text_dim,
                };
                x += screen.draw_button_hint(
                    hint.label,
                    &hint.action,
                    x,
                    footer_y + 8,
                    Some(color),
                    12,
                ) as i32
                    + 16;
            }
        }
    }
}

fn release_size(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / 1_048_576.0)
}

/// Wrap long paths/URLs as well as prose, preserving all Unicode characters and paragraphs.
/// Also used by the global notice banner so error details can always be paged through.
pub(crate) fn wrap_text(
    text: &str,
    width: u32,
    mut measure: impl FnMut(&str) -> u32,
) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if measure(&candidate) <= width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            for ch in word.chars() {
                let mut candidate = line.clone();
                candidate.push(ch);
                if !line.is_empty() && measure(&candidate) > width {
                    lines.push(std::mem::take(&mut line));
                }
                line.push(ch);
            }
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_errors_and_unicode_notes_are_wrapped_without_losing_details() {
        let text = format!(
            "Error\nhttps://example.test/{}\nUnicode: café日本語",
            "x".repeat(400)
        );
        let lines = wrap_text(&text, 25, |s| s.chars().count() as u32);
        assert!(lines.iter().all(|line| line.chars().count() <= 25));
        assert_eq!(
            lines.concat().split_whitespace().collect::<String>(),
            text.split_whitespace().collect::<String>()
        );
        assert_eq!(
            wrap_text("one\n\ntwo", 10, |s| s.len() as u32),
            ["one", "", "two"]
        );
    }
}

#[cfg(test)]
mod controller_tests {
    use super::*;
    use crate::system_update_jobs::SystemUpdateJobs;

    #[test]
    fn system_update_download_needs_a_separate_press_and_b_cancels_confirmation() {
        let mut ctx = super::super::test_context();
        ctx.system_update_jobs = SystemUpdateJobs::ready_for_test();
        let mut screen = SystemUpdateScreen::new();
        let press = |button| InputEvent {
            button,
            action: InputAction::Press,
        };
        let repeat = InputEvent {
            button: Button::A,
            action: InputAction::Repeat,
        };
        screen.handle_input(&[repeat], &mut ctx);
        assert!(screen.confirmation.is_none());
        screen.handle_input(&[press(Button::A), press(Button::A)], &mut ctx);
        assert!(screen.confirmation.is_some());
        assert!(!ctx.system_update_jobs.is_busy());
        screen.handle_input(&[repeat], &mut ctx);
        assert!(!ctx.system_update_jobs.is_busy());
        assert!(matches!(
            screen.handle_input(&[press(Button::B)], &mut ctx),
            ScreenAction::None
        ));
        assert!(screen.confirmation.is_none());
        assert!(!ctx.system_update_jobs.is_busy());
        screen.handle_input(&[press(Button::A)], &mut ctx);
        screen.handle_input(&[press(Button::A)], &mut ctx);
        assert!(ctx.system_update_jobs.is_staging());
        assert!(matches!(
            screen.handle_input(&[press(Button::B)], &mut ctx),
            ScreenAction::Pop
        ));
        std::fs::remove_dir_all(ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }
}
