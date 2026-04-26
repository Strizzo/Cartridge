pub mod api;
pub mod manifest;
pub mod runner;

use std::path::Path;
use std::time::Instant;

use cartridge_core::font::FontCache;
use cartridge_core::image_cache::ImageCache;
use cartridge_core::input::{Button, InputManager};
use cartridge_core::screen::{Screen, HEIGHT, WIDTH};
use cartridge_core::text_cache::TextCache;
use cartridge_core::theme::Theme;

use manifest::CartridgeManifest;
use runner::LuaAppRunner;

const TARGET_FPS: u32 = 30;

/// Run a Lua cartridge app from the given directory.
///
/// This function:
/// 1. Reads `cartridge.json` from `app_dir`
/// 2. Initializes SDL2, creates a 720x720 window
/// 3. Creates the Lua VM and registers all APIs
/// 4. Loads the entry Lua file
/// 5. Enters a 30fps game loop calling Lua lifecycle functions
/// 6. Handles Escape/window close to quit
pub fn run_lua_app(app_dir: &Path, assets_dir: &Path) -> Result<(), String> {
    let manifest = CartridgeManifest::load(app_dir)?;
    log::info!(
        "Running cartridge: {} v{} by {}",
        manifest.name,
        manifest.version,
        manifest.author,
    );

    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let joystick_subsystem = sdl_context.joystick()?;
    let _joysticks = cartridge_core::input::open_all_joysticks(&joystick_subsystem);
    let game_controller_subsystem = sdl_context.game_controller()?;
    let _controllers = cartridge_core::input::open_all_controllers(&game_controller_subsystem);

    let window_title = format!("CartridgeOS - {}", manifest.name);
    let window = video_subsystem
        .window(&window_title, WIDTH, HEIGHT)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    // No present_vsync(): unreliable on RK3326; sleep cap below provides timing.
    let mut canvas = window
        .into_canvas()
        .accelerated()
        .build()
        .map_err(|e| e.to_string())?;

    let texture_creator = canvas.texture_creator();
    let mut fonts = FontCache::new(assets_dir)?;
    fonts.prewarm();
    let mut images = ImageCache::new(&texture_creator)?;
    let mut text_cache = TextCache::new(&texture_creator);

    let theme = Theme::default();
    let mut input_manager = InputManager::new();
    if !_controllers.is_empty() {
        input_manager.set_ignore_joystick(true);
    }
    let mut event_pump = sdl_context.event_pump()?;

    let mut app = LuaAppRunner::new(app_dir, &manifest.entry, &manifest.id, &theme)?;

    // Call on_init
    app.call_init();

    let mut last_frame = Instant::now();

    'running: loop {
        let now = Instant::now();
        let dt = now.duration_since(last_frame).as_secs_f32();
        last_frame = now;

        // Collect events
        let events: Vec<sdl2::event::Event> = event_pump.poll_iter().collect();

        // Check for quit via raw SDL events (bypasses input manager).
        // This catches Select/Start regardless of GameController mapping.
        let mut raw_select = false;
        let mut raw_start = false;
        for event in &events {
            match event {
                sdl2::event::Event::Quit { .. } => break 'running,
                sdl2::event::Event::KeyDown {
                    keycode: Some(sdl2::keyboard::Keycode::Escape),
                    ..
                } => break 'running,
                // Joystick API: button 12=Select, 13=Start on R36S Plus
                sdl2::event::Event::JoyButtonDown { button_idx: 12, .. } => {
                    raw_select = true;
                }
                sdl2::event::Event::JoyButtonDown { button_idx: 13, .. } => {
                    raw_start = true;
                }
                // GameController API: Back=Select, Start=Start
                sdl2::event::Event::ControllerButtonDown { button, .. } => {
                    match button {
                        sdl2::controller::Button::Back
                        | sdl2::controller::Button::Guide => {
                            raw_select = true;
                        }
                        sdl2::controller::Button::Start => {
                            raw_start = true;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        // Exit on Select alone or Start+Select combo
        if raw_select || (raw_start && raw_select) {
            break 'running;
        }

        // Process input
        let input_events = input_manager.process_events(&events);

        // Deliver input to Lua (filter out Select so apps don't see it)
        let lua_events: Vec<_> = input_events
            .into_iter()
            .filter(|ie| ie.button != Button::Select)
            .collect();
        app.call_input(&lua_events);

        // Update
        app.call_update(dt);

        // Render
        {
            let mut screen = Screen {
                canvas: &mut canvas,
                theme: &theme,
                fonts: &mut fonts,
                images: &mut images,
                text_cache: &mut text_cache,
                texture_creator: &texture_creator,
            };

            if let Some(error_msg) = app.error() {
                let msg = error_msg.to_string();
                LuaAppRunner::render_error_screen(&mut screen, &msg);
            } else {
                app.call_render(&mut screen);

                // If render produced an error, show it on the next frame
                if let Some(error_msg) = app.error() {
                    let msg = error_msg.to_string();
                    LuaAppRunner::render_error_screen(&mut screen, &msg);
                }
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

    // Call on_destroy
    app.call_destroy();

    Ok(())
}
