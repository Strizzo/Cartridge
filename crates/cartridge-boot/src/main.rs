use cartridge_core::atmosphere::Atmosphere;
use cartridge_core::font::FontCache;
use cartridge_core::image_cache::ImageCache;
use cartridge_core::text_cache::TextCache;
use cartridge_core::input::{Button, InputAction, InputManager};
use cartridge_core::screen::{Screen, HEIGHT, WIDTH};
use cartridge_core::theme::Theme;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const TARGET_FPS: u32 = 30;
const AUTO_BOOT_SECONDS: f64 = 5.0;
const CHOICE_FILE: &str = "/tmp/.cartridge_boot_choice";

// ---------------------------------------------------------------------------
// Boot state persistence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BootState {
    last_choice: String,
    boot_count: u64,
}

impl Default for BootState {
    fn default() -> Self {
        Self {
            last_choice: "cartridge".to_string(),
            boot_count: 0,
        }
    }
}

fn state_path() -> PathBuf {
    cartridge_core::paths::cartridges_dir()
        .join("boot")
        .join("state.json")
}

fn load_state() -> BootState {
    let path = state_path();
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        BootState::default()
    }
}

fn save_state(state: &BootState) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    if let Ok(data) = serde_json::to_string_pretty(state) {
        fs::write(&path, data).ok();
    }
}

// ---------------------------------------------------------------------------
// Boot option data
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BootChoice {
    Cartridge,
    EmulationStation,
}

impl BootChoice {
    fn label(self) -> &'static str {
        match self {
            BootChoice::Cartridge => "Cartridge OS",
            BootChoice::EmulationStation => "EmulationStation",
        }
    }

    fn description(self) -> &'static str {
        match self {
            BootChoice::Cartridge => "App launcher, tools, and utilities",
            BootChoice::EmulationStation => "Retro game emulation frontend",
        }
    }

    fn key(self) -> &'static str {
        match self {
            BootChoice::Cartridge => "cartridge",
            BootChoice::EmulationStation => "emulationstation",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "emulationstation" => BootChoice::EmulationStation,
            _ => BootChoice::Cartridge,
        }
    }

    fn exit_code(self) -> i32 {
        match self {
            BootChoice::Cartridge => 0,
            BootChoice::EmulationStation => 1,
        }
    }
}

const OPTIONS: [BootChoice; 2] = [BootChoice::Cartridge, BootChoice::EmulationStation];

// ---------------------------------------------------------------------------
// Asset directory lookup (shared with main binary via cartridge_core::paths)
// ---------------------------------------------------------------------------

fn find_assets_dir() -> PathBuf {
    cartridge_core::paths::assets_dir()
}

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

/// Draw "CARTRIDGE" title with glow effect using the new draw_text_glow method.
fn draw_title_glow(screen: &mut Screen, cx: i32, y: i32) {
    let title = "CARTRIDGEOS";
    let tw = screen.get_text_width(title, 28, true);
    let tx = cx - tw as i32 / 2;
    screen.draw_text_glow(
        title,
        tx,
        y,
        screen.theme.accent,
        screen.theme.glow_primary,
        28,
        true,
        None,
    );
}

/// Draw a single option card.
fn draw_option_card(
    screen: &mut Screen,
    rect: Rect,
    choice: BootChoice,
    selected: bool,
    is_last_used: bool,
    theme: &Theme,
) {
    let bg = if selected {
        theme.card_highlight
    } else {
        theme.card_bg
    };
    let border = if selected {
        theme.accent
    } else {
        theme.card_border
    };

    screen.draw_card(rect, Some(bg), Some(border), 8, selected);

    let text_color = if selected { theme.text } else { theme.text_dim };
    let label_x = rect.x() + 16;
    let label_y = rect.y() + 12;

    screen.draw_text(choice.label(), label_x, label_y, Some(text_color), 20, true, None);
    screen.draw_text(
        choice.description(),
        label_x,
        label_y + 28,
        Some(theme.text_dim),
        13,
        false,
        None,
    );

    if is_last_used {
        let pill_x = rect.x() + rect.width() as i32 - 100;
        let pill_y = rect.y() + 14;
        screen.draw_pill("LAST USED", pill_x, pill_y, theme.positive, Color::RGB(20, 20, 30), 11);
    }

    // Selection indicator: small accent dot on the left
    if selected {
        screen.draw_circle(rect.x() + 6, rect.y() + rect.height() as i32 / 2, 3, theme.accent);
    }
}

// ---------------------------------------------------------------------------
// Neo-Tokyo boot selector
// ---------------------------------------------------------------------------

/// 26px square button capsule with a display-face letter.
fn neo_capsule(screen: &mut Screen, label: &str, x: i32, y: i32, red: bool) -> i32 {
    let theme = screen.theme;
    if label.len() == 1 {
        screen.fill(Rect::new(x, y, 26, 26), if red { theme.accent } else { theme.text });
        let w = screen.display_text_width(label, 19) as i32;
        let ascent = screen.display_ascent(19);
        screen.draw_display_text(label, x + (26 - w) / 2, y + (26 - ascent) / 2 + 1, theme.bg, 19);
        26
    } else {
        let tw = screen.get_text_width(label, 11, false) as i32;
        screen.draw_outline(Rect::new(x, y + 1, (tw + 18) as u32, 24), theme.text_dim, 1);
        let lh = screen.get_line_height(11, false) as i32;
        screen.draw_text(label, x + 9, y + 1 + (24 - lh) / 2, Some(theme.text), 11, false, None);
        tw + 18
    }
}

fn neo_hint(screen: &mut Screen, label: &str, action: &str, x: i32, y: i32, red: bool) -> i32 {
    let w = neo_capsule(screen, label, x, y, red);
    let lh = screen.get_line_height(11, false) as i32;
    let color = screen.theme.text;
    let aw = screen.draw_text(action, x + w + 9, y + (26 - lh) / 2, Some(color), 11, false, None) as i32;
    w + 9 + aw + 26
}

#[allow(clippy::too_many_arguments)]
fn render_neo(
    screen: &mut Screen,
    selected: usize,
    last_choice: BootChoice,
    auto_boot_cancelled: bool,
    elapsed: f64,
) {
    let theme = screen.theme;
    let cx = WIDTH as i32 / 2;

    // Title block: big display title over the red bar.
    let title = "CARTRIDGE";
    let tw = screen.display_text_width(title, 84) as i32;
    let ascent = screen.display_ascent(84);
    screen.draw_display_text(title, cx - tw / 2, 200 - ascent, theme.text, 84);
    screen.fill(Rect::new(0, 214, WIDTH - 150, 6), theme.accent);
    screen.draw_hazard_stripes(Rect::new(WIDTH as i32 - 150, 214, 150, 6), theme.accent, theme.bg, 20);
    let sub = "SELECT BOOT ENVIRONMENT";
    let sw = screen.get_text_width(sub, 11, false) as i32;
    screen.draw_text(sub, cx - sw / 2, 236, Some(theme.text_dim), 11, false, None);

    // Options.
    let row_w: u32 = 440;
    let row_h: i32 = 72;
    let gap: i32 = 12;
    let rows_x = cx - row_w as i32 / 2;
    let rows_y = 292;
    for (i, &choice) in OPTIONS.iter().enumerate() {
        let y = rows_y + i as i32 * (row_h + gap);
        let rect = Rect::new(rows_x, y, row_w, row_h as u32);
        let is_sel = i == selected;
        let (fg, dim) = if is_sel {
            screen.fill(rect, theme.accent);
            (theme.bg, theme.bg)
        } else {
            screen.draw_outline(rect, theme.border, 1);
            (theme.text, theme.text_dim)
        };
        let label = choice.label().to_uppercase();
        let la = screen.display_ascent(28);
        screen.draw_display_text(&label, rows_x + 20, y + 14 + (28 - la).max(0), fg, 28);
        screen.draw_text(choice.description(), rows_x + 20, y + 46, Some(dim), 11, false, None);
        if choice == last_choice {
            let tag = "LAST USED";
            let tw = screen.get_text_width(tag, 11, false) as i32;
            screen.draw_text(tag, rows_x + row_w as i32 - 20 - tw, y + 14, Some(dim), 11, false, None);
            screen.fill(Rect::new(rows_x + row_w as i32 - 20 - tw - 16, y + 17, 8, 8), fg);
        }
    }

    // Auto-boot countdown.
    if !auto_boot_cancelled {
        let remaining = (AUTO_BOOT_SECONDS - elapsed).ceil().max(1.0) as i32;
        let text = format!("AUTO-STARTING {} IN {}", last_choice.label().to_uppercase(), remaining);
        let tw = screen.get_text_width(&text, 11, false) as i32;
        let y = rows_y + OPTIONS.len() as i32 * (row_h + gap) + 26;
        screen.draw_text(&text, cx - tw / 2, y, Some(theme.text), 11, false, None);
        let bar_w: u32 = 200;
        let bar_y = y + 24;
        screen.fill(Rect::new(cx - bar_w as i32 / 2, bar_y, bar_w, 3), theme.border);
        let progress = (elapsed / AUTO_BOOT_SECONDS).clamp(0.0, 1.0) as f32;
        let fill_w = (bar_w as f32 * progress) as u32;
        if fill_w > 0 {
            screen.fill(Rect::new(cx - bar_w as i32 / 2, bar_y, fill_w, 3), theme.accent);
        }
        let cancel = "PRESS ANY BUTTON TO CANCEL";
        let cw = screen.get_text_width(cancel, 11, false) as i32;
        screen.draw_text(cancel, cx - cw / 2, bar_y + 14, Some(theme.text_dim), 11, false, None);
    }

    // Footer.
    let footer_y = HEIGHT as i32 - 48;
    screen.fill(Rect::new(0, footer_y, WIDTH, 1), theme.border);
    let cy = footer_y + 11;
    let mut hx = 18;
    hx += neo_hint(screen, "A", "SELECT", hx, cy, false);
    hx += neo_hint(screen, "B", "LAST USED", hx, cy, true);
    neo_hint(screen, "D-PAD", "NAVIGATE", hx, cy, false);
    let version = format!("V{}", env!("CARGO_PKG_VERSION"));
    let vw = screen.get_text_width(&version, 11, false) as i32;
    let lh = screen.get_line_height(11, false) as i32;
    screen.draw_text(&version, WIDTH as i32 - 18 - vw, cy + (26 - lh) / 2, Some(theme.text_dim), 11, false, None);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    env_logger::init();

    let assets_dir = find_assets_dir();
    log::info!("Boot selector starting, assets: {}", assets_dir.display());

    let exit_code = match run_boot_selector(&assets_dir) {
        Ok(choice) => {
            log::info!("Boot choice: {}", choice.key());
            fs::write(CHOICE_FILE, choice.key()).ok();

            let mut state = load_state();
            state.last_choice = choice.key().to_string();
            state.boot_count += 1;
            save_state(&state);

            choice.exit_code()
        }
        Err(e) => {
            eprintln!("Boot selector error: {e}");
            // On error, default to cartridge
            fs::write(CHOICE_FILE, "cartridge").ok();
            0
        }
    };

    std::process::exit(exit_code);
}

fn run_boot_selector(assets_dir: &Path) -> Result<BootChoice, String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let joystick_subsystem = sdl_context.joystick()?;
    let _joysticks = cartridge_core::input::open_all_joysticks(&joystick_subsystem);
    let game_controller_subsystem = sdl_context.game_controller()?;
    let _controllers = cartridge_core::input::open_all_controllers(&game_controller_subsystem);

    // Shared helper: honors CARTRIDGE_HIDDEN/SOFTWARE/SCALE/FULLSCREEN and
    // never enables vsync (unreliable on RK3326; the frame cap below paces us).
    let mut canvas = cartridge_core::window::create_canvas(
        &video_subsystem,
        "CartridgeOS Boot Selector",
        cartridge_core::window::WindowOptions::default(),
    )?;

    let texture_creator = canvas.texture_creator();
    let theme = Theme::default();
    let mut fonts = FontCache::new(assets_dir)?;
    fonts.set_family(theme.font_regular, theme.font_bold);
    fonts.set_display(theme.font_display);
    fonts.prewarm();
    let mut images = ImageCache::new(&texture_creator)?;
    let mut text_cache = TextCache::new(&texture_creator);
    let neo = theme.ui == cartridge_core::theme::UiStyle::Neo;
    let mut input_manager = InputManager::new();
    if !_controllers.is_empty() {
        input_manager.set_ignore_joystick(true);
    }
    let mut event_pump = sdl_context.event_pump()?;
    let mut atmosphere = Atmosphere::new();
    atmosphere.precompose(&mut canvas, &texture_creator, &mut images, &theme);

    // Load persisted state
    let state = load_state();
    let last_choice = BootChoice::from_key(&state.last_choice);

    // Selection state: start on the last-used option
    let mut selected: usize = OPTIONS
        .iter()
        .position(|&o| o == last_choice)
        .unwrap_or(0);

    // Auto-boot timer
    let start_time = Instant::now();
    let mut auto_boot_cancelled = false;
    let mut last_frame = Instant::now();

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_frame).as_secs_f32();
        last_frame = now;
        atmosphere.update(dt);

        // Collect events
        let events: Vec<sdl2::event::Event> = event_pump.poll_iter().collect();

        // Check for quit
        for event in &events {
            match event {
                sdl2::event::Event::Quit { .. } => return Ok(OPTIONS[selected]),
                sdl2::event::Event::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Escape),
                    ..
                } => return Ok(OPTIONS[selected]),
                _ => {}
            }
        }

        // Screenshot hotkey (F12) or SIGUSR1: captured just before present.
        let screenshot_requested = cartridge_core::screenshot::requested(&events);

        // Process input
        let input_events = input_manager.process_events(&events);
        for ie in &input_events {
            if ie.action == InputAction::Press || ie.action == InputAction::Repeat {
                // Any button press cancels auto-boot
                if !auto_boot_cancelled {
                    auto_boot_cancelled = true;
                }

                match ie.button {
                    Button::DpadUp | Button::DpadLeft => {
                        selected = selected.saturating_sub(1);
                    }
                    Button::DpadDown | Button::DpadRight => {
                        if selected < OPTIONS.len() - 1 {
                            selected += 1;
                        }
                    }
                    Button::A | Button::Start => {
                        return Ok(OPTIONS[selected]);
                    }
                    Button::B => {
                        // B selects last used and confirms immediately
                        return Ok(last_choice);
                    }
                    _ => {}
                }
            }
        }

        // Auto-boot: if not cancelled and timeout elapsed, boot last choice
        if !auto_boot_cancelled {
            let elapsed = now.duration_since(start_time).as_secs_f64();
            if elapsed >= AUTO_BOOT_SECONDS {
                return Ok(last_choice);
            }
        }

        // ---------------------------------------------------------------
        // Render
        // ---------------------------------------------------------------
        {
            let mut screen = Screen {
                canvas: &mut canvas,
                theme: &theme,
                fonts: &mut fonts,
                images: &mut images,
                text_cache: &mut text_cache,
                texture_creator: &texture_creator,
            };

            // Atmospheric background (grid, corner markers)
            atmosphere.draw_background(&mut screen);

            if neo {
                let elapsed = now.duration_since(start_time).as_secs_f64();
                render_neo(&mut screen, selected, last_choice, auto_boot_cancelled, elapsed);
                drop(screen);
                canvas.present();
                let frame_time = Instant::now().duration_since(now);
                let target_time = std::time::Duration::from_secs_f64(1.0 / TARGET_FPS as f64);
                if frame_time < target_time {
                    std::thread::sleep(target_time - frame_time);
                }
                continue;
            }

            // Title with glow
            let center_x = WIDTH as i32 / 2;
            draw_title_glow(&mut screen, center_x, 60);

            // Subtitle
            let subtitle = "Select boot environment";
            let sub_w = screen.get_text_width(subtitle, 14, false);
            screen.draw_text(
                subtitle,
                center_x - sub_w as i32 / 2,
                100,
                Some(theme.text_dim),
                14,
                false,
                None,
            );

            // Main selection card container
            let card_w: u32 = 440;
            let card_h: u32 = 80;
            let card_gap: i32 = 12;
            let total_h = (OPTIONS.len() as u32 * card_h) + ((OPTIONS.len() as u32 - 1) * card_gap as u32);
            let cards_start_y = 140;
            let cards_x = center_x - card_w as i32 / 2;

            // Outer container card (subtle border, no shadow)
            let container_pad = 16;
            let container_rect = Rect::new(
                cards_x - container_pad,
                cards_start_y - container_pad,
                card_w + (container_pad as u32 * 2),
                total_h + (container_pad as u32 * 2),
            );
            screen.draw_card(
                container_rect,
                Some(Color::RGB(22, 22, 32)),
                Some(theme.card_border),
                10,
                false,
            );

            // Draw each option card
            for (i, &choice) in OPTIONS.iter().enumerate() {
                let y = cards_start_y + (i as i32 * (card_h as i32 + card_gap));
                let rect = Rect::new(cards_x, y, card_w, card_h);
                let is_selected = i == selected;
                let is_last = choice == last_choice;
                draw_option_card(&mut screen, rect, choice, is_selected, is_last, &theme);
            }

            // Auto-boot countdown (below the cards)
            if !auto_boot_cancelled {
                let elapsed = now.duration_since(start_time).as_secs_f64();
                let remaining = (AUTO_BOOT_SECONDS - elapsed).ceil() as i32;
                let countdown_text = format!(
                    "Auto-starting {} in {}...",
                    last_choice.label(),
                    remaining.max(1)
                );
                let cw = screen.get_text_width(&countdown_text, 14, false);
                screen.draw_text(
                    &countdown_text,
                    center_x - cw as i32 / 2,
                    cards_start_y + total_h as i32 + container_pad + 20,
                    Some(theme.text_accent),
                    14,
                    false,
                    None,
                );

                // Progress bar for auto-boot countdown
                let progress = (elapsed / AUTO_BOOT_SECONDS).min(1.0) as f32;
                let bar_w: u32 = 200;
                let bar_x = center_x - bar_w as i32 / 2;
                let bar_y = cards_start_y + total_h as i32 + container_pad + 44;
                screen.draw_progress_bar(
                    Rect::new(bar_x, bar_y, bar_w, 6),
                    progress,
                    Some(theme.accent),
                    Some(theme.bg_lighter),
                    3,
                );

                // "Press any button to cancel" hint
                let cancel_text = "Press any button to cancel";
                let ctw = screen.get_text_width(cancel_text, 11, false);
                screen.draw_text(
                    cancel_text,
                    center_x - ctw as i32 / 2,
                    bar_y + 14,
                    Some(theme.text_dim),
                    11,
                    false,
                    None,
                );
            }

            // Footer bar (semi-transparent)
            let footer_y = HEIGHT as i32 - 36;
            screen.draw_rect(
                Rect::new(0, footer_y, WIDTH, 36),
                Some(Color::RGBA(14, 14, 20, 220)),
                true,
                0,
                None,
            );
            screen.draw_glow_line(footer_y, 0, WIDTH as i32 - 1, Color::RGBA(100, 180, 255, 50), 2, -1);

            // Button hints in footer
            let hint_y = footer_y + 8;
            let mut hx: i32 = 16;
            let w = screen.draw_button_hint("A", "Select", hx, hint_y, Some(theme.btn_a), 12);
            hx += w as i32 + 16;
            let w = screen.draw_button_hint("B", "Last Used", hx, hint_y, Some(theme.btn_b), 12);
            hx += w as i32 + 16;
            screen.draw_button_hint("D-Pad", "Navigate", hx, hint_y, Some(theme.btn_l), 12);

            // Version info in bottom-right
            let version_text = concat!("v", env!("CARGO_PKG_VERSION"));
            let vw = screen.get_text_width(version_text, 11, false);
            screen.draw_text(
                version_text,
                WIDTH as i32 - vw as i32 - 12,
                hint_y + 2,
                Some(theme.text_dim),
                11,
                false,
                None,
            );

            // Atmosphere overlays on top of everything
            atmosphere.draw_overlays(&mut screen);
        }

        if screenshot_requested {
            if let Err(e) = cartridge_core::screenshot::save_now(&canvas) {
                log::warn!("Screenshot failed: {e}");
            }
        }

        canvas.present();

        // Frame rate cap
        let frame_time = Instant::now().duration_since(now);
        let target_time = std::time::Duration::from_secs_f64(1.0 / TARGET_FPS as f64);
        if frame_time < target_time {
            std::thread::sleep(target_time - frame_time);
        }
    }
}
