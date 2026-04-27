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
    /// User asked to reboot the device.
    Reboot,
    /// User asked to shut down the device.
    Shutdown,
}

/// Items displayed in the overlay, in order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    EmulationStation,
    CartridgeOS,
    Restart,
    Shutdown,
}

const ITEMS: &[Item] = &[
    Item::EmulationStation,
    Item::CartridgeOS,
    Item::Restart,
    Item::Shutdown,
];

/// Confirmation dialog state for destructive items.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Confirming {
    None,
    Restart,
    Shutdown,
}

pub struct BootOverlay {
    selected: usize,
    confirming: Confirming,
    /// 0 = No (default), 1 = Yes -- inside the confirmation dialog.
    confirm_choice: usize,
}

impl Default for BootOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl BootOverlay {
    pub fn new() -> Self {
        // Default to Cartridge (already active) on first open.
        Self {
            selected: 1,
            confirming: Confirming::None,
            confirm_choice: 0,
        }
    }

    pub fn handle_input(&mut self, events: &[InputEvent]) -> OverlayResult {
        for ie in events {
            if ie.action != InputAction::Press {
                continue;
            }
            // Confirmation dialog has its own input handling.
            if self.confirming != Confirming::None {
                match ie.button {
                    Button::DpadLeft | Button::DpadRight => {
                        self.confirm_choice = 1 - self.confirm_choice;
                    }
                    Button::A => {
                        let action = self.confirming;
                        let yes = self.confirm_choice == 1;
                        self.confirming = Confirming::None;
                        self.confirm_choice = 0;
                        if yes {
                            return match action {
                                Confirming::Restart => OverlayResult::Reboot,
                                Confirming::Shutdown => OverlayResult::Shutdown,
                                Confirming::None => OverlayResult::Active,
                            };
                        }
                        // No -> just close confirm, stay on overlay.
                        return OverlayResult::Active;
                    }
                    Button::B => {
                        self.confirming = Confirming::None;
                        self.confirm_choice = 0;
                    }
                    _ => {}
                }
                continue;
            }

            match ie.button {
                Button::DpadUp => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                }
                Button::DpadDown => {
                    if self.selected + 1 < ITEMS.len() {
                        self.selected += 1;
                    }
                }
                Button::A => {
                    return match ITEMS[self.selected] {
                        Item::EmulationStation => OverlayResult::SwitchToES,
                        Item::CartridgeOS => OverlayResult::StayCartridge,
                        Item::Restart => {
                            self.confirming = Confirming::Restart;
                            OverlayResult::Active
                        }
                        Item::Shutdown => {
                            self.confirming = Confirming::Shutdown;
                            OverlayResult::Active
                        }
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

        // Semi-transparent dark backdrop.
        screen.draw_rect(
            Rect::new(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT),
            Some(Color::RGBA(10, 10, 16, 200)),
            true,
            0,
            None,
        );

        let card_w = 360_u32;
        let card_h = 280_u32;
        let card_x = (SCREEN_WIDTH as i32 - card_w as i32) / 2;
        let card_y = (SCREEN_HEIGHT as i32 - card_h as i32) / 2;

        screen.draw_card(
            Rect::new(card_x, card_y, card_w, card_h),
            Some(theme.card_bg),
            Some(theme.accent),
            CARD_RADIUS,
            true,
        );

        // Title.
        let title = "System Menu";
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

        // Separator.
        screen.draw_line(
            (card_x + 16, card_y + 44),
            (card_x + card_w as i32 - 16, card_y + 44),
            Some(theme.border),
            1,
        );

        // Items.
        for (i, item) in ITEMS.iter().enumerate() {
            let oy = card_y + 56 + i as i32 * 36;
            let is_sel = i == self.selected;

            if is_sel {
                screen.draw_rounded_rect(
                    Rect::new(card_x + 16, oy, card_w - 32, 30),
                    theme.card_highlight,
                    4,
                    false,
                );
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

            let label = match item {
                Item::EmulationStation => "EmulationStation",
                Item::CartridgeOS => "Cartridge OS",
                Item::Restart => "Restart",
                Item::Shutdown => "Shut Down",
            };
            let label_color = match item {
                Item::Shutdown => theme.negative,
                Item::Restart => theme.text_warning,
                _ => {
                    if is_sel { theme.text } else { theme.text_dim }
                }
            };
            screen.draw_text(label, card_x + 42, oy + 6, Some(label_color), 14, false, None);

            // Active indicator next to Cartridge OS row.
            if matches!(item, Item::CartridgeOS) {
                screen.draw_circle(
                    card_x + card_w as i32 - 36,
                    oy + 15,
                    4,
                    theme.positive,
                );
            }
        }

        // Button hints.
        let hint_y = card_y + card_h as i32 - 30;
        let mut hx = card_x + 20;
        let w = screen.draw_button_hint("A", "Select", hx, hint_y, Some(theme.btn_a), 11);
        hx += w as i32 + 16;
        screen.draw_button_hint("B", "Cancel", hx, hint_y, Some(theme.btn_b), 11);

        // Confirmation overlay -- drawn on top of everything else.
        if self.confirming != Confirming::None {
            self.render_confirm(screen);
        }
    }

    fn render_confirm(&self, screen: &mut Screen) {
        let theme = screen.theme;

        // Darken the underlying menu.
        screen.draw_rect(
            Rect::new(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT),
            Some(Color::RGBA(0, 0, 0, 160)),
            true,
            0,
            None,
        );

        let card_w = 320_u32;
        let card_h = 140_u32;
        let card_x = (SCREEN_WIDTH as i32 - card_w as i32) / 2;
        let card_y = (SCREEN_HEIGHT as i32 - card_h as i32) / 2;

        let border = match self.confirming {
            Confirming::Shutdown => theme.negative,
            _ => theme.text_warning,
        };
        screen.draw_card(
            Rect::new(card_x, card_y, card_w, card_h),
            Some(theme.card_bg),
            Some(border),
            CARD_RADIUS,
            true,
        );

        let prompt = match self.confirming {
            Confirming::Restart => "Restart device?",
            Confirming::Shutdown => "Shut down device?",
            Confirming::None => "",
        };
        let pw = screen.get_text_width(prompt, 15, true);
        screen.draw_text(
            prompt,
            card_x + (card_w as i32 - pw as i32) / 2,
            card_y + 22,
            Some(theme.text),
            15,
            true,
            None,
        );

        // No / Yes buttons.
        let labels = ["No", "Yes"];
        let btn_w = 100_u32;
        let btn_h = 34_u32;
        let total_w = btn_w * 2 + 20;
        let start_x = card_x + (card_w as i32 - total_w as i32) / 2;
        let by = card_y + card_h as i32 - 50;

        for (i, label) in labels.iter().enumerate() {
            let bx = start_x + i as i32 * (btn_w as i32 + 20);
            let is_sel = i == self.confirm_choice;
            let bg = if is_sel { theme.card_highlight } else { theme.card_bg };
            let border = if is_sel { theme.accent } else { theme.card_border };
            screen.draw_card(
                Rect::new(bx, by, btn_w, btn_h),
                Some(bg),
                Some(border),
                4,
                is_sel,
            );
            let lw = screen.get_text_width(label, 14, true);
            screen.draw_text(
                label,
                bx + (btn_w as i32 - lw as i32) / 2,
                by + 8,
                Some(theme.text),
                14,
                true,
                None,
            );
        }
    }
}
