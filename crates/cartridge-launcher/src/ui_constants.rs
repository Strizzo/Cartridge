use sdl2::pixels::Color;

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

/// Build a list of name variants to try (original, hyphen, underscore).
pub fn name_variants(app_id: &str) -> Vec<String> {
    cartridge_core::paths::cartridge_name_variants(app_id)
}

use std::sync::OnceLock;
use std::sync::Mutex;
use std::collections::HashMap;

/// Process-wide cache for resolved icon paths. Resolution involves multiple
/// stat() syscalls; invalidated when a Store job changes local apps.
fn icon_path_cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Resolve the icon.png path for an app, checking bundled and install locations.
/// Invalidated after install, update, rollback or removal.
pub fn resolve_icon_path(app_id: &str) -> Option<String> {
    resolve_icon_file(app_id, "icon.png")
}

/// Resolve an arbitrary icon variant next to the cartridge (for example
/// `icon_focused.png`, the red-tile variant of the Neo-Tokyo icons).
pub fn resolve_icon_file(app_id: &str, file: &str) -> Option<String> {
    let key = format!("{app_id}\u{0}{file}");
    if let Ok(cache) = icon_path_cache().lock() {
        if let Some(cached) = cache.get(&key) {
            return cached.clone();
        }
    }

    let result = resolve_icon_path_uncached(app_id, file);

    if let Ok(mut cache) = icon_path_cache().lock() {
        cache.insert(key, result.clone());
    }
    result
}

/// Force re-resolution on next call (use after install/remove).
pub fn invalidate_icon_path(app_id: &str) {
    if let Ok(mut cache) = icon_path_cache().lock() {
        cache.retain(|k, _| !k.starts_with(&format!("{app_id}\u{0}")));
    }
}

fn resolve_icon_path_uncached(app_id: &str, file: &str) -> Option<String> {
    let path = cartridge_core::paths::resolve_cartridge_dir(app_id)?.join(file);
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

/// Paths previously cached for an app, including variants, for GPU eviction.
pub fn cached_icon_paths(app_id: &str) -> Vec<String> {
    icon_path_cache().lock().map(|cache| cache.iter()
        .filter(|(key, _)| key.starts_with(&format!("{app_id}\u{0}")))
        .filter_map(|(_, path)| path.clone()).collect()).unwrap_or_default()
}

#[cfg(test)]
mod icon_completion_tests {
    use super::*;
    use crate::store_jobs::{Completion, LocalApps};

    #[test]
    fn completion_invalidates_both_icon_variants_and_evicts_old_gpu_paths() {
        let id = "dev.cartridge.icon-completion-test";
        let entry: crate::data::AppEntry = serde_json::from_value(serde_json::json!({"id":id,"name":"Test"})).unwrap();
        let mut ctx = crate::screens::test_context();
        ctx.local_apps.apps.push(entry.clone());
        {
            let mut cache = icon_path_cache().lock().unwrap();
            cache.insert(format!("{id}\u{0}icon.png"), Some("/old/icon.png".into()));
            cache.insert(format!("{id}\u{0}icon_focused.png"), Some("/old/icon_focused.png".into()));
            cache.insert(format!("{id}\u{0}missing.png"), None);
        }
        ctx.apply_store_completion(Completion {
            local: LocalApps { apps:vec![entry], ..Default::default() }, installer:None,
            registry:None, outcome:Ok(None),
        });
        assert!(ctx.invalidated_textures.contains(&"/old/icon.png".into()));
        assert!(ctx.invalidated_textures.contains(&"/old/icon_focused.png".into()));
        assert!(!icon_path_cache().lock().unwrap().keys().any(|key| key.starts_with(&format!("{id}\u{0}"))));
        std::fs::remove_dir_all(ctx.storage.data_dir.parent().unwrap().parent().unwrap()).unwrap();
    }
}
