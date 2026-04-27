use sdl2::pixels::Color;
use sdl2::render::{Texture, TextureCreator, TextureQuery};
use sdl2::video::WindowContext;
use std::collections::HashMap;

use crate::font::{FontCache, FontStyle};

/// Maximum number of cached text textures before LRU eviction.
/// 512 covers a typical busy screen (~150 texts) plus dynamic churn.
const DEFAULT_MAX_ENTRIES: usize = 512;

#[derive(Hash, Eq, PartialEq, Clone)]
struct TextKey {
    text: String,
    size: u16,
    bold: bool,
    color: (u8, u8, u8, u8),
}

struct CachedText {
    texture: Texture<'static>,
    width: u32,
    height: u32,
    last_used: u64,
}

/// LRU cache of pre-rendered text textures. Avoids re-rasterizing the
/// same string every frame. On the device, text rendering was the #1
/// CPU hotspot (120+ draw_text calls per frame, each creating a fresh
/// surface and uploading a fresh GPU texture).
pub struct TextCache {
    textures: HashMap<TextKey, CachedText>,
    max_entries: usize,
    tick: u64,
    creator_ptr: *const TextureCreator<WindowContext>,
    /// Stats: hits and misses since last reset.
    pub hits: u64,
    pub misses: u64,
}

impl TextCache {
    pub fn new(texture_creator: &TextureCreator<WindowContext>) -> Self {
        Self {
            textures: HashMap::new(),
            max_entries: DEFAULT_MAX_ENTRIES,
            tick: 0,
            creator_ptr: texture_creator as *const TextureCreator<WindowContext>,
            hits: 0,
            misses: 0,
        }
    }

    /// Drop all cached glyphs. Call after the font family changes so old
    /// rasterizations don't leak into the new look.
    pub fn clear(&mut self) {
        self.textures.clear();
    }

    /// Look up or render a text texture. Returns (width, height) of the texture.
    /// The actual texture is rendered via the provided `render_fn` if cached.
    pub fn render(
        &mut self,
        canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
        fonts: &mut FontCache,
        text: &str,
        x: i32,
        y: i32,
        color: Color,
        size: u16,
        bold: bool,
    ) -> u32 {
        if text.is_empty() {
            return 0;
        }

        self.tick = self.tick.wrapping_add(1);
        let key = TextKey {
            text: text.to_string(),
            size,
            bold,
            color: (color.r, color.g, color.b, color.a),
        };

        let cached = self.textures.get_mut(&key);
        if let Some(c) = cached {
            c.last_used = self.tick;
            let w = c.width;
            let h = c.height;
            self.hits += 1;
            canvas
                .copy(&c.texture, None, sdl2::rect::Rect::new(x, y, w, h))
                .ok();
            return w;
        }

        // Cache miss — rasterize.
        self.misses += 1;
        let style = if bold { FontStyle::MonoBold } else { FontStyle::Mono };
        let font = fonts.get(style, size);
        let surface = match font.render(text).blended(color) {
            Ok(s) => s,
            Err(_) => return 0,
        };

        // SAFETY: TextCache holds a pointer to a TextureCreator that outlives
        // it. The textures map is dropped before the creator goes away.
        let creator = unsafe { &*self.creator_ptr };
        let texture = match creator.create_texture_from_surface(&surface) {
            Ok(t) => t,
            Err(_) => return 0,
        };
        let texture: Texture<'static> = unsafe { std::mem::transmute(texture) };

        let TextureQuery { width, height, .. } = texture.query();
        canvas
            .copy(&texture, None, sdl2::rect::Rect::new(x, y, width, height))
            .ok();

        self.textures.insert(
            key,
            CachedText {
                texture,
                width,
                height,
                last_used: self.tick,
            },
        );

        // LRU eviction if over capacity.
        if self.textures.len() > self.max_entries {
            self.evict_oldest();
        }

        width
    }

    /// Get just the rendered width without drawing (for layout calculations).
    /// Uses cached value if available, otherwise queries the font directly.
    pub fn measure(&mut self, fonts: &mut FontCache, text: &str, size: u16, bold: bool) -> u32 {
        if text.is_empty() {
            return 0;
        }
        // Check any cached entry for this text/size/bold (color doesn't affect width).
        for (k, v) in &self.textures {
            if k.text == text && k.size == size && k.bold == bold {
                return v.width;
            }
        }
        // Fallback to font's size_of (FreeType metric query).
        let style = if bold { FontStyle::MonoBold } else { FontStyle::Mono };
        let font = fonts.get(style, size);
        font.size_of(text).map(|(w, _)| w).unwrap_or(0)
    }

    fn evict_oldest(&mut self) {
        // Drop ~10% of oldest entries at once to avoid frequent eviction churn.
        let drop_count = self.max_entries / 10;
        let mut entries: Vec<(TextKey, u64)> = self
            .textures
            .iter()
            .map(|(k, v)| (k.clone(), v.last_used))
            .collect();
        entries.sort_by_key(|(_, used)| *used);
        for (k, _) in entries.into_iter().take(drop_count) {
            self.textures.remove(&k);
        }
    }

    pub fn entry_count(&self) -> usize {
        self.textures.len()
    }

    pub fn reset_stats(&mut self) {
        self.hits = 0;
        self.misses = 0;
    }
}
