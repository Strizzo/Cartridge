use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::data::CATEGORIES;
use crate::ui_constants::*;
use super::{LauncherScreen, ScreenAction, ScreenContext, ScreenId};

pub struct StoreScreen {
    category_index: usize,
    selected_index: i32,
    scroll_offset: i32,
}

impl Default for StoreScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl StoreScreen {
    pub fn new() -> Self {
        Self {
            category_index: 0,
            selected_index: 0,
            scroll_offset: 0,
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
                if cat == "All" {
                    true
                } else {
                    app.category.eq_ignore_ascii_case(cat)
                }
            })
            .map(|(i, _)| i)
            .collect()
    }
}

impl LauncherScreen for StoreScreen {
    fn handle_input(&mut self, events: &[InputEvent], ctx: &mut ScreenContext) -> ScreenAction {
        let filtered = self.filtered_indices(ctx);
        let count = filtered.len() as i32;

        for ie in events {
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
                Button::L1 => {
                    if self.category_index > 0 {
                        self.category_index -= 1;
                    } else {
                        self.category_index = CATEGORIES.len() - 1;
                    }
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                }
                Button::R1 => {
                    self.category_index = (self.category_index + 1) % CATEGORIES.len();
                    self.selected_index = 0;
                    self.scroll_offset = 0;
                }
                Button::A => {
                    if count > 0 {
                        let reg_index = filtered[self.selected_index as usize];
                        return ScreenAction::Push(ScreenId::Detail(reg_index));
                    }
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
        }

        // Adjust scroll to keep selection visible
        let visible_cards = (CONTENT_HEIGHT - TAB_HEIGHT - MARGIN) / (STORE_CARD_HEIGHT + STORE_CARD_GAP);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }
        if self.selected_index >= self.scroll_offset + visible_cards {
            self.scroll_offset = self.selected_index - visible_cards + 1;
        }

        ScreenAction::None
    }

    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext) {
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
        let count_str = format!("{} apps", ctx.registry.apps.len());
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
        for (i, cat) in CATEGORIES.iter().enumerate() {
            let is_active = i == self.category_index;
            let cat_color = if *cat == "All" {
                theme.accent
            } else {
                category_color(&cat.to_lowercase())
            };

            let text_color = if is_active { theme.text } else { theme.text_dim };
            let w = screen.draw_text(cat, tab_x, tab_y + 6, Some(text_color), 13, is_active, None);
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

        // -- App list cards --
        let list_top = CONTENT_TOP + TAB_HEIGHT + MARGIN;
        let card_w = SCREEN_WIDTH - 24;

        if filtered.is_empty() {
            let msg = "No apps in this category.";
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
                if card_y + STORE_CARD_HEIGHT > CONTENT_BOTTOM {
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

                // INSTALLED pill
                if ctx.installed.is_installed(&app.id) {
                    let inst_w = screen.get_text_width("INSTALLED", 11, true) + 12;
                    screen.draw_pill(
                        "INSTALLED",
                        pill_x - inst_w as i32 - 6,
                        card_y + 10,
                        theme.positive,
                        Color::RGB(20, 20, 30),
                        11,
                    );
                }
            }
        }

        // -- Footer --
        draw_store_footer(screen);
    }
}

fn draw_store_footer(screen: &mut Screen) {
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
    let w = screen.draw_button_hint("L1/R1", "Category", fx, footer_y + 8, Some(theme.btn_l), 12);
    fx += w as i32 + 12;
    let w = screen.draw_button_hint("A", "Detail", fx, footer_y + 8, Some(theme.btn_a), 12);
    fx += w as i32 + 12;
    screen.draw_button_hint("B", "Back", fx, footer_y + 8, Some(theme.btn_b), 12);
}
