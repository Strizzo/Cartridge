//! Native management for the independently signed browser engine.
use super::system_update::wrap_text;
use super::{LauncherScreen, ScreenAction, ScreenContext};
use crate::{
    browser_component_jobs::{EngineRelease, MANIFEST_URL},
    neo::{self, Hint},
};
use cartridge_core::{
    input::{Button, InputAction, InputEvent},
    screen::Screen,
};
use sdl2::rect::Rect;
use std::sync::Arc;

enum Confirmation {
    Install(Arc<EngineRelease>),
    Rollback(String),
}
#[derive(Default)]
pub struct BrowserToolsScreen {
    confirmation: Option<Confirmation>,
    scroll: usize,
    max_scroll: usize,
    revision: u64,
    elapsed: u64,
}
impl BrowserToolsScreen {
    pub fn new() -> Self {
        Self::default()
    }
    fn primary(&self, ctx: &ScreenContext) -> Option<&'static str> {
        if ctx.browser_component_jobs.is_busy() {
            None
        } else if self.confirmation.is_some() {
            Some("Confirm")
        } else if ctx.browser_component_jobs.can_install() {
            Some("Install")
        } else {
            Some("Check")
        }
    }
    fn paragraphs(&self, ctx: &ScreenContext) -> Vec<(String, bool)> {
        let jobs = &ctx.browser_component_jobs;
        let mut text = Vec::new();
        match &self.confirmation {
            Some(Confirmation::Install(release)) => {
                text.push((
                    "Install the latest signed browser engine from the official channel?".into(),
                    false,
                ));
                text.push((
                    format!(
                        "Last checked: engine {} / Chromium {} / download {}.",
                        release.version,
                        release.chromium,
                        size(release.download_bytes)
                    ),
                    false,
                ));
                text.push(("The latest manifest is fetched and its signature verified again during installation. The published release may have changed since this check.".into(),false));
                text.push(("A confirms. B cancels. Keep the handheld charged. You can browse the launcher while it installs; app and game launches wait until it finishes.".into(),false));
            }
            Some(Confirmation::Rollback(_)) => {
                text.push((
                    "Switch back to the previously installed browser engine?".into(),
                    false,
                ));
                text.push(("The previous engine's signature and all files are verified before switching. If the current engine also verifies, it becomes the next rollback candidate.".into(),false));
                text.push((
                    "A confirms. B cancels. Verification can take several minutes on SD storage."
                        .into(),
                    false,
                ));
            }
            None => {
                text.push((jobs.message.clone(), jobs.is_error));
                if jobs.is_busy() {
                    text.push((
                        format!(
                            "Elapsed: {}s. File verification and downloads run in the background.",
                            jobs.elapsed()
                        ),
                        false,
                    ));
                    text.push(("B returns to the launcher; this operation continues. Open Settings > Browser Tools to see its status.".into(),false));
                }
            }
        }
        if let Some(local) = &jobs.local {
            if let Some(current) = &local.current {
                text.push((
                    format!(
                        "Installed engine: {}\nChromium: {}\nInstalled file verification: passed",
                        current.version, current.chromium
                    ),
                    false,
                ));
            } else if local.state.current.is_some() {
                text.push(("An engine is recorded, but its files could not be verified. R1 retries local verification; X can recover the previous engine.".into(),true));
            } else if local.supported {
                text.push(("Installed signed engine: none".into(), false));
            }
            if let Some(problem) = &local.problem {
                if problem != &jobs.message {
                    text.push((problem.clone(), true));
                }
            }
            text.push((
                if local.state.previous.is_some() {
                    "Previous engine: available for verified rollback"
                } else {
                    "Previous engine: none"
                }
                .into(),
                false,
            ));
        }
        if let Some(release) = &jobs.release {
            text.push((
                format!(
                    "Official release: {}\nChromium: {}\nDownload: {}\nRequires CartridgeOS {}",
                    release.version,
                    release.chromium,
                    size(release.download_bytes),
                    release.min_runtime
                ),
                false,
            ));
        }
        text.push(("Official signed channel".into(), false));
        text.push((MANIFEST_URL.into(), false));
        text
    }
}
impl LauncherScreen for BrowserToolsScreen {
    fn update(&mut self, ctx: &mut ScreenContext) -> bool {
        ctx.browser_component_jobs.ensure_status();
        let jobs = &ctx.browser_component_jobs;
        if self.revision != jobs.revision {
            self.revision = jobs.revision;
            self.confirmation = None;
            self.scroll = 0;
            return true;
        }
        let elapsed = jobs.elapsed();
        if elapsed != self.elapsed {
            self.elapsed = elapsed;
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
                Button::A if !ctx.browser_component_jobs.is_busy() => {
                    if let Some(confirmation) = self.confirmation.take() {
                        match confirmation {
                            Confirmation::Install(release) => {
                                ctx.install_browser_component(release);
                            }
                            Confirmation::Rollback(previous) => {
                                ctx.rollback_browser_component(&previous);
                            }
                        }
                    } else if ctx.browser_component_jobs.can_install() {
                        self.confirmation = ctx
                            .browser_component_jobs
                            .release
                            .clone()
                            .map(Confirmation::Install);
                    } else {
                        ctx.browser_component_jobs.check();
                    }
                    self.revision = ctx.browser_component_jobs.revision;
                    self.scroll = 0;
                    return ScreenAction::None;
                }
                Button::X
                    if self.confirmation.is_none() && ctx.browser_component_jobs.can_rollback() =>
                {
                    self.confirmation = ctx
                        .browser_component_jobs
                        .local
                        .as_ref()
                        .and_then(|s| s.state.previous.clone())
                        .map(Confirmation::Rollback);
                    self.revision = ctx.browser_component_jobs.revision;
                    self.scroll = 0;
                    return ScreenAction::None;
                }
                Button::Y | Button::R1
                    if self.confirmation.is_none() && !ctx.browser_component_jobs.is_busy() =>
                {
                    if event.button == Button::Y {
                        ctx.browser_component_jobs.check();
                    } else {
                        ctx.browser_component_jobs.refresh();
                    }
                    self.revision = ctx.browser_component_jobs.revision;
                    self.scroll = 0;
                    return ScreenAction::None;
                }
                _ => {}
            }
        }
        ScreenAction::None
    }
    fn render(&mut self, s: &mut Screen, ctx: &ScreenContext) {
        let t = s.theme;
        let neo = neo::is_neo(t);
        if neo {
            neo::draw_header(s, "BROWSER TOOLS", "", Some(&ctx.sysinfo));
            neo::draw_bar(s);
        } else {
            s.fill(Rect::new(0, 0, 720, 64), t.card_bg);
            s.draw_text("Browser Tools", 24, 18, Some(t.accent), 22, true, Some(672));
        }
        s.draw_text("Browser engine", 28, 91, Some(t.text), 20, true, Some(664));
        s.draw_text(
            "Signed HTTPS installation / verified rollback",
            28,
            122,
            Some(t.text_dim),
            12,
            false,
            Some(664),
        );
        let top = 155;
        let bottom = if ctx.has_background_notice() {
            if neo { neo::FOOTER_Y - 112 } else { 572 }
        } else {
            neo::FOOTER_Y - 12
        };
        let line_height = (s.get_line_height(14, false) as i32 + 5).max(23);
        let capacity = ((bottom - top - 40) / line_height).max(1) as usize;
        let mut lines = Vec::new();
        for (paragraph, error) in self.paragraphs(ctx) {
            lines.extend(
                wrap_text(&paragraph, 640, |text| s.get_text_width(text, 14, false))
                    .into_iter()
                    .map(|text| (text, error)),
            );
            lines.push((String::new(), false));
        }
        self.max_scroll = lines.len().saturating_sub(capacity);
        self.scroll = self.scroll.min(self.max_scroll);
        s.fill(Rect::new(18, top, 684, (bottom - top) as u32), t.card_bg);
        s.draw_outline(
            Rect::new(18, top, 684, (bottom - top) as u32),
            if self.confirmation.is_some() {
                t.accent
            } else {
                t.border
            },
            1,
        );
        for (row, (line, error)) in lines.iter().skip(self.scroll).take(capacity).enumerate() {
            s.draw_text(
                line,
                32,
                top + 12 + row as i32 * line_height,
                Some(if *error { t.text_error } else { t.text }),
                14,
                false,
                Some(640),
            );
        }
        if self.max_scroll > 0 {
            s.draw_text(
                &format!(
                    "D-Pad scroll / {}-{} of {}",
                    self.scroll + 1,
                    (self.scroll + capacity).min(lines.len()),
                    lines.len()
                ),
                32,
                bottom - 24,
                Some(t.text_dim),
                11,
                false,
                Some(640),
            );
        }
        let mut hints = Vec::new();
        if let Some(primary) = self.primary(ctx) {
            hints.push(Hint::a(primary));
        }
        hints.push(Hint::b(if self.confirmation.is_some() {
            "Cancel"
        } else {
            "Back"
        }));
        if self.confirmation.is_none() && !ctx.browser_component_jobs.is_busy() {
            if ctx.browser_component_jobs.can_rollback() {
                hints.push(Hint::wide("X", "Rollback"));
            }
            hints.push(Hint::y("Check"));
            hints.push(Hint::wide("R1", "Status"));
        }
        if neo {
            neo::draw_footer(s, &hints);
        } else {
            let y = crate::ui_constants::CONTENT_BOTTOM;
            s.fill(Rect::new(0, y, 720, 36), t.card_bg);
            let mut x = 18;
            for hint in hints {
                x += s.draw_button_hint(hint.label, &hint.action, x, y + 8, Some(t.text_dim), 12)
                    as i32
                    + 16;
            }
        }
    }
}
fn size(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / 1048576.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn press(button: Button) -> InputEvent {
        InputEvent {
            button,
            action: InputAction::Press,
        }
    }
    #[test]
    fn install_and_rollback_require_separate_presses_and_cancel_is_safe() {
        let mut ctx = super::super::test_context();
        ctx.browser_component_jobs = crate::browser_component_jobs::ready_for_test();
        let mut screen = BrowserToolsScreen::new();
        screen.handle_input(&[press(Button::A), press(Button::A)], &mut ctx);
        assert!(matches!(
            screen.confirmation,
            Some(Confirmation::Install(_))
        ));
        assert!(!ctx.browser_component_jobs.is_busy());
        screen.handle_input(
            &[InputEvent {
                button: Button::A,
                action: InputAction::Repeat,
            }],
            &mut ctx,
        );
        assert!(!ctx.browser_component_jobs.is_busy());
        screen.handle_input(&[press(Button::B)], &mut ctx);
        assert!(screen.confirmation.is_none());
        assert!(!ctx.browser_component_jobs.is_busy());
        screen.handle_input(&[press(Button::X), press(Button::A)], &mut ctx);
        assert!(matches!(
            screen.confirmation,
            Some(Confirmation::Rollback(_))
        ));
        assert!(!ctx.browser_component_jobs.is_busy());
        assert!(matches!(
            screen.handle_input(&[press(Button::B)], &mut ctx),
            ScreenAction::None
        ));
        assert!(matches!(
            screen.handle_input(&[press(Button::B)], &mut ctx),
            ScreenAction::Pop
        ));
    }
    #[test]
    fn backing_out_does_not_drop_the_context_owned_verification_job() {
        let mut ctx = super::super::test_context();
        let (jobs, done) = crate::browser_component_jobs::blocked_for_test(false);
        ctx.browser_component_jobs = jobs;
        let mut screen = BrowserToolsScreen::new();
        let now = std::time::Instant::now();
        assert!(matches!(
            screen.handle_input(&[press(Button::B)], &mut ctx),
            ScreenAction::Pop
        ));
        assert!(ctx.browser_component_jobs.is_busy());
        assert!(now.elapsed() < std::time::Duration::from_millis(100));
        done.send(()).unwrap();
    }
}
