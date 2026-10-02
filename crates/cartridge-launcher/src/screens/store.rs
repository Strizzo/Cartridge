use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use cartridge_core::theme::{style_of, UiStyle};
use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::data::CATEGORIES;
use crate::neo::{self, Chip, Hint};
use crate::ui_constants::*;
use super::{LauncherScreen, ScreenAction, ScreenContext, ScreenId};

// Neo-Tokyo store: tab row under the bar, then 76px numbered rows.
const NEO_TAB_H: i32 = 40;
const FILTER_H: i32 = 32;
const NEO_LIST_Y: i32 = neo::CONTENT_Y + NEO_TAB_H + FILTER_H;
const NEO_ROW_H: i32 = 80;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StoreView {
    #[default]
    Browse,
    Installed,
    Updates,
}

const VIEWS: [StoreView; 3] = [StoreView::Browse, StoreView::Installed, StoreView::Updates];

impl StoreView {
    fn label(self) -> &'static str {
        match self { Self::Browse => "Browse", Self::Installed => "Installed", Self::Updates => "Updates" }
    }
    fn includes(self, ctx: &ScreenContext, id: &str) -> bool {
        match self {
            Self::Browse => true,
            Self::Installed => ctx.installed_version(id).is_some(),
            Self::Updates => ctx.has_update(id),
        }
    }
}

pub struct StoreScreen {
    view: StoreView,
    seen_revision: Option<u64>,
    category_index: usize,
    selected_index: i32,
    scroll_offset: i32,
    app_order: Vec<String>,
    refresh_started: bool,
}

impl Default for StoreScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl StoreScreen {
    pub fn new() -> Self {
        Self {
            view: StoreView::Browse,
            seen_revision: None,
            category_index: 0,
            selected_index: 0,
            scroll_offset: 0,
            app_order: Vec::new(),
            refresh_started: false,
        }
    }

    fn reconcile_selection(&mut self, ctx: &ScreenContext) {
        if self.seen_revision == Some(ctx.registry_revision) { return; }
        self.seen_revision = Some(ctx.registry_revision);
        let order: Vec<_> = self.filtered_indices(ctx).iter().map(|i| ctx.registry.apps[*i].id.clone()).collect();
        self.selected_index = super::preserve_selection(&self.app_order, &order, self.selected_index);
        self.app_order = order;
        if self.selected_index < self.scroll_offset { self.scroll_offset = self.selected_index; }
        let visible = visible_rows(ctx);
        if self.selected_index >= self.scroll_offset + visible { self.scroll_offset = self.selected_index - visible + 1; }
    }

    fn keep_visible(&mut self, ctx: &ScreenContext) {
        self.scroll_offset = self.scroll_offset.min(self.selected_index);
        self.scroll_offset = self.scroll_offset.max(self.selected_index - visible_rows(ctx) + 1);
    }

    fn change_view(&mut self, forward: bool) {
        let index = VIEWS.iter().position(|v| *v == self.view).unwrap_or(0);
        self.view = VIEWS[(index + if forward { 1 } else { VIEWS.len() - 1 }) % VIEWS.len()];
        // A category chosen in Browse should not silently hide installed apps or updates.
        self.category_index = 0;
        self.reset_selection();
    }

    fn reset_selection(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.app_order.clear();
        self.seen_revision = None;
    }

    fn view_label(&self, view: StoreView, ctx: &ScreenContext) -> String {
        if view == StoreView::Updates && !ctx.registry.apps.iter().any(|a| a.package.is_some()) {
            return view.label().into();
        }
        let count = ctx.registry.apps.iter().filter(|a| view.includes(ctx, &a.id)).count();
        format!("{} {}", view.label(), count)
    }

    fn empty_message(&self, ctx: &ScreenContext) -> (&'static str, &'static str) {
        if self.category_index != 0 {
            return ("NO MATCHING APPS", "Press X to choose another category.");
        }
        match self.view {
            StoreView::Browse => ("YOUR STORE IS READY", "Press Y to load the app catalogue over Wi-Fi."),
            StoreView::Installed => ("YOUR LIBRARY STARTS HERE", "Use L1 / R1 to browse, then A to open app details."),
            StoreView::Updates if !ctx.registry.apps.iter().any(|a| a.package.is_some()) =>
                ("CHECK FOR UPDATES", "Press Y to check the online catalogue."),
            StoreView::Updates => ("NO APP UPDATES", "No newer app versions in this catalogue. Y checks again."),
        }
    }

    /// Get the filtered list of app indices (into registry.apps) for the current category.
    fn filtered_indices(&self, ctx: &ScreenContext) -> Vec<usize> {
        let cat = CATEGORIES[self.category_index];
        ctx.registry
            .apps
            .iter()
            .enumerate()
            .filter(|(_, app)| {
                self.view.includes(ctx, &app.id)
                    && (cat == "All" || app.category.eq_ignore_ascii_case(cat))
            })
            .map(|(i, _)| i)
            .collect()
    }
}

impl LauncherScreen for StoreScreen {
    fn update(&mut self, ctx: &mut ScreenContext) -> bool {
        self.reconcile_selection(ctx);
        // Background status banners change capacity without changing the catalogue.
        let first = self.scroll_offset;
        self.keep_visible(ctx);
        let moved = first != self.scroll_offset;
        if !self.refresh_started && !ctx.store_jobs.is_busy() {
            self.refresh_started = true;
            if ctx.settings.auto_refresh && ctx.automatic_store_refresh { ctx.refresh_registry_cached(); return true }
        }
        moved
    }

    fn handle_input(&mut self, events: &[InputEvent], ctx: &mut ScreenContext) -> ScreenAction {
        self.reconcile_selection(ctx);
        for ie in events {
            let filtered = self.filtered_indices(ctx);
            let count = filtered.len() as i32;
            if ie.action != InputAction::Press && ie.action != InputAction::Repeat {
                continue;
            }

            match ie.button {
                Button::DpadDown => {
                    if count > 0 {
                        self.selected_index = (self.selected_index + 1).min(count - 1);
                    }
                }
                Button::DpadUp => {
                    if count > 0 {
                        self.selected_index = (self.selected_index - 1).max(0);
                    }
                }
                Button::L1 => self.change_view(false),
                Button::R1 => self.change_view(true),
                Button::X if ie.action == InputAction::Press => {
                    self.category_index = (self.category_index + 1) % CATEGORIES.len();
                    self.reset_selection();
                }
                Button::A => {
                    if count > 0 {
                        if let Some(&reg_index) = filtered.get(self.selected_index as usize) {
                            return ScreenAction::Push(ScreenId::Detail(ctx.registry.apps[reg_index].id.clone()));
                        }
                    }
                }
                Button::Y if ie.action == InputAction::Press => {
                    self.refresh_started = true;
                    ctx.refresh_registry();
                }
                Button::B => {
                    return ScreenAction::Pop;
                }
                Button::Start => {
                    return ScreenAction::Push(ScreenId::Settings);
                }
                Button::Select => {
                    return ScreenAction::ShowOverlay;
                }
                _ => {}
            }
            self.reconcile_selection(ctx);
        }

        // Adjust scroll to keep selection visible
        let visible_cards = visible_rows(ctx);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }
        if self.selected_index >= self.scroll_offset + visible_cards {
            self.scroll_offset = self.selected_index - visible_cards + 1;
        }

        ScreenAction::None
    }

    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext) {
        if neo::is_neo(screen.theme) {
            self.render_neo(screen, ctx);
            return;
        }

        let theme = screen.theme;
        let filtered = self.filtered_indices(ctx);

        // -- Header (semi-transparent, grid bleeds through) --
        screen.draw_rect(
            Rect::new(0, 0, SCREEN_WIDTH, HEADER_HEIGHT as u32),
            Some(Color::RGBA(14, 14, 20, 220)),
            true,
            0,
            None,
        );
        screen.draw_glow_line(0, 0, SCREEN_WIDTH as i32 - 1, Color::RGBA(100, 180, 255, 80), 3, 1);
        screen.draw_text_glow("CartridgeOS Store", 12, 8, theme.accent, theme.glow_primary, 20, true, None);

        // App count
        let count_str = self.view_label(self.view, ctx);
        let cw = screen.get_text_width(&count_str, 13, false);
        screen.draw_text(
            &count_str,
            SCREEN_WIDTH as i32 - 12 - cw as i32,
            12,
            Some(theme.text_dim),
            13,
            false,
            None,
        );

        // -- Category tabs --
        let tab_y = CONTENT_TOP;
        let mut tab_x = 12;
        for view in VIEWS {
            let is_active = view == self.view;
            let cat = self.view_label(view, ctx);
            let cat_color = theme.accent;

            let text_color = if is_active { theme.text } else { theme.text_dim };
            let w = screen.draw_text(&cat, tab_x, tab_y + 6, Some(text_color), 13, is_active, None);
            if is_active {
                screen.draw_line(
                    (tab_x, tab_y + TAB_HEIGHT - 2),
                    (tab_x + w as i32, tab_y + TAB_HEIGHT - 2),
                    Some(cat_color),
                    2,
                );
            }
            tab_x += w as i32 + 18;

            // Don't overflow screen
            if tab_x > SCREEN_WIDTH as i32 - 40 {
                break;
            }
        }

        // Thin separator line below tabs
        screen.draw_line(
            (0, CONTENT_TOP + TAB_HEIGHT),
            (SCREEN_WIDTH as i32, CONTENT_TOP + TAB_HEIGHT),
            Some(theme.border),
            1,
        );

        screen.draw_text(&format!("X  Category: {}", CATEGORIES[self.category_index]), 12,
            CONTENT_TOP + TAB_HEIGHT + 8, Some(theme.text_dim), 12, false, None);
        // -- App list cards --
        let list_top = CONTENT_TOP + TAB_HEIGHT + FILTER_H + MARGIN;
        let card_w = SCREEN_WIDTH - 24;

        if filtered.is_empty() {
            let (msg, help) = self.empty_message(ctx);
            screen.draw_text(help, 24, list_top + 92, Some(theme.text_dim), 13, false, Some(SCREEN_WIDTH - 48));
            let mw = screen.get_text_width(msg, 14, false);
            screen.draw_text(
                msg,
                (SCREEN_WIDTH as i32 - mw as i32) / 2,
                list_top + 60,
                Some(theme.text_dim),
                14,
                false,
                None,
            );
        } else {
            for (vis_i, &reg_i) in filtered.iter().enumerate() {
                let row = vis_i as i32 - self.scroll_offset;
                if row < 0 {
                    continue;
                }
                let card_y = list_top + row * (STORE_CARD_HEIGHT + STORE_CARD_GAP);
                if row >= visible_rows(ctx) {
                    break;
                }

                let is_selected = vis_i as i32 == self.selected_index;
                let app = &ctx.registry.apps[reg_i];
                let cat_color = category_color(&app.category);

                let card_bg = if is_selected {
                    theme.card_highlight
                } else {
                    theme.card_bg
                };
                let card_border = if is_selected {
                    theme.accent
                } else {
                    theme.card_border
                };

                // Glow border on selected card
                if is_selected {
                    screen.draw_card_glow(
                        Rect::new(12, card_y, card_w, STORE_CARD_HEIGHT as u32),
                        Color::RGBA(100, 180, 255, 40),
                        CARD_RADIUS,
                        3,
                    );
                }

                // Card background
                screen.draw_card(
                    Rect::new(12, card_y, card_w, STORE_CARD_HEIGHT as u32),
                    Some(card_bg),
                    Some(card_border),
                    CARD_RADIUS,
                    is_selected,
                );

                // Category color strip (left border)
                screen.draw_rect(
                    Rect::new(12, card_y + 4, STORE_LEFT_STRIP_WIDTH, STORE_CARD_HEIGHT as u32 - 8),
                    Some(cat_color),
                    true,
                    0,
                    None,
                );

                // Icon thumbnail (if available)
                let text_x = if let Some(icon_path) = crate::ui_constants::resolve_icon_path(&app.id) {
                    let icon_sz = (STORE_CARD_HEIGHT - 12) as u32;
                    screen.draw_image(
                        &icon_path,
                        20,
                        card_y + 6,
                        Some((icon_sz, icon_sz)),
                        None,
                    );
                    20 + icon_sz as i32 + 8
                } else {
                    24
                };

                // App name
                screen.draw_text(
                    &app.name,
                    text_x,
                    card_y + 8,
                    Some(theme.text),
                    15,
                    true,
                    Some(300),
                );

                // Description
                screen.draw_text(
                    &app.description,
                    text_x,
                    card_y + 28,
                    Some(theme.text_dim),
                    12,
                    false,
                    Some(450),
                );

                // Author + version
                let meta = format!("{}  v{}", app.author, app.version);
                screen.draw_text(
                    &meta,
                    text_x,
                    card_y + 48,
                    Some(theme.text_dim),
                    11,
                    false,
                    None,
                );

                // Category pill (right side)
                let cat_upper = app.category.to_uppercase();
                let pill_w = screen.get_text_width(&cat_upper, 11, true) + 12;
                let pill_x = (SCREEN_WIDTH - 24 - 8) as i32 - pill_w as i32;
                screen.draw_pill(
                    &cat_upper,
                    pill_x,
                    card_y + 10,
                    cat_color,
                    Color::RGB(20, 20, 30),
                    11,
                );

                // Cached manifest versions avoid disk I/O during rendering.
                if ctx.installed.is_installed(&app.id) {
                    let has_update = ctx.has_update(&app.id);
                    let (label, color) = if has_update {
                        ("UPDATE", theme.text_warning)
                    } else {
                        ("INSTALLED", theme.positive)
                    };
                    let inst_w = screen.get_text_width(label, 11, true) + 12;
                    screen.draw_pill(
                        label,
                        pill_x - inst_w as i32 - 6,
                        card_y + 10,
                        color,
                        Color::RGB(20, 20, 30),
                        11,
                    );
                }
            }
        }

        // -- Footer --
        draw_store_footer(screen, !filtered.is_empty());
    }
}

impl StoreScreen {
    fn render_neo(&self, screen: &mut Screen, ctx: &ScreenContext) {
        let theme = screen.theme;
        let filtered = self.filtered_indices(ctx);

        let subtitle = format!("{} apps", ctx.registry.apps.len());
        neo::draw_header(screen, "APP STORE", &subtitle, Some(&ctx.sysinfo));
        neo::draw_bar(screen);

        let tab_y = neo::CONTENT_Y;
        let lh = screen.get_line_height(13, false) as i32;
        let ty = tab_y + (NEO_TAB_H - lh) / 2;
        let mut tab_x = neo::MARGIN_X;
        for view in VIEWS {
            let active = view == self.view;
            let label = self.view_label(view, ctx).to_uppercase();
            let w = screen.draw_text(&label, tab_x, ty,
                Some(if active { theme.text } else { theme.text_dim }), 13, active, None);
            if active { screen.fill(Rect::new(tab_x, tab_y + NEO_TAB_H - 3, w, 3), theme.accent); }
            tab_x += w as i32 + 32;
        }
        screen.fill(Rect::new(0, tab_y + NEO_TAB_H - 1, SCREEN_WIDTH, 1), theme.border);
        screen.draw_text(&format!("X  CATEGORY: {}", CATEGORIES[self.category_index].to_uppercase()),
            neo::MARGIN_X, tab_y + NEO_TAB_H + 8, Some(theme.text_dim), neo::LABEL_SIZE, false, None);
        let position = if filtered.is_empty() { "0 APPS".into() } else {
            format!("{} / {}", self.selected_index + 1, filtered.len())
        };
        neo::text_right(screen, &position, SCREEN_WIDTH as i32 - neo::MARGIN_X,
            NEO_LIST_Y - 10, theme.text_dim, neo::LABEL_SIZE, false);

        if filtered.is_empty() {
            let (title, help) = self.empty_message(ctx);
            neo::display_at_baseline(screen, title, neo::MARGIN_X + 20, NEO_LIST_Y + 100, theme.text, 36);
            screen.draw_text(help, neo::MARGIN_X + 20, NEO_LIST_Y + 122,
                Some(theme.text_dim), 14, false, Some(SCREEN_WIDTH - 76));
        }

        for (vis_i, &reg_i) in filtered.iter().enumerate() {
            let row = vis_i as i32 - self.scroll_offset;
            if row < 0 {
                continue;
            }
            if row >= visible_rows(ctx) {
                break;
            }
            let y = NEO_LIST_Y + row * NEO_ROW_H;
            let is_selected = vis_i as i32 == self.selected_index;
            let app = &ctx.registry.apps[reg_i];
            let installed = ctx.installed.is_installed(&app.id);
            let has_update = ctx.has_update(&app.id);

            let (fg, dim, num) = if is_selected {
                screen.fill(Rect::new(0, y, SCREEN_WIDTH, NEO_ROW_H as u32), theme.accent);
                (theme.bg, theme.bg, theme.bg)
            } else {
                screen.fill(Rect::new(neo::MARGIN_X, y + NEO_ROW_H - 1, SCREEN_WIDTH - neo::MARGIN_X as u32 * 2, 1), theme.border);
                (theme.text, theme.text_dim, theme.text_muted)
            };

            // Index numeral.
            neo::display_at_baseline(screen, &neo::index_label(vis_i), neo::MARGIN_X, y + 46, num, 22);

            // Icon tile.
            let tile = Rect::new(neo::MARGIN_X + 40, y + 14, 48, 48);
            if is_selected {
                screen.fill(tile, theme.bg);
            } else {
                screen.fill(tile, theme.card_bg);
                screen.draw_outline(tile, theme.border, 1);
            }
            let drew = crate::ui_constants::resolve_icon_path(&app.id)
                .map(|p| screen.draw_image(&p, tile.x() + 9, tile.y() + 9, Some((30, 30)), None))
                .unwrap_or(false);
            if !drew {
                let abbr: String = app.name.chars().take(2).collect::<String>().to_uppercase();
                let w = screen.display_text_width(&abbr, 22) as i32;
                screen.draw_display_text(&abbr, tile.x() + (48 - w) / 2, tile.y() + 12, theme.text, 22);
            }

            // Keep the title and status on one line; descriptions use the full width below.
            let status = if has_update { "Update" } else if installed { "Installed" } else if app.package.is_some() { "Get" } else { "No download" };
            let chip_kind = if is_selected { Chip::OutlineBlack } else if has_update { Chip::FilledRed } else { Chip::OutlineDim };
            let chip_w = screen.get_text_width(&status.to_uppercase(), neo::LABEL_SIZE, false) + 16;
            let chips_x = SCREEN_WIDTH as i32 - neo::MARGIN_X - chip_w as i32;
            neo::draw_chip(screen, status, chips_x, y + 10, chip_kind);
            let text_x = neo::MARGIN_X + 104;
            let text_w = SCREEN_WIDTH - neo::MARGIN_X as u32 - text_x as u32;
            let name = fit_title(screen, &app.name.to_uppercase(), (chips_x - 12 - text_x).max(1) as u32);
            neo::display_at_baseline(screen, &name, text_x, y + 30, fg, 24);
            screen.draw_text(&app.description, text_x, y + 37, Some(fg), 12, false, Some(text_w));
            let version = match ctx.installed_version(&app.id) {
                Some(current) if has_update => format!("V{current} > V{}", app.version),
                Some(current) if self.view == StoreView::Installed => format!("V{current}"),
                _ => format!("V{}", app.version),
            };
            let meta = format!("{version}  /  {}", app.category).to_uppercase();
            screen.draw_text(&meta, text_x, y + 58, Some(dim), neo::LABEL_SIZE, false, Some(text_w));
        }

        let mut hints = vec![Hint::wide("L1 / R1", "View")];
        if !filtered.is_empty() { hints.push(Hint::a("Detail")); }
        hints.extend([Hint::y("Refresh"), Hint::b("Back")]);
        neo::draw_footer(screen, &hints);
    }
}

fn visible_rows(ctx: &ScreenContext) -> i32 {
    let notice = ctx.store_jobs.progress.is_some() || !ctx.store_jobs.notices.is_empty();
    if style_of(&ctx.settings.theme_id) == UiStyle::Neo {
        let bottom = if notice { neo::FOOTER_Y - 112 } else { neo::FOOTER_Y };
        ((bottom - NEO_LIST_Y) / NEO_ROW_H).max(1)
    } else {
        let bottom = if notice { 572 } else { CONTENT_BOTTOM };
        ((bottom - CONTENT_TOP - TAB_HEIGHT - FILTER_H - MARGIN) / (STORE_CARD_HEIGHT + STORE_CARD_GAP)).max(1)
    }
}

/// Keep long catalogue names out of the state badge, including multibyte names.
fn fit_title(screen: &mut Screen, text: &str, width: u32) -> String {
    if screen.display_text_width(text, 24) <= width { return text.into(); }
    let mut boundaries: Vec<_> = text.char_indices().map(|(index, _)| index).collect();
    boundaries.push(text.len());
    let (mut lo, mut hi) = (0, boundaries.len() - 1);
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if screen.display_text_width(&format!("{}..", &text[..boundaries[mid]]), 24) <= width { lo = mid; }
        else { hi = mid - 1; }
    }
    format!("{}..", &text[..boundaries[lo]])
}

fn draw_store_footer(screen: &mut Screen, has_selection: bool) {
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
    let w = screen.draw_button_hint("L1/R1", "View", fx, footer_y + 8, Some(theme.btn_l), 12);
    fx += w as i32 + 12;
    if has_selection {
        let w = screen.draw_button_hint("A", "Detail", fx, footer_y + 8, Some(theme.btn_a), 12);
        fx += w as i32 + 12;
    }
    let w = screen.draw_button_hint("Y", "Refresh", fx, footer_y + 8, Some(theme.btn_y), 12);
    fx += w as i32 + 12;
    screen.draw_button_hint("B", "Back", fx, footer_y + 8, Some(theme.btn_b), 12);
}

#[cfg(test)]
mod store_refresh_tests {
    use super::*;
    #[test]
    fn scripted_store_does_not_auto_refresh_or_retry_each_frame() {
        let mut ctx = super::super::test_context();
        ctx.settings.auto_refresh = true;
        ctx.automatic_store_refresh = false;
        let mut screen = StoreScreen::new();
        for _ in 0..100 {
            assert!(!screen.update(&mut ctx));
            assert!(!ctx.store_jobs.is_busy());
        }
        assert!(screen.refresh_started);
        std::fs::remove_dir_all(ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }
}

#[cfg(test)]
mod library_view_tests {
    use super::*;
    use crate::data::AppEntry;

    fn entry(id: &str, version: &str, category: &str) -> AppEntry {
        serde_json::from_value(serde_json::json!({"id": id, "name": id, "version": version, "category": category})).unwrap()
    }

    #[test]
    fn installed_and_updates_use_local_versions_and_keep_selection_after_refresh() {
        let mut ctx = super::super::test_context();
        let local = entry("local", "1.0.0", "Tools");
        let ahead = entry("ahead", "3.0.0", "Media");
        let mut update = local.clone();
        update.version = "2.0.0".into();
        update.package = Some(cartridge_net::AppPackage { url: "https://example.org/app.tgz".into(), sha256: "a".repeat(64), size: 1, min_runtime: "0.6.0".into() });
        let mut older = ahead.clone(); older.version = "2.0.0".into(); older.package = update.package.clone();
        ctx.local_apps.apps = vec![local.clone(), ahead.clone()];
        ctx.installed = ctx.local_apps.installed();
        ctx.registry.apps = vec![update, entry("not-installed", "1.0.0", "Media"), older];
        let mut store = StoreScreen::new();
        assert_eq!(store.filtered_indices(&ctx), vec![0, 1, 2]);
        store.change_view(true);
        assert_eq!(store.filtered_indices(&ctx), vec![0, 2]);
        store.reconcile_selection(&ctx);
        store.selected_index = 1;
        ctx.registry.apps.swap(0, 2);
        ctx.registry_revision += 1;
        store.reconcile_selection(&ctx);
        assert_eq!(store.selected_index, 0);
        assert_eq!(store.app_order[0], ahead.id);
        store.change_view(true);
        assert_eq!(store.filtered_indices(&ctx), vec![2]); // Only the genuinely newer version.
        ctx.local_apps.apps[0].version = "2.0.0".into();
        ctx.registry_revision += 1;
        store.reconcile_selection(&ctx);
        assert!(store.app_order.is_empty());
        assert_eq!(store.selected_index, 0);
        assert_eq!(store.scroll_offset, 0);
        std::fs::remove_dir_all(ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn background_notices_keep_the_selected_row_visible() {
        let mut ctx = super::super::test_context();
        ctx.registry.apps = (0..12).map(|i| entry(&format!("app{i}"), "1.0.0", "Tools")).collect();
        let mut store = StoreScreen::new();
        store.update(&mut ctx);
        store.selected_index = 5;
        store.keep_visible(&ctx);
        ctx.store_jobs.error("Offline. Your installed apps remain available.");
        store.update(&mut ctx);
        assert!(store.selected_index < store.scroll_offset + visible_rows(&ctx));
        assert_eq!(store.selected_index, 5);
        std::fs::remove_dir_all(ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn controls_reset_hidden_filters_and_do_not_claim_updates_were_checked_offline() {
        let mut ctx = super::super::test_context();
        let mut store = StoreScreen::new();
        let press = |button| InputEvent { button, action: InputAction::Press };
        store.handle_input(&[press(Button::X)], &mut ctx);
        assert_eq!(store.category_index, 1);
        store.handle_input(&[press(Button::R1), press(Button::R1)], &mut ctx);
        assert_eq!(store.category_index, 0);
        assert_eq!(store.view, StoreView::Updates);
        assert_eq!(store.empty_message(&ctx).0, "CHECK FOR UPDATES");
        assert!(matches!(store.handle_input(&[press(Button::A)], &mut ctx), ScreenAction::None));
        store.handle_input(&[press(Button::R1)], &mut ctx);
        assert_eq!(store.view, StoreView::Browse);
        store.handle_input(&[press(Button::L1)], &mut ctx);
        assert_eq!(store.view, StoreView::Updates);
        std::fs::remove_dir_all(ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }
}
