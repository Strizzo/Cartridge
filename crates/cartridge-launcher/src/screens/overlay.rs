use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use sdl2::pixels::Color;
use sdl2::rect::Rect;

use crate::ui_constants::*;

/// Boot selector overlay result.
pub enum OverlayResult {
    /// Stay visible, no action yet.
    Active,
    /// User cancelled, dismiss overlay.
    Dismiss,
    /// User selected EmulationStation.
    SwitchToES,
    /// User selected Cartridge (already active).
    StayCartridge,
}

pub struct BootOverlay {
    selected: usize, // 0 = EmulationStation, 1 = Cartridge OS
}

impl Default for BootOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl BootOverlay {
    pub fn new() -> Self {
        Self { selected: 1 } // Default to Cartridge (already active)
    }

    pub fn handle_input(&mut self, events: &[InputEvent]) -> OverlayResult {
        for ie in events {
            if ie.action != InputAction::Press {
                continue;
            }
            match ie.button {
                Button::DpadUp => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                }
                Button::DpadDown => {
                    if self.selected < 1 {
                        self.selected += 1;
                    }
                }
                Button::A => {
                    return if self.selected == 0 {
                        OverlayResult::SwitchToES
                    } else {
                        OverlayResult::StayCartridge
                    };
                }
                Button::B => {
                    return OverlayResult::Dismiss;
                }
                _ => {}
            }
        }
        OverlayResult::Active
    }

    pub fn render(&self, screen: &mut Screen) {
        let theme = screen.theme;

        // Semi-transparent dark overlay (simulate with a dark rectangle)
        screen.draw_rect(
            Rect::new(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT),
            Some(Color::RGBA(10, 10, 16, 200)),
            true,
            0,
            None,
        );

        // Centered card
        let card_w = 340_u32;
        let card_h = 180_u32;
        let card_x = (SCREEN_WIDTH as i32 - card_w as i32) / 2;
        let card_y = (SCREEN_HEIGHT as i32 - card_h as i32) / 2;

        screen.draw_card(
            Rect::new(card_x, card_y, card_w, card_h),
            Some(theme.card_bg),
            Some(theme.accent),
            CARD_RADIUS,
            true,
        );

        // Title
        let title = "Switch Environment";
        let tw = screen.get_text_width(title, 16, true);
        screen.draw_text(
            title,
            card_x + (card_w as i32 - tw as i32) / 2,
            card_y + 16,
            Some(theme.text),
            16,
            true,
            None,
        );

        // Separator line
        screen.draw_line(
            (card_x + 16, card_y + 44),
            (card_x + card_w as i32 - 16, card_y + 44),
            Some(theme.border),
            1,
        );

        // Options
        let options = ["EmulationStation", "Cartridge OS"];
        for (i, label) in options.iter().enumerate() {
            let oy = card_y + 56 + i as i32 * 36;
            let is_sel = i == self.selected;

            if is_sel {
                // Highlight row
                screen.draw_rounded_rect(
                    Rect::new(card_x + 16, oy, card_w - 32, 30),
                    theme.card_highlight,
                    4,
                    false,
                );

                // Selection arrow
                screen.draw_text(
                    ">",
                    card_x + 24,
                    oy + 6,
                    Some(theme.accent),
                    14,
                    true,
                    None,
                );
            }

            let text_color = if is_sel { theme.text } else { theme.text_dim };
            screen.draw_text(label, card_x + 42, oy + 6, Some(text_color), 14, false, None);

            // Active indicator for Cartridge
            if i == 1 {
                screen.draw_circle(
                    card_x + card_w as i32 - 36,
                    oy + 15,
                    4,
                    theme.positive,
                );
            }
        }

        // Button hints at bottom of card
        let hint_y = card_y + card_h as i32 - 30;
        let mut hx = card_x + 20;
        let w = screen.draw_button_hint("A", "Select", hx, hint_y, Some(theme.btn_a), 11);
        hx += w as i32 + 16;
        screen.draw_button_hint("B", "Cancel", hx, hint_y, Some(theme.btn_b), 11);
    }
}
