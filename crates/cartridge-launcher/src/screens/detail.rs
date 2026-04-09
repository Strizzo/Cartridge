use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::ui_constants::*;
use super::{LauncherScreen, ScreenAction, ScreenContext};

/// Focus zone on detail screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailFocus {
    Info,
    Action,
}

pub struct DetailScreen {
    app_index: usize,
    focus: DetailFocus,
    scroll_y: i32,
    status_msg: Option<(String, std::time::Instant, bool)>, // (message, when, is_error)
}

impl DetailScreen {
    pub fn new(app_index: usize) -> Self {
        Self {
            app_index,
            focus: DetailFocus::Info,
            scroll_y: 0,
            status_msg: None,
        }
    }

    fn set_status(&mut self, msg: &str, is_error: bool) {
        self.status_msg = Some((msg.to_string(), std::time::Instant::now(), is_error));
    }
}

impl LauncherScreen for DetailScreen {
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
                    if self.focus == DetailFocus::Info {
                        self.focus = DetailFocus::Action;
                    } else {
                        self.scroll_y = (self.scroll_y + 20).min(100);
                    }
                }
                Button::DpadUp => {
                    if self.focus == DetailFocus::Action {
                        self.focus = DetailFocus::Info;
                    } else {
                        self.scroll_y = (self.scroll_y - 20).max(0);
                    }
                }
                Button::A => {
                    if self.focus == DetailFocus::Action
                        && let Some(app) = ctx.registry.apps.get(self.app_index) {
                            let app_id = app.id.clone();
                            if ctx.installed.is_installed(&app_id) {
                                // Record a recent entry, then launch
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0);
                                let entry = crate::data::RecentEntry {
                                    app_id: app_id.clone(),
                                    name: app.name.clone(),
                                    timestamp_secs: now,
                                };
                                // Remove old entry for same app
                                ctx.recents.retain(|r| r.app_id != app_id);
                                ctx.recents.insert(0, entry);
                                if ctx.recents.len() > 10 {
                                    ctx.recents.truncate(10);
                                }
                                ctx.save_recents();
                                return ScreenAction::LaunchApp(app_id);
                            } else {
                                // Install via network
                                if app.repo_url.is_empty() {
                                    self.set_status("This app is bundled and cannot be installed separately", true);
                                } else {
                                    self.set_status("Installing...", false);
                                    let net_app = to_net_app(app);
                                    if let Some(installer) = &ctx.installer {
                                        log::info!("Attempting network install of {}...", app_id);
                                        match installer.install(&net_app) {
                                            Ok(()) => {
                                                log::info!("Successfully installed {} via network", app_id);
                                                ctx.installed.install(&app_id);
                                                ctx.save_installed();
                                                self.set_status("Installed successfully", false);
                                            }
                                            Err(e) => {
                                                log::warn!("Network install failed for {}: {e}", app_id);
                                                self.set_status(&format!("Install failed: {e}"), true);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                }
                Button::X => {
                    // Remove if installed
                    if let Some(app) = ctx.registry.apps.get(self.app_index) {
                        let app_id = app.id.clone();
                        if ctx.installed.is_installed(&app_id) {
                            if let Some(installer) = &ctx.installer {
                                log::info!("Removing {} from disk...", app_id);
                                match installer.remove(&app_id) {
                                    Ok(()) => {
                                        log::info!("Successfully removed {} from disk", app_id);
                                        self.set_status("Removed", false);
                                    }
                                    Err(e) => {
                                        log::warn!("Disk removal failed for {}: {e}", app_id);
                                        self.set_status(&format!("Remove failed: {e}"), true);
                                    }
                                }
                            }
                            ctx.installed.remove(&app_id);
                            ctx.save_installed();
                        }
                    }
                }
                Button::Y => {
                    // Update: reinstall if newer version available
                    if let Some(app) = ctx.registry.apps.get(self.app_index) {
                        let app_id = app.id.clone();
                        if ctx.installed.is_installed(&app_id) {
                            if let Some(installer) = &ctx.installer {
                                let installed_ver = installer.installed_version(&app_id);
                                let registry_ver = &app.version;
                                if installed_ver.as_deref() != Some(registry_ver) {
                                    self.set_status(&format!("Updating to v{}...", registry_ver), false);
                                    log::info!("Updating {} to v{}...", app_id, registry_ver);
                                    let net_app = to_net_app(app);
                                    match installer.install(&net_app) {
                                        Ok(()) => {
                                            log::info!("Updated {} to v{}", app_id, registry_ver);
                                            self.set_status(&format!("Updated to v{}", registry_ver), false);
                                        }
                                        Err(e) => {
                                            log::warn!("Update failed for {}: {e}", app_id);
                                            self.set_status(&format!("Update failed: {e}"), true);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Button::Start => {
                    return ScreenAction::Push(super::ScreenId::Settings);
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
        let app = match ctx.registry.apps.get(self.app_index) {
            Some(a) => a,
            None => return,
        };

        let is_installed = ctx.installed.is_installed(&app.id);
        let has_update = is_installed && ctx.installer.as_ref().map_or(false, |inst| {
            inst.installed_version(&app.id).as_deref() != Some(&app.version)
        });
        let cat_color = category_color(&app.category);

        // -- Header (semi-transparent, atmosphere bleeds through) --
        screen.draw_rect(
            Rect::new(0, 0, SCREEN_WIDTH, HEADER_HEIGHT as u32),
            Some(Color::RGBA(14, 14, 20, 220)),
            true,
            0,
            None,
        );
        screen.draw_glow_line(0, 0, SCREEN_WIDTH as i32 - 1, Color::RGBA(100, 180, 255, 80), 3, 1);
        screen.draw_text_glow("< Back", 12, 8, theme.text_accent, theme.glow_primary, 16, false, None);

        // -- Header card: app name, version, author --
        let header_card_y = CONTENT_TOP + 8;
        let header_card_h = 70;
        screen.draw_card(
            Rect::new(12, header_card_y, SCREEN_WIDTH - 24, header_card_h as u32),
            None,
            None,
            CARD_RADIUS,
            false,
        );

        // Category color strip
        screen.draw_rect(
            Rect::new(12, header_card_y + 4, 3, header_card_h as u32 - 8),
            Some(cat_color),
            true,
            0,
            None,
        );

        // Icon (if available)
        let text_x = if let Some(icon_path) = crate::ui_constants::resolve_icon_path(&app.id) {
            let icon_sz = (header_card_h - 14) as u32;
            screen.draw_image(
                &icon_path,
                20,
                header_card_y + 7,
                Some((icon_sz, icon_sz)),
                None,
            );
            20 + icon_sz as i32 + 10
        } else {
            28
        };

        // App name
        screen.draw_text(
            &app.name,
            text_x,
            header_card_y + 10,
            Some(theme.text),
            20,
            true,
            Some(400),
        );

        // Version
        let ver = format!("v{}", app.version);
        let vw = screen.get_text_width(&ver, 13, false);
        screen.draw_text(
            &ver,
            SCREEN_WIDTH as i32 - 28 - vw as i32,
            header_card_y + 14,
            Some(theme.text_dim),
            13,
            false,
            None,
        );

        // Author
        let author_str = format!("by {}", app.author);
        screen.draw_text(
            &author_str,
            text_x,
            header_card_y + 38,
            Some(theme.text_dim),
            14,
            false,
            None,
        );

        // Status pill
        if is_installed {
            if has_update {
                screen.draw_pill(
                    "UPDATE AVAILABLE",
                    SCREEN_WIDTH as i32 - 155,
                    header_card_y + 40,
                    theme.text_warning,
                    Color::RGB(20, 20, 30),
                    11,
                );
            } else {
                screen.draw_pill(
                    "INSTALLED",
                    SCREEN_WIDTH as i32 - 120,
                    header_card_y + 40,
                    theme.positive,
                    Color::RGB(20, 20, 30),
                    11,
                );
            }
        }

        // -- Description card --
        let desc_card_y = header_card_y + header_card_h + MARGIN;
        let desc_card_h = 80;
        screen.draw_card(
            Rect::new(12, desc_card_y, SCREEN_WIDTH - 24, desc_card_h as u32),
            None,
            None,
            CARD_RADIUS,
            false,
        );

        screen.draw_text(
            "Description",
            24,
            desc_card_y + 8,
            Some(theme.text),
            14,
            true,
            None,
        );

        // Wrap description text manually across lines
        let desc = &app.description;
        let max_w = SCREEN_WIDTH - 52;
        let line_h = 18;
        let mut desc_y = desc_card_y + 28;
        let words: Vec<&str> = desc.split_whitespace().collect();
        let mut line = String::new();
        for word in &words {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            let w = screen.get_text_width(&candidate, 13, false);
            if w > max_w && !line.is_empty() {
                screen.draw_text(&line, 24, desc_y, Some(theme.text_dim), 13, false, None);
                desc_y += line_h;
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        if !line.is_empty() {
            screen.draw_text(&line, 24, desc_y, Some(theme.text_dim), 13, false, None);
        }

        // -- Tags card --
        let tags_card_y = desc_card_y + desc_card_h + MARGIN;
        let tags_card_h = 50;
        screen.draw_card(
            Rect::new(12, tags_card_y, SCREEN_WIDTH - 24, tags_card_h as u32),
            None,
            None,
            CARD_RADIUS,
            false,
        );

        screen.draw_text("Tags", 24, tags_card_y + 6, Some(theme.text), 13, true, None);

        let mut tx = 24;
        let tag_pill_y = tags_card_y + 24;
        for tag in &app.tags {
            if tx + 60 > SCREEN_WIDTH as i32 - 24 {
                break;
            }
            let pw = screen.draw_pill(tag, tx, tag_pill_y, theme.bg_lighter, theme.text_dim, 11);
            tx += pw as i32 + 6;
        }

        // -- Permissions card --
        let perm_card_y = tags_card_y + tags_card_h + MARGIN;
        let perm_card_h = 50;
        screen.draw_card(
            Rect::new(12, perm_card_y, SCREEN_WIDTH - 24, perm_card_h as u32),
            None,
            None,
            CARD_RADIUS,
            false,
        );

        screen.draw_text("Permissions", 24, perm_card_y + 6, Some(theme.text), 13, true, None);

        let mut px = 24;
        let perm_pill_y = perm_card_y + 24;
        if app.permissions.is_empty() {
            screen.draw_text("None required", px, perm_pill_y + 2, Some(theme.text_dim), 11, false, None);
        } else {
            for perm in &app.permissions {
                if px + 60 > SCREEN_WIDTH as i32 - 24 {
                    break;
                }
                let perm_color = match perm.as_str() {
                    "network" => theme.text_warning,
                    "storage" => theme.text_accent,
                    _ => theme.text_dim,
                };
                let pw = screen.draw_pill(perm, px, perm_pill_y, theme.bg_lighter, perm_color, 11);
                px += pw as i32 + 6;
            }
        }

        // -- Action button area --
        let action_y = perm_card_y + perm_card_h + MARGIN + 4;
        let is_action_focused = self.focus == DetailFocus::Action;

        {
            // Action hints — show which button does what (not navigable)
            let mut ax = 12;
            if is_installed {
                let w = screen.draw_button_hint("A", "Launch", ax, action_y + 8, Some(theme.positive), 14);
                ax += w as i32 + 16;
                if has_update {
                    let w = screen.draw_button_hint("Y", "Update", ax, action_y + 8, Some(theme.text_warning), 14);
                    ax += w as i32 + 16;
                }
                let w = screen.draw_button_hint("X", "Remove", ax, action_y + 8, Some(theme.negative), 14);
                ax += w as i32 + 20;
            } else {
                let w = screen.draw_button_hint("A", "Install", ax, action_y + 8, Some(theme.accent), 14);
                ax += w as i32 + 20;
            }

            // Category pill
            let cat_upper = app.category.to_uppercase();
            screen.draw_pill(
                &cat_upper,
                ax,
                action_y + 6,
                cat_color,
                Color::RGB(20, 20, 30),
                11,
            );
        }

        // -- Status message --
        if let Some((ref msg, when, is_error)) = self.status_msg {
            let elapsed = when.elapsed().as_secs_f32();
            if elapsed < 5.0 {
                let msg_color = if is_error { theme.negative } else { theme.positive };
                let msg_y = action_y + 44;
                screen.draw_text(msg, 12, msg_y, Some(msg_color), 13, false, Some(SCREEN_WIDTH - 24));
            } else {
                // Auto-clear after 5 seconds — can't mutate self here,
                // so it will be cleared on next input.
            }
        }

        // -- Footer --
        draw_detail_footer(screen, is_installed, has_update);
    }
}

/// Convert a launcher `AppEntry` into the `cartridge_net::RegistryApp`
/// expected by `AppInstaller::install`.
fn to_net_app(app: &crate::data::AppEntry) -> cartridge_net::RegistryApp {
    cartridge_net::RegistryApp {
        id: app.id.clone(),
        name: app.name.clone(),
        description: app.description.clone(),
        version: app.version.clone(),
        author: app.author.clone(),
        category: app.category.clone(),
        tags: app.tags.clone(),
        repo_url: app.repo_url.clone(),
        permissions: app.permissions.clone(),
    }
}

fn draw_detail_footer(screen: &mut Screen, is_installed: bool, has_update: bool) {
    let theme = screen.theme;
    let footer_y = SCREEN_HEIGHT as i32 - FOOTER_HEIGHT;

    screen.draw_rect(
        Rect::new(0, footer_y, SCREEN_WIDTH, FOOTER_HEIGHT as u32),
        Some(Color::RGBA(14, 14, 20, 220)),
        true,
        0,
        None,
    );
    screen.draw_glow_line(footer_y, 0, SCREEN_WIDTH as i32 - 1, Color::RGBA(100, 180, 255, 50), 2, -1);

    let mut fx = 12;
    let action_label = if is_installed { "Launch" } else { "Install" };
    let w = screen.draw_button_hint("A", action_label, fx, footer_y + 8, Some(theme.btn_a), 12);
    fx += w as i32 + 12;
    let w = screen.draw_button_hint("B", "Back", fx, footer_y + 8, Some(theme.btn_b), 12);
    fx += w as i32 + 12;
    if is_installed {
        let w = screen.draw_button_hint("X", "Remove", fx, footer_y + 8, Some(theme.btn_x), 12);
        fx += w as i32 + 12;
        if has_update {
            screen.draw_button_hint("Y", "Update", fx, footer_y + 8, Some(theme.btn_y), 12);
        }
    }
}
