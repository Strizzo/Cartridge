use sdl2::pixels::Color;
use sdl2::render::{Texture, TextureCreator, TextureQuery};
use sdl2::video::WindowContext;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use crate::font::{FontCache, FontStyle};

/// Maximum number of cached text textures before LRU eviction.
/// 512 covers a typical busy screen (~150 texts) plus dynamic churn.
const DEFAULT_MAX_ENTRIES: usize = 512;

/// The width index also learns strings that are measured but never drawn
/// (word-wrap probes), so it gets more headroom than the texture cache.
/// It is a plain cache: when full it is simply dropped and rebuilt.
const MAX_WIDTH_ENTRIES: usize = 2048;

/// Keys are pre-mixed 64-bit hashes; running them through SipHash again
/// on every lookup would be wasted work.
#[derive(Default)]
struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 << 8) ^ (*b as u64);
        }
    }
    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }
}

type IdMap<V> = HashMap<u64, V, BuildHasherDefault<IdentityHasher>>;

/// Final avalanche (murmur3 fmix64) so the low bits used for bucket
/// selection are well distributed.
#[inline]
fn mix(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^ (h >> 33)
}

/// Hash of a glyph run: (text, size, bold). Colour-independent, so it
/// doubles as the width-index key.
#[inline]
fn hash_run(text: &str, size: u16, bold: bool) -> u64 {
    // FNV-1a over the bytes, then fold in the style bits.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= ((size as u64) << 1) | (bold as u64);
    h = h.wrapping_mul(0x0000_0100_0000_01b3);
    mix(h)
}

/// Texture key: the run hash combined with the colour.
#[inline]
fn hash_key(run: u64, color: (u8, u8, u8, u8)) -> u64 {
    let c = u32::from_be_bytes([color.0, color.1, color.2, color.3]) as u64;
    mix(run ^ c.wrapping_mul(0x9e37_79b9_7f4a_7c15))
}

struct CachedText {
    texture: Texture<'static>,
    width: u32,
    height: u32,
    last_used: u64,
    // Full key kept for collision checks on the 64-bit hash.
    text: String,
    size: u16,
    bold: bool,
    color: (u8, u8, u8, u8),
}

impl CachedText {
    #[inline]
    fn matches(&self, text: &str, size: u16, bold: bool, color: (u8, u8, u8, u8)) -> bool {
        self.size == size && self.bold == bold && self.color == color && self.text == text
    }
}

struct CachedWidth {
    width: u32,
    text: String,
    size: u16,
    bold: bool,
}

/// LRU cache of pre-rendered text textures. Avoids re-rasterizing the
/// same string every frame. On the device, text rendering was the #1
/// CPU hotspot (120+ draw_text calls per frame, each creating a fresh
/// surface and uploading a fresh GPU texture).
///
/// Lookups hash the borrowed `&str` directly -- no `String` is allocated
/// on a hit. A separate colour-independent width index makes `measure()`
/// an O(1) probe instead of a scan over every texture.
pub struct TextCache {
    textures: IdMap<CachedText>,
    widths: IdMap<CachedWidth>,
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
            textures: IdMap::default(),
            widths: IdMap::default(),
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
        self.widths.clear();
    }

    /// Look up or render a text texture and blit it at (x, y).
    /// Returns the rendered width.
    #[allow(clippy::too_many_arguments)]
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

        // When the theme's bold face is the same file as the regular one
        // the glyph runs are pixel-identical: don't cache them twice.
        let bold = bold && !fonts.bold_is_regular();

        self.tick = self.tick.wrapping_add(1);
        let color_key = (color.r, color.g, color.b, color.a);
        let run = hash_run(text, size, bold);
        let key = hash_key(run, color_key);

        if let Some(c) = self.textures.get_mut(&key) {
            if c.matches(text, size, bold, color_key) {
                c.last_used = self.tick;
                let w = c.width;
                let h = c.height;
                self.hits += 1;
                canvas
                    .copy(&c.texture, None, sdl2::rect::Rect::new(x, y, w, h))
                    .ok();
                return w;
            }
            // Hash collision with a different string: fall through and
            // let the new entry replace it.
        }

        // Cache miss -- rasterize.
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
                text: text.to_string(),
                size,
                bold,
                color: color_key,
            },
        );
        self.remember_width(run, text, size, bold, width);

        // LRU eviction if over capacity.
        if self.textures.len() > self.max_entries {
            self.evict_oldest();
        }

        width
    }

    /// Get just the rendered width without drawing (for layout calculations).
    /// Uses the width index if available, otherwise queries the font once
    /// and remembers the answer.
    pub fn measure(&mut self, fonts: &mut FontCache, text: &str, size: u16, bold: bool) -> u32 {
        if text.is_empty() {
            return 0;
        }
        let bold = bold && !fonts.bold_is_regular();
        let run = hash_run(text, size, bold);
        if let Some(w) = self.widths.get(&run) {
            if w.size == size && w.bold == bold && w.text == text {
                return w.width;
            }
        }
        // Fallback to font's size_of (FreeType metric query).
        let style = if bold { FontStyle::MonoBold } else { FontStyle::Mono };
        let font = fonts.get(style, size);
        let width = font.size_of(text).map(|(w, _)| w).unwrap_or(0);
        self.remember_width(run, text, size, bold, width);
        width
    }

    fn remember_width(&mut self, run: u64, text: &str, size: u16, bold: bool, width: u32) {
        if self.widths.contains_key(&run) {
            return;
        }
        if self.widths.len() >= MAX_WIDTH_ENTRIES {
            self.widths.clear();
        }
        self.widths.insert(
            run,
            CachedWidth { width, text: text.to_string(), size, bold },
        );
    }

    fn evict_oldest(&mut self) {
        // Drop ~10% of oldest entries at once to avoid frequent eviction
        // churn. Only (tick, key) pairs are copied -- no String clones.
        let drop_count = (self.max_entries / 10).max(1);
        let mut ages: Vec<(u64, u64)> = self
            .textures
            .iter()
            .map(|(k, v)| (v.last_used, *k))
            .collect();
        if ages.len() > drop_count {
            // Partition so the `drop_count` smallest ticks are at the front.
            ages.select_nth_unstable(drop_count);
        }
        for (_, k) in ages.iter().take(drop_count) {
            self.textures.remove(k);
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
