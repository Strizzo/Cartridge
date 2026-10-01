use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::neo::{self, Chip, Hint};
use crate::ui_constants::*;
use super::{LauncherScreen, ScreenAction, ScreenContext};

pub struct DetailScreen {
    app_id: String,
}

impl DetailScreen {
    pub fn new(app_id: String) -> Self { Self { app_id } }
}

impl LauncherScreen for DetailScreen {
    fn handle_input(&mut self, events: &[InputEvent], ctx: &mut ScreenContext) -> ScreenAction {
        use crate::store_jobs::StoreOperation;
        for event in events {
            if event.action != InputAction::Press { continue }
            match event.button {
                Button::B => return ScreenAction::Pop,
                Button::Start => return ScreenAction::Push(super::ScreenId::Settings),
                Button::Select => return ScreenAction::ShowOverlay,
                Button::A | Button::X | Button::Y | Button::L1 if !ctx.store_jobs.is_busy() => {
                    let Some(app) = ctx.app(&self.app_id).cloned() else { continue };
                    match event.button {
                        Button::A if ctx.installed.is_installed(&self.app_id) => {
                            let name = ctx.local_apps.apps.iter().find(|a| a.id == self.app_id).map(|a| a.name.clone()).unwrap_or(app.name);
                            ctx.recents.retain(|r| r.app_id != self.app_id);
                            ctx.recents.insert(0, crate::data::RecentEntry {
                                app_id: self.app_id.clone(), name,
                                timestamp_secs: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
                            });
                            ctx.recents.truncate(10);
                            ctx.save_recents();
                            return ScreenAction::LaunchApp(self.app_id.clone());
                        }
                        Button::A => ctx.start_store_job(StoreOperation::Install(app)),
                        Button::Y => ctx.start_store_job(StoreOperation::Update(app)),
                        Button::X => ctx.start_store_job(StoreOperation::Remove(self.app_id.clone())),
                        Button::L1 => ctx.start_store_job(StoreOperation::Rollback(self.app_id.clone())),
                        _ => {},
                    }
                }
                _ => {},
            }
        }
        ScreenAction::None
    }

    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext) {
        let theme = screen.theme;
        let app = match detail_app(ctx, &self.app_id) {
            Some(a) => a,
            None => return,
        };

        let is_installed = ctx.installed.is_installed(&app.id);
        let has_update = ctx.has_update(&app.id);

        if neo::is_neo(theme) {
            self.render_neo(screen, ctx, app, is_installed, has_update);
            return;
        }

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

        let availability = availability_text(ctx, &app.id);
        screen.draw_text(&availability, 24, perm_card_y + perm_card_h + 24, Some(theme.text_dim), 13, false, Some(SCREEN_WIDTH - 48));
        draw_detail_footer(screen, ctx, &app.id);

    }
}

impl DetailScreen {
    fn render_neo(
        &self,
        screen: &mut Screen,
        ctx: &ScreenContext,
        app: &crate::data::AppEntry,
        is_installed: bool,
        has_update: bool,
    ) {
        let theme = screen.theme;
        let subtitle = format!("V{} · {}", app.version, app.author);
        neo::draw_header(screen, &app.name.to_uppercase(), &subtitle, Some(&ctx.sysinfo));
        neo::draw_bar(screen);

        // Icon tile + description.
        let top = neo::CONTENT_Y + 20;
        let tile = Rect::new(neo::MARGIN_X, top, 96, 96);
        screen.fill(tile, theme.card_bg);
        screen.draw_outline(tile, theme.border, 1);
        let drew = crate::ui_constants::resolve_icon_path(&app.id)
            .map(|p| screen.draw_image(&p, tile.x() + 14, tile.y() + 14, Some((68, 68)), None))
            .unwrap_or(false);
        if !drew {
            let abbr: String = app.name.chars().take(2).collect::<String>().to_uppercase();
            let w = screen.display_text_width(&abbr, 44) as i32;
            screen.draw_display_text(&abbr, tile.x() + (96 - w) / 2, tile.y() + 24, theme.text, 44);
        }

        let text_x = neo::MARGIN_X + 96 + 20;
        let text_w = (SCREEN_WIDTH as i32 - neo::MARGIN_X - text_x) as u32;
        let lines = neo::wrap_lines(screen, &app.description, 13, false, text_w, 4);
        let mut ly = top + 2;
        for line in &lines {
            screen.draw_text(line, text_x, ly, Some(theme.text), 13, false, None);
            ly += 20;
        }

        // State, category, permissions.
        let mut chips: Vec<(String, Chip)> = Vec::new();
        if has_update {
            chips.push(("Update available".to_string(), Chip::FilledRed));
        } else if is_installed {
            chips.push(("Installed".to_string(), Chip::OutlineWhite));
        } else {
            chips.push(("Not installed".to_string(), Chip::OutlineDim));
        }
        if !app.category.is_empty() {
            chips.push((app.category.clone(), Chip::OutlineWhite));
        }
        for perm in &app.permissions {
            chips.push((perm.clone(), Chip::OutlineDim));
        }
        let chip_y = (ly + 8).max(top + 104);
        neo::draw_chip_row(screen, &chips, neo::MARGIN_X, chip_y, 8, SCREEN_WIDTH as i32 - neo::MARGIN_X);

        // Tags.
        let mut y = chip_y + 44;
        neo::rule(screen, y);
        y += 14;
        screen.draw_text("TAGS", neo::MARGIN_X, y, Some(theme.text_dim), neo::LABEL_SIZE, false, None);
        y += 20;
        if app.tags.is_empty() {
            screen.draw_text("NONE", neo::MARGIN_X, y + 3, Some(theme.text_muted), neo::LABEL_SIZE, false, None);
        } else {
            let tags: Vec<(String, Chip)> = app.tags.iter().map(|t| (t.clone(), Chip::OutlineDim)).collect();
            neo::draw_chip_row(screen, &tags, neo::MARGIN_X, y, 8, SCREEN_WIDTH as i32 - neo::MARGIN_X);
        }

        // Source.
        y += 44;
        neo::rule(screen, y);
        y += 14;
        screen.draw_text("SOURCE", neo::MARGIN_X, y, Some(theme.text_dim), neo::LABEL_SIZE, false, None);
        y += 18;
        let source = if app.repo_url.is_empty() { "Bundled with CartridgeOS".to_string() } else { app.repo_url.clone() };
        screen.draw_text(&source, neo::MARGIN_X, y, Some(theme.text), 12, false, Some(SCREEN_WIDTH - neo::MARGIN_X as u32 * 2));

        let availability = availability_text(ctx, &app.id);
        screen.draw_text(&availability, neo::MARGIN_X, y + 38, Some(theme.text_dim), 12, false, Some(SCREEN_WIDTH - 48));
        let hints = detail_hints(ctx, &app.id);
        neo::draw_footer(screen, &hints);
    }
}

// Show the available release's description and permissions before Update is
// accepted. Installed version remains explicit in the availability line.
fn detail_app<'a>(ctx: &'a ScreenContext, id: &str) -> Option<&'a crate::data::AppEntry> {
    ctx.app(id).or_else(|| ctx.local_apps.apps.iter().find(|app| app.id == id))
}

fn availability_text(ctx: &ScreenContext, id: &str) -> String {
    let installed = ctx.installed_version(id);
    let remote = ctx.app(id);
    match (installed, remote) {
        (Some(current), Some(app)) if app.package.is_some() => format!("Installed v{current} · Store v{}", app.version),
        (Some(current), _) => format!("Installed v{current} · No signed download available"),
        (_, Some(app)) if app.package.is_some() => format!("Signed package · v{}", app.version),
        _ => "No signed download available".into(),
    }
}

fn detail_hints(ctx: &ScreenContext, id: &str) -> Vec<Hint> {
    let mut hints = vec![];
    if !ctx.store_jobs.is_busy() {
        if ctx.installed.is_installed(id) { hints.push(Hint::a("Launch")); }
        else if ctx.app(id).is_some_and(|a| a.package.is_some()) { hints.push(Hint::a("Install")); }
        if ctx.has_update(id) { hints.push(Hint::y("Update")); }
        if ctx.has_override(id) { hints.push(Hint::x("Remove")); }
        if ctx.can_rollback(id) { hints.push(Hint::wide("L1", "Rollback")); }
    }
    hints.push(Hint::b("Back"));
    hints
}

fn draw_detail_footer(screen: &mut Screen, ctx: &ScreenContext, id: &str) {
    let theme = screen.theme;
    let footer_y = SCREEN_HEIGHT as i32 - FOOTER_HEIGHT;
    screen.fill(Rect::new(0, footer_y, SCREEN_WIDTH, FOOTER_HEIGHT as u32), theme.bg);
    let mut x = 12;
    for hint in detail_hints(ctx, id) {
        let width = screen.draw_button_hint(hint.label, &hint.action, x, footer_y + 8, Some(theme.accent), 12);
        x += width as i32 + 12;
    }
}

#[cfg(test)]
mod release_details_tests {
    use super::*;
    #[test]
    fn update_shows_new_release_permissions_not_old_installed_permissions() {
        let mut ctx = super::super::test_context();
        let old: crate::data::AppEntry = serde_json::from_value(serde_json::json!({"id":"dev.cartridge.test","name":"Test","version":"1.0.0","permissions":["storage"]})).unwrap();
        let mut new = old.clone();
        new.version = "1.1.0".into();
        new.permissions.push("network".into());
        ctx.local_apps.apps.push(old);
        ctx.registry.apps.push(new);
        let shown = detail_app(&ctx, "dev.cartridge.test").unwrap();
        assert_eq!(shown.version, "1.1.0");
        assert!(shown.permissions.iter().any(|p| p == "network"));
    }
}
