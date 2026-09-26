use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use sdl2::rect::Rect;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use super::{LauncherScreen, ScreenAction, ScreenContext};
use crate::games::{self, Game, GameRequest, GameSystem, Loaded};
use crate::neo::{self, Hint};

const ROWS: usize = 8;
const LIST_Y: i32 = 132;
const ROW_H: i32 = 60;

pub struct GamesScreen {
    systems: Vec<GameSystem>,
    games: Vec<Game>,
    selected_system: usize,
    selected_game: usize,
    in_system: bool,
    pending: Option<Receiver<Result<Loaded, String>>>,
    resume: Option<GameRequest>,
    error: Option<String>,
    last_art: Option<PathBuf>,
}

impl GamesScreen {
    pub fn new(resume: Option<GameRequest>) -> Self {
        Self {
            systems: vec![],
            games: vec![],
            selected_system: 0,
            selected_game: 0,
            in_system: false,
            pending: Some(games::load(None)),
            error: resume.as_ref().and_then(|r| r.error.clone()),
            resume,
            last_art: None,
        }
    }

    fn enter_system(&mut self) {
        if let Some(system) = self.systems.get(self.selected_system) {
            self.in_system = true;
            self.games.clear();
            self.selected_game = 0;
            self.pending = Some(games::load(Some(system.id.clone())));
        }
    }
}

impl LauncherScreen for GamesScreen {
    fn is_loading(&self) -> bool {
        self.pending.is_some()
    }
    fn update(&mut self, _ctx: &mut ScreenContext) -> bool {
        let Some(rx) = &self.pending else {
            return false;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(_) => Err("Library reader stopped unexpectedly".into()),
        };
        self.pending = None;
        match result {
            Ok(Loaded::Systems(systems)) => {
                self.systems = systems;
                if let Some(resume) = &self.resume {
                    if let Some(i) = self.systems.iter().position(|s| s.id == resume.system) {
                        self.selected_system = i;
                        self.enter_system();
                    } else {
                        self.resume = None;
                    }
                }
            }
            Ok(Loaded::Games(games)) => {
                self.games = games;
                if let Some(resume) = self.resume.take() {
                    self.selected_game = self
                        .games
                        .iter()
                        .position(|g| g.path == resume.path)
                        .unwrap_or(0);
                }
            }
            Err(error) => self.error = Some(error),
        }
        true
    }

    fn handle_input(&mut self, events: &[InputEvent], _ctx: &mut ScreenContext) -> ScreenAction {
        for event in events {
            if !matches!(event.action, InputAction::Press | InputAction::Repeat) {
                continue;
            }
            if event.button == Button::B {
                if self.in_system {
                    self.pending = None;
                    self.in_system = false;
                    self.error = None;
                    self.resume = None;
                } else {
                    return ScreenAction::Pop;
                }
                continue;
            }
            if event.button == Button::Select {
                return ScreenAction::ShowOverlay;
            }
            if self.pending.is_some() {
                continue;
            }
            let len = if self.in_system {
                self.games.len()
            } else {
                self.systems.len()
            };
            let index = if self.in_system {
                &mut self.selected_game
            } else {
                &mut self.selected_system
            };
            match event.button {
                Button::DpadUp => *index = index.saturating_sub(1),
                Button::DpadDown => *index = (*index + 1).min(len.saturating_sub(1)),
                Button::L1 => *index = index.saturating_sub(ROWS),
                Button::R1 => *index = (*index + ROWS).min(len.saturating_sub(1)),
                Button::Y => {
                    self.error = None;
                    if self.in_system {
                        self.enter_system();
                    } else {
                        self.pending = Some(games::load(None));
                    }
                }
                Button::A if self.in_system => {
                    if let (Some(system), Some(game)) = (
                        self.systems.get(self.selected_system),
                        self.games.get(self.selected_game),
                    ) {
                        return ScreenAction::LaunchGame(GameRequest {
                            system: system.id.clone(),
                            path: game.path.clone(),
                            error: None,
                        });
                    }
                }
                Button::A => self.enter_system(),
                _ => {}
            }
        }
        ScreenAction::None
    }

    fn render(&mut self, screen: &mut Screen, ctx: &ScreenContext) {
        let theme = screen.theme;
        screen.clear(Some(theme.bg));
        neo::draw_header(screen, "GAMES", "LIBRARY", Some(&ctx.sysinfo));
        neo::draw_bar(screen);
        let system = self.systems.get(self.selected_system);
        let heading = if self.in_system {
            system.map(|s| s.name.as_str()).unwrap_or("Games")
        } else {
            "CHOOSE A SYSTEM"
        };
        screen.draw_text(
            &heading.to_uppercase(),
            18,
            88,
            Some(theme.text),
            20,
            true,
            Some(680),
        );
        let index = if self.in_system {
            self.selected_game
        } else {
            self.selected_system
        };
        let len = if self.in_system {
            self.games.len()
        } else {
            self.systems.len()
        };
        let start = index / ROWS * ROWS;
        for i in start..len.min(start + ROWS) {
            let y = LIST_Y + (i - start) as i32 * ROW_H;
            let focused = i == index;
            screen.fill(
                Rect::new(18, y, 430, ROW_H as u32 - 4),
                if focused { theme.accent } else { theme.card_bg },
            );
            let fg = if focused { theme.bg } else { theme.text };
            let (title, subtitle) = if self.in_system {
                let game = &self.games[i];
                (
                    game.name.clone(),
                    if game.favorite {
                        "FAVORITE".into()
                    } else {
                        if game.players.is_empty() {
                            String::new()
                        } else {
                            format!("PLAYERS {}", game.players)
                        }
                    },
                )
            } else {
                (
                    self.systems[i].name.clone(),
                    self.systems[i].theme.to_uppercase(),
                )
            };
            screen.draw_text(&title, 32, y + 8, Some(fg), 17, true, Some(397));
            screen.draw_text(
                &subtitle,
                32,
                y + 33,
                Some(if focused { fg } else { theme.text_dim }),
                11,
                false,
                Some(397),
            );
        }
        screen.fill(Rect::new(466, LIST_Y, 236, 476), theme.card_bg);
        let game = if self.in_system {
            self.games.get(index)
        } else {
            None
        };
        let art = game.and_then(|g| g.image.clone());
        if self.last_art != art {
            if let Some(path) = self.last_art.take() {
                screen.images.remove(&path.to_string_lossy());
            }
            self.last_art = art.clone();
        }
        let mut drawn = false;
        if let Some(path) = art {
            let path = path.to_string_lossy();
            let size = screen.images.get(&path).map(|texture| {
                let q = texture.query();
                (q.width, q.height)
            });
            if let Some((w, h)) = size {
                let scale = (208.0 / w.max(1) as f32).min(270.0 / h.max(1) as f32);
                let (w, h) = ((w as f32 * scale) as u32, (h as f32 * scale) as u32);
                drawn = screen.draw_image(
                    &path,
                    480 + (208 - w) as i32 / 2,
                    154 + (270 - h) as i32 / 2,
                    Some((w, h)),
                    None,
                );
            }
        }
        if !drawn {
            draw_controller(screen, 508, 211);
        }
        let label = game.map(|g| g.name.as_str()).unwrap_or("YOUR COLLECTION");
        for (line, text) in neo::wrap_lines(screen, label, 16, true, 208, 3)
            .iter()
            .enumerate()
        {
            screen.draw_text(
                text,
                480,
                449 + line as i32 * 22,
                Some(theme.text),
                16,
                true,
                None,
            );
        }
        let note = if let Some(game) = game {
            if game.players.is_empty() {
                "READY TO PLAY".into()
            } else {
                format!("PLAYERS {}", game.players)
            }
        } else {
            "OPEN A SYSTEM TO PLAY".into()
        };
        screen.draw_text(&note, 480, 546, Some(theme.text_dim), 10, false, Some(208));
        let message = if self.pending.is_some() {
            "READING LIBRARY...".to_string()
        } else if let Some(e) = &self.error {
            e.clone()
        } else if len == 0 {
            "No games found. Check the configured ROM paths.".into()
        } else {
            format!(
                "{:02} / {:02}   •   {} {}",
                index + 1,
                len,
                len,
                if self.in_system { "GAMES" } else { "SYSTEMS" }
            )
        };
        screen.draw_text(
            &message,
            18,
            634,
            Some(if self.error.is_some() {
                theme.accent
            } else {
                theme.text_dim
            }),
            12,
            false,
            Some(682),
        );
        neo::draw_footer(
            screen,
            &[
                Hint::a(if self.in_system { "Play" } else { "Open" }),
                Hint::b("Back"),
                Hint::wide("L1/R1", "Page"),
                Hint::y("Refresh"),
            ],
        );
    }
}

fn draw_controller(screen: &mut Screen, x: i32, y: i32) {
    let t = screen.theme;
    // Opaque SDL fills keep the controller crisp without per-scanline gfx work.
    screen.fill(Rect::new(x + 12, y, 128, 90), t.text);
    screen.fill(Rect::new(x, y + 12, 152, 66), t.text);
    screen.fill(Rect::new(x + 22, y + 39, 42, 12), t.bg);
    screen.fill(Rect::new(x + 37, y + 24, 12, 42), t.bg);
    screen.fill(Rect::new(x + 101, y + 20, 20, 20), t.accent);
    screen.fill(Rect::new(x + 121, y + 44, 20, 20), t.bg);
    screen.fill(Rect::new(x + 68, y + 64, 15, 5), t.bg);
}
