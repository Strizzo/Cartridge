use sdl2::image::{InitFlag, LoadTexture, Sdl2ImageContext};
use sdl2::render::{Texture, TextureCreator};
use sdl2::video::WindowContext;
use std::collections::HashMap;

/// Manages image loading and caching, mirroring the FontCache pattern.
pub struct ImageCache {
    _ctx: Sdl2ImageContext,
    textures: HashMap<String, Texture<'static>>,
    creator_ptr: *const TextureCreator<WindowContext>,
}

impl ImageCache {
    pub fn new(texture_creator: &TextureCreator<WindowContext>) -> Result<Self, String> {
        let ctx = sdl2::image::init(InitFlag::PNG | InitFlag::JPG)
            .map_err(|e| format!("Failed to init SDL2_image: {e}"))?;

        Ok(Self {
            _ctx: ctx,
            textures: HashMap::new(),
            creator_ptr: texture_creator as *const TextureCreator<WindowContext>,
        })
    }

    /// Get a cached texture by file path. Lazy-loads from disk on first access.
    /// Returns None if the file cannot be loaded.
    pub fn get(&mut self, path: &str) -> Option<&Texture<'static>> {
        if !self.textures.contains_key(path) {
            // SAFETY: ImageCache is always created with a reference to a TextureCreator
            // that outlives it (owned by the main loop). The textures HashMap is dropped
            // before the TextureCreator goes away.
            let creator = unsafe { &*self.creator_ptr };
            match creator.load_texture(path) {
                Ok(texture) => {
                    let texture: Texture<'static> = unsafe { std::mem::transmute(texture) };
                    self.textures.insert(path.to_string(), texture);
                }
                Err(e) => {
                    log::warn!("Failed to load image '{}': {}", path, e);
                    return None;
                }
            }
        }
        self.textures.get(path)
    }

    /// Same as `get()` but returns a mutable reference. Used when the
    /// caller needs to set per-blit modulators (alpha_mod, color_mod).
    pub fn get_mut(&mut self, path: &str) -> Option<&mut Texture<'static>> {
        // Reuse the lazy-load path in get(), then re-borrow mutably.
        self.get(path)?;
        self.textures.get_mut(path)
    }

    /// Check if an image exists at the given path without loading it.
    pub fn exists(path: &str) -> bool {
        std::path::Path::new(path).exists()
    }
}
