use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::render::{Texture, TextureCreator};
use sdl2::surface::Surface;
use sdl2::video::WindowContext;

/// Every cartridge draws a header gradient each frame; a handful of
/// distinct (size, colours) combinations covers all of them.
const MAX_ENTRIES: usize = 16;

struct Entry {
    w: u32,
    h: u32,
    top: (u8, u8, u8),
    bottom: (u8, u8, u8),
    texture: Texture<'static>,
    last_used: u64,
}

/// Small LRU of pre-rendered vertical gradient textures keyed by
/// (w, h, top, bottom). Replaces drawing one line per scanline every frame.
pub struct GradientCache {
    entries: Vec<Entry>,
    creator_ptr: *const TextureCreator<WindowContext>,
    tick: u64,
}

impl GradientCache {
    pub fn new(texture_creator: &TextureCreator<WindowContext>) -> Self {
        Self {
            entries: Vec::with_capacity(MAX_ENTRIES),
            creator_ptr: texture_creator as *const TextureCreator<WindowContext>,
            tick: 0,
        }
    }

    /// Fetch (or build) the texture for a gradient. Returns `None` if the
    /// texture could not be created -- callers fall back to scanlines.
    pub fn get(&mut self, w: u32, h: u32, top: Color, bottom: Color) -> Option<&Texture<'static>> {
        if w == 0 || h == 0 {
            return None;
        }
        self.tick = self.tick.wrapping_add(1);
        let top_k = (top.r, top.g, top.b);
        let bottom_k = (bottom.r, bottom.g, bottom.b);

        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.w == w && e.h == h && e.top == top_k && e.bottom == bottom_k)
        {
            self.entries[i].last_used = self.tick;
            return Some(&self.entries[i].texture);
        }

        let texture = self.build(w, h, top, bottom)?;
        if self.entries.len() >= MAX_ENTRIES {
            let oldest = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.last_used)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.entries.swap_remove(oldest);
        }
        self.entries.push(Entry {
            w,
            h,
            top: top_k,
            bottom: bottom_k,
            texture,
            last_used: self.tick,
        });
        self.entries.last().map(|e| &e.texture)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn build(&self, w: u32, h: u32, top: Color, bottom: Color) -> Option<Texture<'static>> {
        let mut surface = Surface::new(w, h, PixelFormatEnum::RGB24).ok()?;
        let pitch = surface.pitch() as usize;
        surface.with_lock_mut(|px| {
            for y in 0..h as usize {
                // Same interpolation as the scanline fallback so a cache
                // miss and a cache hit are pixel-identical.
                let t = y as f32 / (h as i32 - 1).max(1) as f32;
                let r = (top.r as f32 + (bottom.r as f32 - top.r as f32) * t) as u8;
                let g = (top.g as f32 + (bottom.g as f32 - top.g as f32) * t) as u8;
                let b = (top.b as f32 + (bottom.b as f32 - top.b as f32) * t) as u8;
                let row = &mut px[y * pitch..y * pitch + (w as usize) * 3];
                for p in row.chunks_exact_mut(3) {
                    p[0] = r;
                    p[1] = g;
                    p[2] = b;
                }
            }
        });

        // SAFETY: GradientCache holds a pointer to a TextureCreator that
        // outlives it (same pattern as TextCache / ImageCache). The entries
        // are dropped before the creator goes away.
        let creator = unsafe { &*self.creator_ptr };
        let texture = creator.create_texture_from_surface(&surface).ok()?;
        let texture: Texture<'static> = unsafe { std::mem::transmute(texture) };
        Some(texture)
    }
}
