use std::path::Path;

use cartridge_core::input::{Button, InputAction, InputEvent};
use cartridge_core::screen::Screen;
use cartridge_core::storage::AppStorage;
use cartridge_core::theme::Theme;
use mlua::prelude::*;

use crate::api::{
    new_screen_handle, register_http_api, register_json_api, register_screen_api,
    register_ssh_api, register_storage_api, register_theme_api, SharedScreenHandle,
};

/// Runs a Lua cartridge app within a Lua VM.
pub struct LuaAppRunner {
    lua: Lua,
    screen_handle: SharedScreenHandle,
    has_error: Option<String>,
}

impl LuaAppRunner {
    /// Create a new runner, register all APIs, and load the Lua source file.
    pub fn new(
        app_dir: &Path,
        entry_file: &str,
        app_id: &str,
        theme: &Theme,
    ) -> Result<Self, String> {
        let lua = Lua::new();
        let screen_handle = new_screen_handle();

        // Sandbox: remove dangerous functions
        Self::sandbox(&lua).map_err(|e| format!("Failed to sandbox Lua: {e}"))?;

        // Set up restricted require that only loads from the app directory
        Self::setup_require(&lua, app_dir).map_err(|e| format!("Failed to setup require: {e}"))?;

        // Register APIs
        register_screen_api(&lua, screen_handle.clone(), app_dir)
            .map_err(|e| format!("Failed to register screen API: {e}"))?;
        register_theme_api(&lua, theme)
            .map_err(|e| format!("Failed to register theme API: {e}"))?;

        let storage = AppStorage::new(app_id);
        register_storage_api(&lua, storage)
            .map_err(|e| format!("Failed to register storage API: {e}"))?;

        register_http_api(&lua, app_id)
            .map_err(|e| format!("Failed to register HTTP API: {e}"))?;
        register_json_api(&lua)
            .map_err(|e| format!("Failed to register JSON API: {e}"))?;
        register_ssh_api(&lua)
            .map_err(|e| format!("Failed to register SSH API: {e}"))?;

        // Register screen dimension constants
        lua.globals()
            .set(
                "SCREEN_WIDTH",
                cartridge_core::screen::WIDTH,
            )
            .map_err(|e| format!("Failed to set SCREEN_WIDTH: {e}"))?;
        lua.globals()
            .set(
                "SCREEN_HEIGHT",
                cartridge_core::screen::HEIGHT,
            )
            .map_err(|e| format!("Failed to set SCREEN_HEIGHT: {e}"))?;

        // Load the entry file
        let entry_path = app_dir.join(entry_file);
        let source = std::fs::read_to_string(&entry_path).map_err(|e| {
            format!(
                "Failed to read {}: {e}",
                entry_path.display()
            )
        })?;

        lua.load(&source)
            .set_name(entry_file)
            .exec()
            .map_err(|e| format!("Lua load error: {e}"))?;

        Ok(Self {
            lua,
            screen_handle,
            has_error: None,
        })
    }

    fn sandbox(lua: &Lua) -> LuaResult<()> {
        let globals = lua.globals();

        // Remove os.execute, os.rename, os.remove, os.tmpname
        if let Ok(os_table) = globals.get::<LuaTable>("os") {
            os_table.set("execute", LuaValue::Nil)?;
            os_table.set("rename", LuaValue::Nil)?;
            os_table.set("remove", LuaValue::Nil)?;
            os_table.set("tmpname", LuaValue::Nil)?;
            os_table.set("exit", LuaValue::Nil)?;
            os_table.set("getenv", LuaValue::Nil)?;
            os_table.set("setlocale", LuaValue::Nil)?;
        }

        // Remove io library entirely
        globals.set("io", LuaValue::Nil)?;

        // Remove debug library
        globals.set("debug", LuaValue::Nil)?;

        // Remove loadfile and dofile (we provide a restricted require instead)
        globals.set("loadfile", LuaValue::Nil)?;
        globals.set("dofile", LuaValue::Nil)?;

        Ok(())
    }

    fn setup_require(lua: &Lua, app_dir: &Path) -> LuaResult<()> {
        let app_dir = app_dir.to_path_buf();

        let require_fn = lua.create_function(move |lua, module_name: String| {
            // Convert module name dots to path separators
            let relative_path = module_name.replace('.', "/");
            let file_path = app_dir.join(format!("{relative_path}.lua"));

            // Security: ensure the resolved path is within the app directory
            let canonical_app = app_dir.canonicalize().map_err(|e| {
                LuaError::RuntimeError(format!(
                    "Cannot resolve app directory: {e}"
                ))
            })?;
            let canonical_file = file_path.canonicalize().map_err(|_| {
                LuaError::RuntimeError(format!(
                    "Module not found: '{module_name}'"
                ))
            })?;
            if !canonical_file.starts_with(&canonical_app) {
                return Err(LuaError::RuntimeError(format!(
                    "Module '{module_name}' is outside the app directory"
                )));
            }

            // Check the loaded table first to avoid double-loading
            let loaded: LuaTable = lua
                .globals()
                .get::<LuaTable>("package")?
                .get::<LuaTable>("loaded")?;

            if let Ok(existing) = loaded.get::<LuaValue>(&*module_name)
                && existing != LuaValue::Nil {
                    return Ok(existing);
                }

            let source = std::fs::read_to_string(&canonical_file).map_err(|e| {
                LuaError::RuntimeError(format!(
                    "Failed to read module '{module_name}': {e}"
                ))
            })?;

            let result: LuaValue = lua
                .load(&source)
                .set_name(&module_name)
                .eval()
                .map_err(|e| {
                    LuaError::RuntimeError(format!(
                        "Error loading module '{module_name}': {e}"
                    ))
                })?;

            // Store in package.loaded
            let store_value = if result == LuaValue::Nil {
                LuaValue::Boolean(true)
            } else {
                result.clone()
            };
            loaded.set(module_name, store_value)?;

            Ok(result)
        })?;

        lua.globals().set("require", require_fn)?;
        Ok(())
    }

    /// Call on_init() if defined.
    pub fn call_init(&mut self) {
        if let Err(e) = self.try_call_init() {
            log::error!("Lua on_init error: {e}");
            self.has_error = Some(format!("on_init error: {e}"));
        }
    }

    fn try_call_init(&self) -> LuaResult<()> {
        let globals = self.lua.globals();
        if let Ok(func) = globals.get::<LuaFunction>("on_init") {
            func.call::<()>(())?;
        }
        Ok(())
    }

    /// Call on_input(button, action) for each input event.
    pub fn call_input(&mut self, events: &[InputEvent]) {
        for event in events {
            if let Err(e) = self.try_call_input(event) {
                log::error!("Lua on_input error: {e}");
                self.has_error = Some(format!("on_input error: {e}"));
            }
        }
    }

    fn try_call_input(&self, event: &InputEvent) -> LuaResult<()> {
        let globals = self.lua.globals();
        if let Ok(func) = globals.get::<LuaFunction>("on_input") {
            let button_str = button_to_str(event.button);
            let action_str = action_to_str(event.action);
            func.call::<()>((button_str, action_str))?;
        }
        Ok(())
    }

    /// Call on_update(dt) with delta time in seconds.
    pub fn call_update(&mut self, dt: f32) {
        if let Err(e) = self.try_call_update(dt) {
            log::error!("Lua on_update error: {e}");
            self.has_error = Some(format!("on_update error: {e}"));
        }
    }

    fn try_call_update(&self, dt: f32) -> LuaResult<()> {
        let globals = self.lua.globals();
        if let Ok(func) = globals.get::<LuaFunction>("on_update") {
            func.call::<()>(dt)?;
        }
        Ok(())
    }

    /// Call on_render() with the screen handle active.
    pub fn call_render(&mut self, screen: &mut Screen<'_>) {
        // Set the screen pointer so Lua screen.* calls work
        self.screen_handle.borrow_mut().set_screen(screen);

        if let Err(e) = self.try_call_render() {
            log::error!("Lua on_render error: {e}");
            self.has_error = Some(format!("on_render error: {e}"));
        }

        // Clear the screen pointer — it's no longer valid after this frame
        self.screen_handle.borrow_mut().clear_screen();
    }

    fn try_call_render(&self) -> LuaResult<()> {
        let globals = self.lua.globals();
        if let Ok(func) = globals.get::<LuaFunction>("on_render") {
            func.call::<()>(())?;
        }
        Ok(())
    }

    /// Call on_destroy() if defined.
    pub fn call_destroy(&mut self) {
        if let Err(e) = self.try_call_destroy() {
            log::error!("Lua on_destroy error: {e}");
        }
    }

    fn try_call_destroy(&self) -> LuaResult<()> {
        let globals = self.lua.globals();
        if let Ok(func) = globals.get::<LuaFunction>("on_destroy") {
            func.call::<()>(())?;
        }
        Ok(())
    }

    /// Returns the current error message if the app has encountered an error.
    pub fn error(&self) -> Option<&str> {
        self.has_error.as_deref()
    }

    /// Clear the error state (e.g., after displaying it).
    pub fn clear_error(&mut self) {
        self.has_error = None;
    }

    /// Render an error screen when the Lua app has encountered an error.
    pub fn render_error_screen(screen: &mut Screen<'_>, error_msg: &str) {
        screen.clear(Some(sdl2::pixels::Color::RGB(30, 10, 10)));

        screen.draw_text(
            "Lua Error",
            20,
            20,
            Some(sdl2::pixels::Color::RGB(255, 100, 100)),
            20,
            true,
            None,
        );

        screen.draw_line(
            (20, 48),
            (620, 48),
            Some(sdl2::pixels::Color::RGB(100, 40, 40)),
            1,
        );

        // Word-wrap error message for display
        let max_chars_per_line = 70;
        let mut y = 60;
        let mut remaining = error_msg;

        while !remaining.is_empty() && y < 440 {
            let line_end = remaining
                .char_indices()
                .take(max_chars_per_line)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(remaining.len());

            let (line, rest) = remaining.split_at(line_end.min(remaining.len()));
            screen.draw_text(
                line,
                20,
                y,
                Some(sdl2::pixels::Color::RGB(220, 180, 180)),
                14,
                false,
                None,
            );
            y += 20;
            remaining = rest;
        }

        screen.draw_text(
            "Press Escape to quit",
            20,
            450,
            Some(sdl2::pixels::Color::RGB(150, 150, 160)),
            13,
            false,
            None,
        );
    }
}

fn button_to_str(button: Button) -> &'static str {
    match button {
        Button::DpadUp => "dpad_up",
        Button::DpadDown => "dpad_down",
        Button::DpadLeft => "dpad_left",
        Button::DpadRight => "dpad_right",
        Button::A => "a",
        Button::B => "b",
        Button::X => "x",
        Button::Y => "y",
        Button::L1 => "l1",
        Button::R1 => "r1",
        Button::L2 => "l2",
        Button::R2 => "r2",
        Button::Start => "start",
        Button::Select => "select",
    }
}

fn action_to_str(action: InputAction) -> &'static str {
    match action {
        InputAction::Press => "press",
        InputAction::Release => "release",
        InputAction::Repeat => "repeat",
    }
}
