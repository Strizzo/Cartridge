use cartridge_core::font::FontCache;
use cartridge_core::image_cache::ImageCache;
use cartridge_core::input::{Button, InputAction, InputManager};
use cartridge_core::screen::{Screen, HEIGHT, WIDTH};
use cartridge_core::theme::Theme;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use std::path::Path;
use std::time::Instant;

const TARGET_FPS: u32 = 30;

/// Runs a demo screen exercising all drawing primitives.
pub fn run_demo(assets_dir: &Path) -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let _joystick_subsystem = sdl_context.joystick()?;
    let game_controller_subsystem = sdl_context.game_controller()?;
    let _controllers = cartridge_core::input::open_all_controllers(&game_controller_subsystem);

    let window = video_subsystem
        .window("Cartridge", WIDTH, HEIGHT)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .map_err(|e| e.to_string())?;

    let texture_creator = canvas.texture_creator();
    let mut fonts = FontCache::new(assets_dir)?;
    fonts.prewarm();
    let mut images = ImageCache::new(&texture_creator)?;

    let theme = Theme::default();
    let mut input_manager = InputManager::new();
    let mut event_pump = sdl_context.event_pump()?;

    // Demo state
    let mut selected_card: i32 = 0;
    let card_count = 4;
    let mut progress: f32 = 0.0;

    let mut last_frame = Instant::now();

    'running: loop {
        let now = Instant::now();
        let dt = now.duration_since(last_frame).as_secs_f32();
        last_frame = now;

        // Collect events
        let events: Vec<sdl2::event::Event> = event_pump.poll_iter().collect();

        // Check for quit
        for event in &events {
            match event {
                sdl2::event::Event::Quit { .. } => break 'running,
                sdl2::event::Event::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }

        // Process input
        let input_events = input_manager.process_events(&events);
        for ie in &input_events {
            if ie.action == InputAction::Press || ie.action == InputAction::Repeat {
                match ie.button {
                    Button::DpadDown => selected_card = (selected_card + 1).min(card_count - 1),
                    Button::DpadUp => selected_card = (selected_card - 1).max(0),
                    _ => {}
                }
            }
        }

        // Update
        progress = (progress + dt * 0.1) % 1.0;

        // Render
        {
            let mut screen = Screen {
                canvas: &mut canvas,
                theme: &theme,
                fonts: &mut fonts,
                images: &mut images,
                texture_creator: &texture_creator,
            };

            screen.clear(None);

            // Gradient header
            screen.draw_gradient_rect(
                Rect::new(0, 0, WIDTH, 40),
                theme.header_gradient_top,
                theme.header_gradient_bottom,
            );

            // Header text
            screen.draw_text("Cartridge", 12, 10, Some(theme.text), 20, true, None);

            // WiFi indicator dot (simulated)
            screen.draw_circle(560, 20, 5, theme.positive);
            screen.draw_text("WiFi", 570, 12, Some(theme.text_dim), 13, false, None);

            // Thin accent line at top
            screen.draw_line((0, 0), (WIDTH as i32, 0), Some(theme.accent), 1);

            // Category tabs
            let tab_y = 44;
            let tab_labels = ["All", "News", "Finance", "Tools"];
            let tab_colors = [
                theme.accent,
                Color::RGB(74, 158, 255),
                Color::RGB(74, 222, 128),
                Color::RGB(167, 139, 250),
            ];
            let mut tab_x = 12;
            for (i, label) in tab_labels.iter().enumerate() {
                let is_active = i == 0;
                let text_color = if is_active {
                    theme.text
                } else {
                    theme.text_dim
                };
                let w = screen.draw_text(label, tab_x, tab_y, Some(text_color), 13, is_active, None);
                if is_active {
                    // Active indicator line
                    screen.draw_line(
                        (tab_x, tab_y + 18),
                        (tab_x + w as i32, tab_y + 18),
                        Some(tab_colors[i]),
                        2,
                    );
                }
                tab_x += w as i32 + 20;
            }

            // App cards
            let card_names = ["Hacker News", "Stock Market", "Weather", "Calculator"];
            let card_descs = [
                "Browse top stories and comments",
                "Track stocks and your watchlist",
                "Current conditions and forecast",
                "Simple calculator with history",
            ];
            let card_categories = ["NEWS", "FINANCE", "TOOLS", "TOOLS"];
            let category_colors = [
                Color::RGB(74, 158, 255),
                Color::RGB(74, 222, 128),
                Color::RGB(167, 139, 250),
                Color::RGB(167, 139, 250),
            ];

            let start_y = 70;
            let row_height = 80;

            for i in 0..card_count {
                let y = start_y + i * row_height + 4;
                let is_selected = i == selected_card;

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

                // Card
                screen.draw_card(
                    Rect::new(12, y, WIDTH - 24, row_height as u32 - 8),
                    Some(card_bg),
                    Some(card_border),
                    8,
                    is_selected,
                );

                // Category color strip (left border)
                let strip_color = category_colors[i as usize];
                screen.draw_rect(
                    Rect::new(12, y + 4, 3, row_height as u32 - 16),
                    Some(strip_color),
                    true,
                    0,
                    None,
                );

                // App name
                screen.draw_text(
                    card_names[i as usize],
                    24,
                    y + 8,
                    Some(theme.text),
                    15,
                    true,
                    None,
                );

                // Description
                screen.draw_text(
                    card_descs[i as usize],
                    24,
                    y + 28,
                    Some(theme.text_dim),
                    12,
                    false,
                    Some(450),
                );

                // Version + author
                screen.draw_text(
                    "v0.1.0  Cartridge Team",
                    24,
                    y + 46,
                    Some(theme.text_dim),
                    11,
                    false,
                    None,
                );

                // Category pill
                let cat_text = card_categories[i as usize];
                let pill_x = (WIDTH - 24 - 12) as i32
                    - screen.get_text_width(cat_text, 11, true) as i32
                    - 12;
                screen.draw_pill(
                    cat_text,
                    pill_x,
                    y + 10,
                    category_colors[i as usize],
                    Color::RGB(20, 20, 30),
                    11,
                );

                // "INSTALLED" pill for first two
                if i < 2 {
                    screen.draw_pill(
                        "INSTALLED",
                        pill_x - 90,
                        y + 10,
                        theme.positive,
                        Color::RGB(20, 20, 30),
                        11,
                    );
                }
            }

            // Progress bar at bottom
            let bar_y = start_y + card_count * row_height + 12;
            screen.draw_text("Downloading..", 12, bar_y, Some(theme.text_dim), 13, false, None);
            screen.draw_progress_bar(
                Rect::new(12, bar_y + 20, WIDTH - 24, 12),
                progress,
                None,
                None,
                3,
            );

            // Sparkline
            let spark_y = bar_y + 44;
            screen.draw_text("CPU", 12, spark_y, Some(theme.text_dim), 11, false, None);
            let spark_data: Vec<f32> = (0..30)
                .map(|i| {
                    let t = i as f32 * 0.3 + progress * 10.0;
                    50.0 + 30.0 * t.sin() + 10.0 * (t * 2.3).cos()
                })
                .collect();
            screen.draw_sparkline(
                &spark_data,
                Rect::new(40, spark_y, 200, 30),
                Some(theme.accent),
                Some(theme.border),
            );

            // Footer bar
            screen.draw_rect(
                Rect::new(0, HEIGHT as i32 - 36, WIDTH, 36),
                Some(theme.bg_header),
                true,
                0,
                None,
            );

            let mut fx = 12;
            let w = screen.draw_button_hint("Z", "Open", fx, HEIGHT as i32 - 28, Some(theme.btn_a), 12);
            fx += w as i32 + 12;
            let w = screen.draw_button_hint("X", "Back", fx, HEIGHT as i32 - 28, Some(theme.btn_b), 12);
            fx += w as i32 + 12;
            let w = screen.draw_button_hint("A/S", "Tab", fx, HEIGHT as i32 - 28, Some(theme.btn_l), 12);
            fx += w as i32 + 12;
            screen.draw_button_hint("Enter", "Settings", fx, HEIGHT as i32 - 28, Some(theme.btn_y), 12);
        }

        canvas.present();

        // Frame rate cap
        let frame_time = Instant::now().duration_since(now);
        let target_time = std::time::Duration::from_secs_f64(1.0 / TARGET_FPS as f64);
        if frame_time < target_time {
            std::thread::sleep(target_time - frame_time);
        }
    }

    Ok(())
}
