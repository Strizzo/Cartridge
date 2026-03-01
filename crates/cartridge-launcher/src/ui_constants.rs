use sdl2::pixels::Color;
use std::path::PathBuf;

/// Category color mapping for app store pills and strips.
pub fn category_color(category: &str) -> Color {
    match category.to_lowercase().as_str() {
        "news" => Color::RGB(74, 158, 255),
        "finance" => Color::RGB(74, 222, 128),
        "tools" => Color::RGB(167, 139, 250),
        "productivity" => Color::RGB(251, 191, 36),
        "games" => Color::RGB(239, 68, 68),
        "social" => Color::RGB(249, 146, 60),
        "media" => Color::RGB(236, 72, 153),
        _ => Color::RGB(140, 140, 160),
    }
}

// Layout constants from the UX design document
pub const HEADER_HEIGHT: i32 = 36;
pub const FOOTER_HEIGHT: i32 = 36;
pub const CONTENT_TOP: i32 = 36;
pub const CONTENT_BOTTOM: i32 = 684;
pub const CONTENT_HEIGHT: i32 = 648;
pub const SCREEN_WIDTH: u32 = 720;
pub const SCREEN_HEIGHT: u32 = 720;
pub const PADDING: i32 = 10;
pub const MARGIN: i32 = 8;
pub const TAB_HEIGHT: i32 = 30;
pub const CARD_RADIUS: i16 = 6;

// Home screen dock constants
pub const DOCK_ICON_SIZE: u32 = 80;
pub const DOCK_ICON_FOCUSED_SIZE: u32 = 88;
pub const DOCK_Y: i32 = 56;
pub const DOCK_ROW_HEIGHT: i32 = 100;
pub const DETAIL_PANE_Y: i32 = 170;
pub const DETAIL_PANE_HEIGHT: u32 = 180;
pub const RECENT_STRIP_Y: i32 = 370;
pub const RECENT_STRIP_HEIGHT: i32 = 60;
pub const RECENT_ICON_SIZE: u32 = 40;

// Store screen constants
pub const STORE_CARD_HEIGHT: i32 = 76;
pub const STORE_CARD_GAP: i32 = 6;
pub const STORE_LEFT_STRIP_WIDTH: u32 = 3;

/// Extract the short name from a dotted app_id (e.g. "dev.cartridge.calculator" → "calculator").
pub fn app_short_name(app_id: &str) -> &str {
    app_id.rsplit('.').next().unwrap_or(app_id)
}

/// Resolve the icon.png path for an app, checking standard install and dev locations.
/// Returns Some(path_string) if the icon exists on disk.
pub fn resolve_icon_path(app_id: &str) -> Option<String> {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let cwd = std::env::current_dir().unwrap_or_default();
    let short = app_short_name(app_id);

    // Check both full app_id and short name in each location
    for name in &[app_id, short] {
        // Standard install path (~/.cartridges/apps/)
        let installed_icon = home.join(".cartridges/apps").join(name).join("icon.png");
        if installed_icon.exists() {
            return Some(installed_icon.to_string_lossy().to_string());
        }

        // Bundled/dev path (lua_cartridges/ relative to cwd)
        let dev_icon = cwd.join("lua_cartridges").join(name).join("icon.png");
        if dev_icon.exists() {
            return Some(dev_icon.to_string_lossy().to_string());
        }
    }

    None
}
