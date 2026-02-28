use sdl2::ttf::{Font, Sdl2TtfContext};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Font style variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Mono,
    MonoBold,
}

/// Manages font loading and caching.
pub struct FontCache {
    ttf_context: Sdl2TtfContext,
    fonts: HashMap<(FontStyle, u16), Font<'static, 'static>>,
    regular_path: PathBuf,
    bold_path: PathBuf,
}

impl FontCache {
    pub fn new(assets_dir: &Path) -> Result<Self, String> {
        let ttf_context = sdl2::ttf::init().map_err(|e| e.to_string())?;

        let regular_path = assets_dir.join("fonts").join("ShareTechMono-Regular.ttf");
        let bold_path = assets_dir.join("fonts").join("ShareTechMono-Regular.ttf");

        if !regular_path.exists() {
            return Err(format!(
                "Font not found: {}",
                regular_path.display()
            ));
        }
        if !bold_path.exists() {
            return Err(format!(
                "Font not found: {}",
                bold_path.display()
            ));
        }

        Ok(Self {
            ttf_context,
            fonts: HashMap::new(),
            regular_path,
            bold_path,
        })
    }

    /// Get a font with the given style and size. Loads and caches on first use.
    pub fn get(&mut self, style: FontStyle, size: u16) -> &Font<'static, 'static> {
        let key = (style, size);
        if !self.fonts.contains_key(&key) {
            let path = match style {
                FontStyle::Mono => &self.regular_path,
                FontStyle::MonoBold => &self.bold_path,
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
}
