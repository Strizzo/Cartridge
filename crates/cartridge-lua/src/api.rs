use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use cartridge_core::screen::{Screen, WIDTH, HEIGHT};
use cartridge_core::storage::AppStorage;
use cartridge_core::theme::Theme;
use cartridge_net::{HttpClient, SshTunnel};
use mlua::prelude::*;
use sdl2::pixels::Color;
use sdl2::rect::Rect;

/// A wrapper that holds a raw pointer to Screen, set only during the render phase.
/// This allows Lua callbacks to access Screen drawing methods.
pub struct ScreenHandle {
    ptr: *mut ScreenErased,
}

/// Type-erased screen pointer. We store a raw pointer because Screen has complex
/// lifetimes that cannot be expressed in Lua userdata. The pointer is only valid
/// during the on_render callback.
struct ScreenErased {
    _opaque: (),
}

impl Default for ScreenHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenHandle {
    pub fn new() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
        }
    }

    /// Set the screen pointer. Caller must ensure the pointer stays valid
    /// until `clear_screen` is called.
    pub fn set_screen(&mut self, screen: &mut Screen<'_>) {
        self.ptr = screen as *mut Screen<'_> as *mut ScreenErased;
    }

    pub fn clear_screen(&mut self) {
        self.ptr = std::ptr::null_mut();
    }

    fn with_screen<F, R>(&self, f: F) -> LuaResult<R>
    where
        F: FnOnce(&mut Screen<'_>) -> R,
    {
        if self.ptr.is_null() {
            return Err(LuaError::RuntimeError(
                "screen methods can only be called during on_render()".to_string(),
            ));
        }
        // SAFETY: The pointer is valid during the on_render callback. We set it
        // before calling on_render and clear it after.
        let screen = unsafe { &mut *(self.ptr as *mut Screen<'_>) };
        Ok(f(screen))
    }
}

/// Shared handle used by both the API registration and the runner.
pub type SharedScreenHandle = Rc<RefCell<ScreenHandle>>;

pub fn new_screen_handle() -> SharedScreenHandle {
    Rc::new(RefCell::new(ScreenHandle::new()))
}

fn parse_color_table(table: &LuaTable) -> LuaResult<Color> {
    // Accept both named {r=255, g=0, b=0} and indexed {255, 0, 0} color tables.
    // Use f64 to handle Lua 5.4 float arithmetic results, then clamp to u8.
    let r: f64 = table.get("r").or_else(|_| table.get::<f64>(1))?;
    let g: f64 = table.get("g").or_else(|_| table.get::<f64>(2))?;
    let b: f64 = table.get("b").or_else(|_| table.get::<f64>(3))?;
    Ok(Color::RGB(num_u8(r), num_u8(g), num_u8(b)))
}

fn opt_color_from_table(table: &LuaTable, key: &str) -> LuaResult<Option<Color>> {
    match table.get::<LuaValue>(key)? {
        LuaValue::Table(t) => Ok(Some(parse_color_table(&t)?)),
        LuaValue::Nil => Ok(None),
        _ => Err(LuaError::RuntimeError(format!(
            "expected table or nil for '{key}'"
        ))),
    }
}

fn color_to_table(lua: &Lua, color: Color) -> LuaResult<LuaTable> {
    let table = lua.create_table()?;
    table.set("r", color.r)?;
    table.set("g", color.g)?;
    table.set("b", color.b)?;
    Ok(table)
}

// Helper functions to convert Lua numbers (which may be float due to Lua 5.4 division)
// into integer types. Lua 5.4's `/` operator always produces floats, so `720 / 2` = `360.0`.
// mlua's strict integer conversion rejects non-integer floats, causing runtime errors.
// These helpers accept any numeric value and truncate to the target type.
fn num_i32(v: f64) -> i32 {
    v as i32
}
fn num_u32(v: f64) -> u32 {
    v.max(0.0) as u32
}
fn num_i16(v: f64) -> i16 {
    v as i16
}
fn num_u16(v: f64) -> u16 {
    v.max(0.0) as u16
}
fn num_u8(v: f64) -> u8 {
    v.clamp(0.0, 255.0) as u8
}

/// Register all screen.* functions on the Lua VM.
pub fn register_screen_api(lua: &Lua, handle: SharedScreenHandle, app_dir: &Path) -> LuaResult<()> {
    let screen_table = lua.create_table()?;

    // screen.clear(r, g, b)
    {
        let h = handle.clone();
        screen_table.set(
            "clear",
            lua.create_function(move |_, (r, g, b): (f64, f64, f64)| {
                h.borrow().with_screen(|s| {
                    s.clear(Some(Color::RGB(num_u8(r), num_u8(g), num_u8(b))));
                })
            })?,
        )?;
    }

    // screen.draw_text(text, x, y, opts)
    // opts: {color={r,g,b}, size=16, bold=false, max_width=nil}
    {
        let h = handle.clone();
        screen_table.set(
            "draw_text",
            lua.create_function(
                move |_, (text, x, y, opts): (String, f64, f64, Option<LuaTable>)| {
                    let x = num_i32(x);
                    let y = num_i32(y);
                    let mut color: Option<Color> = None;
                    let mut size: u16 = 16;
                    let mut bold = false;
                    let mut max_width: Option<u32> = None;

                    if let Some(ref t) = opts {
                        color = opt_color_from_table(t, "color")?;
                        if let Ok(s) = t.get::<f64>("size") {
                            size = num_u16(s);
                        }
                        if let Ok(b) = t.get::<bool>("bold") {
                            bold = b;
                        }
                        if let Ok(mw) = t.get::<f64>("max_width") {
                            max_width = Some(num_u32(mw));
                        }
                    }

                    h.borrow().with_screen(|s| {
                        s.draw_text(&text, x, y, color, size, bold, max_width)
                    })
                },
            )?,
        )?;
    }

    // screen.draw_rect(x, y, w, h, opts)
    // opts: {color={r,g,b}, filled=true, radius=0}
    {
        let h = handle.clone();
        screen_table.set(
            "draw_rect",
            lua.create_function(
                move |_, (x, y, w, hh, opts): (f64, f64, f64, f64, Option<LuaTable>)| {
                    let x = num_i32(x);
                    let y = num_i32(y);
                    let w = num_u32(w);
                    let hh = num_u32(hh);
                    let mut color: Option<Color> = None;
                    let mut filled = true;
                    let mut radius: i16 = 0;

                    if let Some(ref t) = opts {
                        color = opt_color_from_table(t, "color")?;
                        if let Ok(f) = t.get::<bool>("filled") {
                            filled = f;
                        }
                        if let Ok(r) = t.get::<f64>("radius") {
                            radius = num_i16(r);
                        }
                    }

                    h.borrow().with_screen(|s| {
                        s.draw_rect(Rect::new(x, y, w, hh), color, filled, radius, None);
                    })
                },
            )?,
        )?;
    }

    // screen.draw_line(x1, y1, x2, y2, opts)
    // opts: {color={r,g,b}, width=1}
    {
        let h = handle.clone();
        screen_table.set(
            "draw_line",
            lua.create_function(
                move |_, (x1, y1, x2, y2, opts): (f64, f64, f64, f64, Option<LuaTable>)| {
                    let x1 = num_i32(x1);
                    let y1 = num_i32(y1);
                    let x2 = num_i32(x2);
                    let y2 = num_i32(y2);
                    let mut color: Option<Color> = None;
                    let mut width: u32 = 1;

                    if let Some(ref t) = opts {
                        color = opt_color_from_table(t, "color")?;
                        if let Ok(w) = t.get::<f64>("width") {
                            width = num_u32(w);
                        }
                    }

                    h.borrow().with_screen(|s| {
                        s.draw_line((x1, y1), (x2, y2), color, width);
                    })
                },
            )?,
        )?;
    }

    // screen.draw_card(x, y, w, h, opts)
    // opts: {bg={r,g,b}, border={r,g,b}, radius=8, shadow=true}
    {
        let h = handle.clone();
        screen_table.set(
            "draw_card",
            lua.create_function(
                move |_, (x, y, w, hh, opts): (f64, f64, f64, f64, Option<LuaTable>)| {
                    let x = num_i32(x);
                    let y = num_i32(y);
                    let w = num_u32(w);
                    let hh = num_u32(hh);
                    let mut bg: Option<Color> = None;
                    let mut border: Option<Color> = None;
                    let mut radius: i16 = 8;
                    let mut shadow = true;

                    if let Some(ref t) = opts {
                        bg = opt_color_from_table(t, "bg")?;
                        border = opt_color_from_table(t, "border")?;
                        if let Ok(r) = t.get::<f64>("radius") {
                            radius = num_i16(r);
                        }
                        if let Ok(s) = t.get::<bool>("shadow") {
                            shadow = s;
                        }
                    }

                    h.borrow().with_screen(|s| {
                        s.draw_card(Rect::new(x, y, w, hh), bg, border, radius, shadow);
                    })
                },
            )?,
        )?;
    }

    // screen.draw_gradient_rect(x, y, w, h, r1, g1, b1, r2, g2, b2)
    {
        let h = handle.clone();
        screen_table.set(
            "draw_gradient_rect",
            lua.create_function(
                move |_,
                      (x, y, w, hh, r1, g1, b1, r2, g2, b2): (
                    f64, f64, f64, f64,
                    f64, f64, f64,
                    f64, f64, f64,
                )| {
                    h.borrow().with_screen(|s| {
                        s.draw_gradient_rect(
                            Rect::new(num_i32(x), num_i32(y), num_u32(w), num_u32(hh)),
                            Color::RGB(num_u8(r1), num_u8(g1), num_u8(b1)),
                            Color::RGB(num_u8(r2), num_u8(g2), num_u8(b2)),
                        );
                    })
                },
            )?,
        )?;
    }

    // screen.draw_pill(text, x, y, bg_r, bg_g, bg_b, opts)
    // opts: {text_color={r,g,b}, size=11}
    {
        let h = handle.clone();
        screen_table.set(
            "draw_pill",
            lua.create_function(
                move |_,
                      (text, x, y, bg_r, bg_g, bg_b, opts): (
                    String,
                    f64,
                    f64,
                    f64,
                    f64,
                    f64,
                    Option<LuaTable>,
                )| {
                    let mut text_color = Color::RGB(20, 20, 30);
                    let mut size: u16 = 11;

                    if let Some(ref t) = opts {
                        if let Some(c) = opt_color_from_table(t, "text_color")? {
                            text_color = c;
                        }
                        if let Ok(s) = t.get::<f64>("size") {
                            size = num_u16(s);
                        }
                    }

                    h.borrow().with_screen(|s| {
                        s.draw_pill(&text, num_i32(x), num_i32(y), Color::RGB(num_u8(bg_r), num_u8(bg_g), num_u8(bg_b)), text_color, size)
                    })
                },
            )?,
        )?;
    }

    // screen.draw_button_hint(label, action, x, y, opts)
    // opts: {color={r,g,b}, size=12}
    {
        let h = handle.clone();
        screen_table.set(
            "draw_button_hint",
            lua.create_function(
                move |_,
                      (label, action, x, y, opts): (
                    String,
                    String,
                    f64,
                    f64,
                    Option<LuaTable>,
                )| {
                    let mut btn_color: Option<Color> = None;
                    let mut size: u16 = 12;

                    if let Some(ref t) = opts {
                        btn_color = opt_color_from_table(t, "color")?;
                        if let Ok(s) = t.get::<f64>("size") {
                            size = num_u16(s);
                        }
                    }

                    h.borrow().with_screen(|s| {
                        s.draw_button_hint(&label, &action, num_i32(x), num_i32(y), btn_color, size)
                    })
                },
            )?,
        )?;
    }

    // screen.draw_progress_bar(x, y, w, h, progress, opts)
    // opts: {fill_color={r,g,b}, bg_color={r,g,b}, radius=3}
    {
        let h = handle.clone();
        screen_table.set(
            "draw_progress_bar",
            lua.create_function(
                move |_,
                      (x, y, w, hh, progress, opts): (
                    f64,
                    f64,
                    f64,
                    f64,
                    f64,
                    Option<LuaTable>,
                )| {
                    let mut fill_color: Option<Color> = None;
                    let mut bg_color: Option<Color> = None;
                    let mut radius: i16 = 3;

                    if let Some(ref t) = opts {
                        fill_color = opt_color_from_table(t, "fill_color")?;
                        bg_color = opt_color_from_table(t, "bg_color")?;
                        if let Ok(r) = t.get::<f64>("radius") {
                            radius = num_i16(r);
                        }
                    }

                    h.borrow().with_screen(|s| {
                        s.draw_progress_bar(
                            Rect::new(num_i32(x), num_i32(y), num_u32(w), num_u32(hh)),
                            progress as f32,
                            fill_color,
                            bg_color,
                            radius,
                        );
                    })
                },
            )?,
        )?;
    }

    // screen.draw_sparkline(data_table, x, y, w, h, opts)
    // opts: {color={r,g,b}, baseline_color={r,g,b}}
    {
        let h = handle.clone();
        screen_table.set(
            "draw_sparkline",
            lua.create_function(
                move |_,
                      (data_table, x, y, w, hh, opts): (
                    LuaTable,
                    f64,
                    f64,
                    f64,
                    f64,
                    Option<LuaTable>,
                )| {
                    let mut data = Vec::new();
                    for pair in data_table.sequence_values::<f32>() {
                        data.push(pair?);
                    }

                    let mut color: Option<Color> = None;
                    let mut baseline_color: Option<Color> = None;

                    if let Some(ref t) = opts {
                        color = opt_color_from_table(t, "color")?;
                        baseline_color = opt_color_from_table(t, "baseline_color")?;
                    }

                    h.borrow().with_screen(|s| {
                        s.draw_sparkline(&data, Rect::new(num_i32(x), num_i32(y), num_u32(w), num_u32(hh)), color, baseline_color);
                    })
                },
            )?,
        )?;
    }

    // screen.draw_circle(cx, cy, radius, r, g, b)
    {
        let h = handle.clone();
        screen_table.set(
            "draw_circle",
            lua.create_function(
                move |_, (cx, cy, radius, r, g, b): (f64, f64, f64, f64, f64, f64)| {
                    h.borrow().with_screen(|s| {
                        s.draw_circle(num_i32(cx), num_i32(cy), num_i16(radius), Color::RGB(num_u8(r), num_u8(g), num_u8(b)));
                    })
                },
            )?,
        )?;
    }

    // screen.draw_rounded_rect(x, y, w, h, r, g, b, radius, shadow)
    {
        let h = handle.clone();
        screen_table.set(
            "draw_rounded_rect",
            lua.create_function(
                move |_,
                      (x, y, w, hh, r, g, b, radius, shadow): (
                    f64, f64, f64, f64,
                    f64, f64, f64,
                    f64,
                    bool,
                )| {
                    h.borrow().with_screen(|s| {
                        s.draw_rounded_rect(
                            Rect::new(num_i32(x), num_i32(y), num_u32(w), num_u32(hh)),
                            Color::RGB(num_u8(r), num_u8(g), num_u8(b)),
                            num_i16(radius),
                            shadow,
                        );
                    })
                },
            )?,
        )?;
    }

    // screen.get_text_width(text, size, bold) -> number
    {
        let h = handle.clone();
        screen_table.set(
            "get_text_width",
            lua.create_function(move |_, (text, size, bold): (String, f64, bool)| {
                let size = num_u16(size);
                h.borrow().with_screen(|s| s.get_text_width(&text, size, bold))
            })?,
        )?;
    }

    // screen.get_line_height(size, bold) -> number
    {
        let h = handle.clone();
        screen_table.set(
            "get_line_height",
            lua.create_function(move |_, (size, bold): (f64, bool)| {
                let size = num_u16(size);
                h.borrow()
                    .with_screen(|s| s.get_line_height(size, bold))
            })?,
        )?;
    }

    // screen.draw_image(path, x, y, opts)
    // opts: {w=64, h=64, src_x=0, src_y=0, src_w=32, src_h=32}
    // Path is resolved relative to the app directory (sandboxed).
    {
        let h = handle.clone();
        let app_dir = app_dir.to_path_buf();
        // Canonicalize app_dir once at registration. Per-frame draw_image
        // calls then only need to verify the leaf path is inside it,
        // and they cache canonical resolutions per-path.
        let canonical_app = app_dir.canonicalize().unwrap_or_else(|_| app_dir.clone());
        let path_cache: Rc<RefCell<std::collections::HashMap<String, Option<String>>>> =
            Rc::new(RefCell::new(std::collections::HashMap::new()));

        screen_table.set(
            "draw_image",
            lua.create_function(
                move |_, (path, x, y, opts): (String, f64, f64, Option<LuaTable>)| {
                    let x = num_i32(x);
                    let y = num_i32(y);

                    // Per-path cache: canonicalize each unique path only once.
                    let path_str = {
                        let mut cache = path_cache.borrow_mut();
                        if let Some(cached) = cache.get(&path) {
                            match cached {
                                Some(s) => s.clone(),
                                None => return Err(LuaError::RuntimeError(format!("Image not found: '{path}'"))),
                            }
                        } else {
                            let full_path = app_dir.join(&path);
                            let resolved = match full_path.canonicalize() {
                                Ok(p) if p.starts_with(&canonical_app) => {
                                    Some(p.to_string_lossy().to_string())
                                }
                                Ok(_) => {
                                    cache.insert(path.clone(), None);
                                    return Err(LuaError::RuntimeError(format!(
                                        "Image path '{path}' is outside the app directory"
                                    )));
                                }
                                Err(_) => {
                                    cache.insert(path.clone(), None);
                                    return Err(LuaError::RuntimeError(format!("Image not found: '{path}'")));
                                }
                            };
                            cache.insert(path.clone(), resolved.clone());
                            resolved.unwrap()
                        }
                    };

                    let mut dst_size: Option<(u32, u32)> = None;
                    let mut src_rect: Option<sdl2::rect::Rect> = None;

                    if let Some(ref t) = opts {
                        let w = t.get::<Option<f64>>("w").ok().flatten();
                        let hh = t.get::<Option<f64>>("h").ok().flatten();
                        if let (Some(w), Some(hh)) = (w, hh) {
                            dst_size = Some((num_u32(w), num_u32(hh)));
                        }

                        let src_x = t.get::<Option<f64>>("src_x").ok().flatten();
                        let src_y = t.get::<Option<f64>>("src_y").ok().flatten();
                        let src_w = t.get::<Option<f64>>("src_w").ok().flatten();
                        let src_h = t.get::<Option<f64>>("src_h").ok().flatten();
                        if let (Some(sx), Some(sy), Some(sw), Some(sh)) =
                            (src_x, src_y, src_w, src_h)
                        {
                            src_rect = Some(sdl2::rect::Rect::new(num_i32(sx), num_i32(sy), num_u32(sw), num_u32(sh)));
                        }
                    }

                    h.borrow().with_screen(|s| {
                        s.draw_image(&path_str, x, y, dst_size, src_rect)
                    })
                },
            )?,
        )?;
    }

    // Screen dimension constants (available from file scope, no render context needed)
    screen_table.set("width", WIDTH)?;
    screen_table.set("height", HEIGHT)?;

    lua.globals().set("screen", screen_table)?;
    Ok(())
}

/// Register the theme table as a read-only global.
pub fn register_theme_api(lua: &Lua, theme: &Theme) -> LuaResult<()> {
    let theme_table = lua.create_table()?;

    // Color fields
    let color_fields: Vec<(&str, Color)> = vec![
        ("bg", theme.bg),
        ("bg_lighter", theme.bg_lighter),
        ("bg_selected", theme.bg_selected),
        ("bg_header", theme.bg_header),
        ("card_bg", theme.card_bg),
        ("card_border", theme.card_border),
        ("card_highlight", theme.card_highlight),
        ("shadow", theme.shadow),
        ("header_gradient_top", theme.header_gradient_top),
        ("header_gradient_bottom", theme.header_gradient_bottom),
        ("text", theme.text),
        ("text_dim", theme.text_dim),
        ("text_accent", theme.text_accent),
        ("text_error", theme.text_error),
        ("text_success", theme.text_success),
        ("text_warning", theme.text_warning),
        ("accent", theme.accent),
        ("border", theme.border),
        ("btn_a", theme.btn_a),
        ("btn_b", theme.btn_b),
        ("btn_x", theme.btn_x),
        ("btn_y", theme.btn_y),
        ("btn_l", theme.btn_l),
        ("btn_r", theme.btn_r),
        ("positive", theme.positive),
        ("negative", theme.negative),
        ("orange", theme.orange),
    ];

    for (name, color) in color_fields {
        theme_table.set(name, color_to_table(lua, color)?)?;
    }

    // Numeric fields
    theme_table.set("shadow_offset", theme.shadow_offset)?;
    theme_table.set("border_radius", theme.border_radius)?;
    theme_table.set("border_radius_small", theme.border_radius_small)?;
    theme_table.set("padding", theme.padding)?;
    theme_table.set("item_height", theme.item_height)?;
    theme_table.set("header_height", theme.header_height)?;
    theme_table.set("footer_height", theme.footer_height)?;
    theme_table.set("font_size_normal", theme.font_size_normal)?;
    theme_table.set("font_size_small", theme.font_size_small)?;
    theme_table.set("font_size_large", theme.font_size_large)?;
    theme_table.set("font_size_title", theme.font_size_title)?;

    lua.globals().set("theme", theme_table)?;
    Ok(())
}

/// Register the storage table.
pub fn register_storage_api(lua: &Lua, storage: AppStorage) -> LuaResult<()> {
    let storage = Rc::new(storage);
    let storage_table = lua.create_table()?;

    // storage.save(key, data_table)
    {
        let st = storage.clone();
        storage_table.set(
            "save",
            lua.create_function(move |_, (key, value): (String, LuaValue)| {
                let json_value = lua_to_json(&value)?;
                st.save(&key, &json_value);
                Ok(())
            })?,
        )?;
    }

    // storage.load(key) -> table or nil
    {
        let st = storage.clone();
        storage_table.set(
            "load",
            lua.create_function(move |lua, key: String| match st.load(&key) {
                Some(value) => json_to_lua(lua, &value),
                None => Ok(LuaValue::Nil),
            })?,
        )?;
    }

    // storage.delete(key)
    {
        let st = storage.clone();
        storage_table.set(
            "delete",
            lua.create_function(move |_, key: String| {
                st.delete(&key);
                Ok(())
            })?,
        )?;
    }

    // storage.list_keys() -> table
    {
        let st = storage.clone();
        storage_table.set(
            "list_keys",
            lua.create_function(move |lua, ()| {
                let keys = st.list_keys();
                let table = lua.create_table()?;
                for (i, key) in keys.iter().enumerate() {
                    table.set(i + 1, key.as_str())?;
                }
                Ok(table)
            })?,
        )?;
    }

    lua.globals().set("storage", storage_table)?;
    Ok(())
}

/// Convert a Lua value to a serde_json::Value.
pub(crate) fn lua_to_json(value: &LuaValue) -> LuaResult<serde_json::Value> {
    match value {
        LuaValue::Nil => Ok(serde_json::Value::Null),
        LuaValue::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
        LuaValue::Integer(i) => Ok(serde_json::Value::Number(
            serde_json::Number::from(*i),
        )),
        LuaValue::Number(n) => {
            let num = serde_json::Number::from_f64(*n).ok_or_else(|| {
                LuaError::RuntimeError("cannot convert NaN/Infinity to JSON".to_string())
            })?;
            Ok(serde_json::Value::Number(num))
        }
        LuaValue::String(s) => Ok(serde_json::Value::String(
            s.to_str()?.to_string(),
        )),
        LuaValue::Table(t) => {
            // Determine if this is an array or a map by checking if sequential keys exist
            let len = t.raw_len();
            if len > 0 {
                // Treat as array
                let mut arr = Vec::new();
                for i in 1..=len {
                    let v: LuaValue = t.get(i)?;
                    arr.push(lua_to_json(&v)?);
                }
                Ok(serde_json::Value::Array(arr))
            } else {
                // Treat as object
                let mut map = serde_json::Map::new();
                for pair in t.pairs::<String, LuaValue>() {
                    let (k, v) = pair?;
                    map.insert(k, lua_to_json(&v)?);
                }
                Ok(serde_json::Value::Object(map))
            }
        }
        _ => Err(LuaError::RuntimeError(
            "unsupported type for JSON conversion".to_string(),
        )),
    }
}

/// Convert a serde_json::Value to a Lua value.
pub(crate) fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> LuaResult<LuaValue> {
    match value {
        serde_json::Value::Null => Ok(LuaValue::Nil),
        serde_json::Value::Bool(b) => Ok(LuaValue::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(LuaValue::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(LuaValue::Number(f))
            } else {
                Ok(LuaValue::Nil)
            }
        }
        serde_json::Value::String(s) => {
            Ok(LuaValue::String(lua.create_string(s)?))
        }
        serde_json::Value::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(table))
        }
        serde_json::Value::Object(map) => {
            let table = lua.create_table()?;
            for (k, v) in map {
                table.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(table))
        }
    }
}

/// Register the `http` global table with sync (`get`, `get_cached`, `post`)
/// and async (`get_async`, `post_async`, `poll`) methods.
///
/// The synchronous methods block the render thread — fine for one-off calls.
/// The async methods spawn a background thread, return a request id immediately,
/// and let Lua poll for the result. This keeps the UI responsive while
/// HTTP is in flight.
pub fn register_http_api(lua: &Lua, app_id: &str) -> LuaResult<()> {
    use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
    use std::sync::{Arc, Mutex};
    use std::thread;

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let cache_dir = PathBuf::from(home)
        .join(".cartridges")
        .join(app_id)
        .join("cache")
        .join("http");
    // Arc so async threads can share the client.
    let client = Arc::new(HttpClient::new(cache_dir));

    // Async request infrastructure
    struct AsyncResp {
        id: u64,
        ok: bool,
        status: u16,
        body: String,
    }
    type AsyncTx = Sender<AsyncResp>;
    type AsyncRx = Receiver<AsyncResp>;

    let (async_tx, async_rx): (AsyncTx, AsyncRx) = channel();
    let async_tx = Arc::new(Mutex::new(async_tx));
    let async_rx = Rc::new(RefCell::new(async_rx));
    let next_id = Rc::new(RefCell::new(0u64));

    let http_table = lua.create_table()?;

    // http.get(url) -> {ok, status, body} — synchronous (blocks)
    {
        let c = client.clone();
        http_table.set(
            "get",
            lua.create_function(move |lua, url: String| {
                let resp = c.get(&url).map_err(LuaError::RuntimeError)?;
                let table = lua.create_table()?;
                table.set("ok", resp.ok)?;
                table.set("status", resp.status)?;
                table.set("body", resp.body)?;
                Ok(table)
            })?,
        )?;
    }

    // http.get_cached(url, ttl_seconds) -> {ok, status, body}
    {
        let c = client.clone();
        http_table.set(
            "get_cached",
            lua.create_function(move |lua, (url, ttl): (String, u64)| {
                let resp = c
                    .get_cached(&url, ttl)
                    .map_err(LuaError::RuntimeError)?;
                let table = lua.create_table()?;
                table.set("ok", resp.ok)?;
                table.set("status", resp.status)?;
                table.set("body", resp.body)?;
                Ok(table)
            })?,
        )?;
    }

    // http.post(url, body) -> {ok, status, body} — synchronous (blocks)
    {
        let c = client.clone();
        http_table.set(
            "post",
            lua.create_function(move |lua, (url, body): (String, String)| {
                let resp = c
                    .post(&url, &body)
                    .map_err(LuaError::RuntimeError)?;
                let table = lua.create_table()?;
                table.set("ok", resp.ok)?;
                table.set("status", resp.status)?;
                table.set("body", resp.body)?;
                Ok(table)
            })?,
        )?;
    }

    // http.get_async(url) -> request_id (number)
    // Spawns a background thread; result is retrieved via http.poll().
    {
        let c = client.clone();
        let tx = async_tx.clone();
        let id_counter = next_id.clone();
        http_table.set(
            "get_async",
            lua.create_function(move |_, url: String| {
                let id = {
                    let mut n = id_counter.borrow_mut();
                    *n += 1;
                    *n
                };
                let c2 = c.clone();
                let tx2 = tx.clone();
                thread::Builder::new()
                    .name(format!("lua-http-{id}"))
                    .spawn(move || {
                        let (ok, status, body) = match c2.get(&url) {
                            Ok(r) => (r.ok, r.status, r.body),
                            Err(e) => (false, 0, e),
                        };
                        if let Ok(sender) = tx2.lock() {
                            let _ = sender.send(AsyncResp { id, ok, status, body });
                        }
                    })
                    .ok();
                Ok(id)
            })?,
        )?;
    }

    // http.post_async(url, body) -> request_id (number)
    {
        let c = client.clone();
        let tx = async_tx.clone();
        let id_counter = next_id.clone();
        http_table.set(
            "post_async",
            lua.create_function(move |_, (url, body): (String, String)| {
                let id = {
                    let mut n = id_counter.borrow_mut();
                    *n += 1;
                    *n
                };
                let c2 = c.clone();
                let tx2 = tx.clone();
                thread::Builder::new()
                    .name(format!("lua-http-post-{id}"))
                    .spawn(move || {
                        let (ok, status, resp_body) = match c2.post(&url, &body) {
                            Ok(r) => (r.ok, r.status, r.body),
                            Err(e) => (false, 0, e),
                        };
                        if let Ok(sender) = tx2.lock() {
                            let _ = sender.send(AsyncResp { id, ok, status, body: resp_body });
                        }
                    })
                    .ok();
                Ok(id)
            })?,
        )?;
    }

    // http.poll() -> array of completed responses: [{id, ok, status, body}, ...]
    // Non-blocking; returns empty table if no responses ready.
    {
        let rx = async_rx.clone();
        http_table.set(
            "poll",
            lua.create_function(move |lua, ()| {
                let table = lua.create_table()?;
                let mut idx = 1;
                let receiver = rx.borrow();
                loop {
                    match receiver.try_recv() {
                        Ok(resp) => {
                            let entry = lua.create_table()?;
                            entry.set("id", resp.id)?;
                            entry.set("ok", resp.ok)?;
                            entry.set("status", resp.status)?;
                            entry.set("body", resp.body)?;
                            table.set(idx, entry)?;
                            idx += 1;
                        }
                        Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                    }
                }
                Ok(table)
            })?,
        )?;
    }

    lua.globals().set("http", http_table)?;
    Ok(())
}

/// Register the `json` global table with decode and encode methods.
pub fn register_json_api(lua: &Lua) -> LuaResult<()> {
    let json_table = lua.create_table()?;

    // json.decode(text) -> LuaValue
    json_table.set(
        "decode",
        lua.create_function(|lua, text: String| {
            let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                LuaError::RuntimeError(format!("json.decode error: {e}"))
            })?;
            json_to_lua(lua, &value)
        })?,
    )?;

    // json.encode(value) -> String
    json_table.set(
        "encode",
        lua.create_function(|_, value: LuaValue| {
            let json_value = lua_to_json(&value)?;
            serde_json::to_string(&json_value).map_err(|e| {
                LuaError::RuntimeError(format!("json.encode error: {e}"))
            })
        })?,
    )?;

    lua.globals().set("json", json_table)?;
    Ok(())
}

/// Register the `ssh` global table with tunnel, close, and is_alive methods.
pub fn register_ssh_api(lua: &Lua) -> LuaResult<()> {
    let tunnel: Rc<RefCell<Option<SshTunnel>>> = Rc::new(RefCell::new(None));
    let ssh_table = lua.create_table()?;

    // ssh.tunnel({host, user?, key_path?, key_dir?, remote_port?}) -> {ok, local_port, error?}
    {
        let t = tunnel.clone();
        ssh_table.set(
            "tunnel",
            lua.create_function(move |lua, opts: LuaTable| {
                let host: String = opts.get("host")?;
                let user: String = opts.get::<Option<String>>("user")?.unwrap_or_default();
                let key_path: Option<String> = opts.get("key_path")?;
                let key_dir: Option<String> = opts.get("key_dir")?;
                let remote_port: u16 = opts.get::<Option<f64>>("remote_port")?
                    .map(|v| v as u16)
                    .unwrap_or(8766);

                let result = lua.create_table()?;

                match SshTunnel::open(&host, &user, key_path.as_deref(), key_dir.as_deref(), remote_port) {
                    Ok(tun) => {
                        let port = tun.local_port();
                        *t.borrow_mut() = Some(tun);
                        result.set("ok", true)?;
                        result.set("local_port", port)?;
                    }
                    Err(e) => {
                        result.set("ok", false)?;
                        result.set("error", e)?;
                    }
                }
                Ok(result)
            })?,
        )?;
    }

    // ssh.close() -> nil
    {
        let t = tunnel.clone();
        ssh_table.set(
            "close",
            lua.create_function(move |_, ()| {
                if let Some(ref mut tun) = *t.borrow_mut() {
                    tun.close();
                }
                *t.borrow_mut() = None;
                Ok(())
            })?,
        )?;
    }

    // ssh.is_alive() -> bool
    {
        let t = tunnel.clone();
        ssh_table.set(
            "is_alive",
            lua.create_function(move |_, ()| {
                let alive = match *t.borrow_mut() {
                    Some(ref mut tun) => tun.is_alive(),
                    None => false,
                };
                Ok(alive)
            })?,
        )?;
    }

    lua.globals().set("ssh", ssh_table)?;
    Ok(())
}
