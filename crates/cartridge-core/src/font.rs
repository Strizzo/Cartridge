use sdl2::pixels::Color;
use sdl2::render::{Canvas, Texture, TextureCreator, TextureQuery};
use sdl2::ttf::{Font, Sdl2TtfContext};
use sdl2::video::{Window, WindowContext};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Font style variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Mono,
    MonoBold,
    /// Display face for titles, numerals and the clock. Themes that have
    /// no dedicated display face map this to their regular font.
    Display,
}

/// Manages font loading and caching.
pub struct FontCache {
    ttf_context: Sdl2TtfContext,
    fonts: HashMap<(FontStyle, u16), Font<'static, 'static>>,
    assets_dir: PathBuf,
    regular_path: PathBuf,
    bold_path: PathBuf,
    display_path: PathBuf,
    display_cache: DisplayCache,
}

const DEFAULT_FAMILY: &str = "ShareTechMono-Regular";

/// Display strings on a screen are few (title, clock, a dozen numerals),
/// so the cache is bounded by a flush rather than an LRU.
const DISPLAY_CACHE_MAX: usize = 256;

impl FontCache {
    pub fn new(assets_dir: &Path) -> Result<Self, String> {
        let ttf_context = sdl2::ttf::init().map_err(|e| e.to_string())?;
        let regular_path = font_path(assets_dir, DEFAULT_FAMILY);
        let bold_path = font_path(assets_dir, DEFAULT_FAMILY);
        let display_path = font_path(assets_dir, DEFAULT_FAMILY);

        if !regular_path.exists() {
            return Err(format!("Font not found: {}", regular_path.display()));
        }

        Ok(Self {
            ttf_context,
            fonts: HashMap::new(),
            assets_dir: assets_dir.to_path_buf(),
            regular_path,
            bold_path,
            display_path,
            display_cache: DisplayCache::default(),
        })
    }

    /// Swap to a different font family. Filenames are without ".ttf"
    /// extension and live under `assets/fonts/`. Falls back to the default
    /// family for any missing file. Flushes loaded glyphs so subsequent
    /// `get()` calls reload from the new files.
    pub fn set_family(&mut self, regular: &str, bold: &str) {
        let regular_path = self.resolve(regular);
        let bold_path = self.resolve(bold);
        let unchanged = regular_path == self.regular_path && bold_path == self.bold_path;
        self.regular_path = regular_path;
        self.bold_path = bold_path;
        if !unchanged {
            // Drop cached glyphs so the new family takes effect.
            self.fonts.clear();
        }
    }

    /// Swap the display face (titles, numerals). Same fallback rules as
    /// `set_family`; flushes the display text cache when it changes.
    pub fn set_display(&mut self, display: &str) {
        let display_path = self.resolve(display);
        if display_path != self.display_path {
            self.display_path = display_path;
            self.fonts.retain(|(style, _), _| *style != FontStyle::Display);
            self.display_cache.clear();
        }
    }

    fn resolve(&self, family: &str) -> PathBuf {
        let candidate = font_path(&self.assets_dir, family);
        if candidate.exists() {
            candidate
        } else {
            font_path(&self.assets_dir, DEFAULT_FAMILY)
        }
    }

    /// Get a font with the given style and size. Loads and caches on first use.
    pub fn get(&mut self, style: FontStyle, size: u16) -> &Font<'static, 'static> {
        let key = (style, size);
        if !self.fonts.contains_key(&key) {
            let path = match style {
                FontStyle::Mono => &self.regular_path,
                FontStyle::MonoBold => &self.bold_path,
                FontStyle::Display => &self.display_path,
            };
            // SAFETY: We extend the lifetime because FontCache owns the TtfContext
            // and fonts are only accessed through &self, ensuring the context outlives
            // all fonts. The fonts HashMap is dropped before ttf_context.
            let font: Font<'static, 'static> = unsafe {
                std::mem::transmute(
                    self.ttf_context
                        .load_font(path, size)
                        .unwrap_or_else(|e| panic!("Failed to load font {}: {}", path.display(), e)),
                )
            };
            self.fonts.insert(key, font);
        }
        &self.fonts[&key]
    }

    /// Pre-warm cache with common sizes.
    pub fn prewarm(&mut self) {
        for size in [11, 13, 14, 16, 20, 24] {
            self.get(FontStyle::Mono, size);
            self.get(FontStyle::MonoBold, size);
        }
    }

    /// Draw a string in the display face through the display text cache.
    /// Returns the rendered width.
    pub fn draw_display(
        &mut self,
        canvas: &mut Canvas<Window>,
        creator: &TextureCreator<WindowContext>,
        text: &str,
        x: i32,
        y: i32,
        color: Color,
        size: u16,
    ) -> u32 {
        if text.is_empty() {
            return 0;
        }
        let key = (text.to_string(), size, (color.r, color.g, color.b, color.a));
        if let Some(entry) = self.display_cache.entries.get(&key) {
            canvas
                .copy(&entry.texture, None, sdl2::rect::Rect::new(x, y, entry.width, entry.height))
                .ok();
            return entry.width;
        }

        let font = self.get(FontStyle::Display, size);
        let surface = match font.render(text).blended(color) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let texture = match creator.create_texture_from_surface(&surface) {
            Ok(t) => t,
            Err(_) => return 0,
        };
        // SAFETY: same lifetime extension as TextCache -- the creator outlives
        // the FontCache in every run loop (declared before it, dropped after).
        let texture: Texture<'static> = unsafe { std::mem::transmute(texture) };
        let TextureQuery { width, height, .. } = texture.query();
        canvas
            .copy(&texture, None, sdl2::rect::Rect::new(x, y, width, height))
            .ok();

        if self.display_cache.entries.len() >= DISPLAY_CACHE_MAX {
            self.display_cache.clear();
        }
        self.display_cache
            .entries
            .insert(key, DisplayEntry { texture, width, height });
        width
    }

    /// Width of a string in the display face, using the cache when the
    /// string was drawn before in any color.
    pub fn display_width(&mut self, text: &str, size: u16) -> u32 {
        if text.is_empty() {
            return 0;
        }
        if let Some(w) = self
            .display_cache
            .entries
            .iter()
            .find(|((t, s, _), _)| t == text && *s == size)
            .map(|(_, e)| e.width)
        {
            return w;
        }
        self.get(FontStyle::Display, size)
            .size_of(text)
            .map(|(w, _)| w)
            .unwrap_or(0)
    }
}

#[derive(Default)]
struct DisplayCache {
    entries: HashMap<(String, u16, (u8, u8, u8, u8)), DisplayEntry>,
}

impl DisplayCache {
    fn clear(&mut self) {
        self.entries.clear();
    }
}

struct DisplayEntry {
    texture: Texture<'static>,
    width: u32,
    height: u32,
}

fn font_path(assets_dir: &Path, family: &str) -> PathBuf {
    assets_dir.join("fonts").join(format!("{family}.ttf"))
}
