use sdl2::pixels::Color;

/// Which visual system the launcher screens draw with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiStyle {
    /// The original cyberdeck look: cards, glows, atmosphere overlays.
    Cyberdeck,
    /// Neo-Tokyo: flat black, one red, display type, rules instead of cards.
    Neo,
}

/// Visual style configuration for Cartridge apps.
pub struct Theme {
    /// Preset id (see THEME_PRESETS).
    pub id: &'static str,
    /// Which screen layouts the launcher uses for this theme.
    pub ui: UiStyle,

    // Core palette
    pub bg: Color,
    pub bg_lighter: Color,
    pub bg_selected: Color,
    pub bg_header: Color,

    // Card / panel colors
    pub card_bg: Color,
    pub card_border: Color,
    pub card_highlight: Color,

    // Shadows
    pub shadow: Color,
    pub shadow_offset: i32,

    // Gradient header
    pub header_gradient_top: Color,
    pub header_gradient_bottom: Color,

    // Text
    pub text: Color,
    pub text_dim: Color,
    /// Quieter than text_dim: unfocused numerals, rules-as-text.
    pub text_muted: Color,
    pub text_accent: Color,
    pub text_error: Color,
    pub text_success: Color,
    pub text_warning: Color,

    // Accent & border
    pub accent: Color,
    pub border: Color,

    // Face button colors (for hint badges)
    pub btn_a: Color,
    pub btn_b: Color,
    pub btn_x: Color,
    pub btn_y: Color,
    pub btn_l: Color,
    pub btn_r: Color,

    // Semantic colors
    pub positive: Color,
    pub negative: Color,
    pub orange: Color,

    // Atmosphere / glow colors
    pub glow_primary: Color,
    pub glow_secondary: Color,
    pub corner_marker: Color,
    pub sweep_line: Color,
    pub data_readout: Color,

    // Border radius defaults
    pub border_radius: u16,
    pub border_radius_small: u16,

    // Layout constants
    pub padding: i32,
    pub item_height: i32,
    pub header_height: i32,
    pub footer_height: i32,

    // Font sizes
    pub font_size_normal: u16,
    pub font_size_small: u16,
    pub font_size_large: u16,
    pub font_size_title: u16,

    // Font family filenames (under assets/fonts/). Without extension --
    // FontCache appends ".ttf". Both files must exist or rendering fails.
    pub font_regular: &'static str,
    pub font_bold: &'static str,
    /// Display face for titles, numerals and the clock.
    pub font_display: &'static str,

    // Atmosphere style flags.
    /// If false, the launcher draws no atmosphere at all: no grid, no
    /// scanlines, no vignette, no corner markers, no sweep.
    pub atmosphere: bool,
    /// 0..=255 alpha for the baked CRT scanline overlay. Higher = more
    /// pronounced retro stripes. Free at runtime (baked once).
    pub scanline_strength: u8,
    /// If true, the launcher renders a slow horizontal sweep line.
    /// Gated additionally by the user's animations_enabled setting.
    pub animated_sweep: bool,
}

/// A theme preset's identifier and display name.
pub struct ThemePreset {
    pub id: &'static str,
    pub name: &'static str,
}

/// Built-in theme presets, in display order.
pub const THEME_PRESETS: &[ThemePreset] = &[
    ThemePreset { id: "neo",      name: "Neo-Tokyo" },
    ThemePreset { id: "midnight", name: "Midnight" },
    ThemePreset { id: "amber",    name: "Amber Terminal" },
    ThemePreset { id: "matrix",   name: "Matrix" },
];

/// Default theme id used when no user choice is set.
pub const DEFAULT_THEME_ID: &str = "neo";

/// Which UI style a preset id maps to, without building the palette.
pub fn style_of(id: &str) -> UiStyle {
    match id {
        "neo" => UiStyle::Neo,
        _ => UiStyle::Cyberdeck,
    }
}

impl Theme {
    /// Build a theme by preset id. Falls back to the default preset on an
    /// unknown id.
    pub fn by_id(id: &str) -> Self {
        match id {
            "neo"      => Self::neo(),
            "midnight" => Self::midnight(),
            "amber"    => Self::amber(),
            "matrix"   => Self::matrix(),
            _          => Self::by_id(DEFAULT_THEME_ID),
        }
    }

    /// Build the theme the user picked in the launcher settings, falling
    /// back to Midnight if the file isn't there yet (first run, or a
    /// cartridge running outside the launcher).
    ///
    /// Reads `~/.cartridges/cartridge-launcher/data/settings.json` and
    /// honors the `theme_id` field. Cheap (small file, called once at
    /// cartridge startup).
    pub fn user_selected() -> Self {
        Self::by_id(&user_theme_id().unwrap_or_else(|| DEFAULT_THEME_ID.to_string()))
    }
}

fn user_theme_id() -> Option<String> {
    let path = crate::paths::cartridges_dir()
        .join("cartridge-launcher")
        .join("data")
        .join("settings.json");
    let content = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("theme_id")?.as_str().map(|s| s.to_string())
}

impl Theme {

    /// Neo-Tokyo: near-black, warm white, one red. No atmosphere layer;
    /// the character comes from type and composition alone.
    pub fn neo() -> Self {
        let black = Color::RGB(12, 12, 14);
        let panel = Color::RGB(20, 20, 23);
        let rule = Color::RGB(38, 38, 43);
        let muted = Color::RGB(74, 74, 80);
        let dim = Color::RGB(138, 134, 128);
        let white = Color::RGB(242, 237, 228);
        let red = Color::RGB(226, 61, 44);
        let layout = LayoutDefaults {
            border_radius: 0,
            border_radius_small: 0,
            font_regular: "JetBrainsMono-Regular",
            font_bold: "JetBrainsMono-Bold",
            font_display: "BebasNeue-Regular",
            scanline_strength: 0,
            animated_sweep: false,
            atmosphere: false,
            ..LayoutDefaults::default()
        };
        Self {
            id: "neo",
            ui: UiStyle::Neo,

            bg: black,
            bg_lighter: panel,
            bg_selected: red,
            bg_header: black,

            card_bg: panel,
            card_border: rule,
            card_highlight: red,

            shadow: black,
            shadow_offset: 0,

            header_gradient_top: black,
            header_gradient_bottom: black,

            text: white,
            text_dim: dim,
            text_muted: muted,
            text_accent: red,
            text_error: red,
            text_success: white,
            text_warning: red,

            accent: red,
            border: rule,

            btn_a: white,
            btn_b: red,
            btn_x: red,
            btn_y: white,
            btn_l: dim,
            btn_r: dim,

            positive: white,
            negative: red,
            orange: red,

            glow_primary: Color::RGBA(226, 61, 44, 0),
            glow_secondary: Color::RGBA(226, 61, 44, 0),
            corner_marker: Color::RGBA(0, 0, 0, 0),
            sweep_line: Color::RGBA(0, 0, 0, 0),
            data_readout: rule,

            ..layout.into_theme_skeleton()
        }
    }

    /// Cyberdeck blue on near-black -- the original CartridgeOS look.
    pub fn midnight() -> Self {
        let layout = LayoutDefaults {
            border_radius: 8,
            border_radius_small: 4,
            font_regular: "ShareTechMono-Regular",
            font_bold: "ShareTechMono-Regular",
            font_display: "ShareTechMono-Regular",
            scanline_strength: 24,
            animated_sweep: false,
            ..LayoutDefaults::default()
        };
        Self {
            id: "midnight",
            ui: UiStyle::Cyberdeck,

            bg: Color::RGB(18, 18, 24),
            bg_lighter: Color::RGB(30, 30, 42),
            bg_selected: Color::RGB(40, 50, 80),
            bg_header: Color::RGB(24, 24, 36),

            card_bg: Color::RGB(28, 28, 40),
            card_border: Color::RGB(55, 55, 75),
            card_highlight: Color::RGB(45, 55, 85),

            shadow: Color::RGB(8, 8, 12),
            shadow_offset: 2,

            header_gradient_top: Color::RGB(35, 35, 55),
            header_gradient_bottom: Color::RGB(24, 24, 36),

            text: Color::RGB(220, 220, 230),
            text_dim: Color::RGB(120, 120, 140),
            text_muted: Color::RGB(80, 80, 95),
            text_accent: Color::RGB(100, 180, 255),
            text_error: Color::RGB(255, 100, 100),
            text_success: Color::RGB(100, 220, 100),
            text_warning: Color::RGB(255, 200, 60),

            accent: Color::RGB(100, 180, 255),
            border: Color::RGB(50, 50, 70),

            btn_a: Color::RGB(80, 200, 80),
            btn_b: Color::RGB(220, 80, 80),
            btn_x: Color::RGB(80, 140, 240),
            btn_y: Color::RGB(230, 200, 60),
            btn_l: Color::RGB(140, 140, 160),
            btn_r: Color::RGB(140, 140, 160),

            positive: Color::RGB(80, 210, 120),
            negative: Color::RGB(240, 80, 90),
            orange: Color::RGB(255, 140, 40),

            glow_primary: Color::RGBA(100, 180, 255, 60),
            glow_secondary: Color::RGBA(60, 80, 120, 40),
            corner_marker: Color::RGBA(60, 80, 120, 100),
            sweep_line: Color::RGBA(100, 180, 255, 12),
            data_readout: Color::RGBA(60, 80, 120, 80),

            ..layout.into_theme_skeleton()
        }
    }

    /// Warm amber phosphor on deep brown-black -- old vector terminal vibes.
    pub fn amber() -> Self {
        let layout = LayoutDefaults {
            border_radius: 4,
            border_radius_small: 2,
            font_regular: "CascadiaMono-Bold",
            font_bold: "CascadiaMono-Bold",
            font_display: "CascadiaMono-Bold",
            scanline_strength: 90,
            animated_sweep: true,
            ..LayoutDefaults::default()
        };
        Self {
            id: "amber",
            ui: UiStyle::Cyberdeck,

            bg: Color::RGB(18, 12, 6),
            bg_lighter: Color::RGB(34, 24, 12),
            bg_selected: Color::RGB(80, 50, 14),
            bg_header: Color::RGB(24, 16, 6),

            card_bg: Color::RGB(30, 20, 10),
            card_border: Color::RGB(110, 70, 20),
            card_highlight: Color::RGB(90, 56, 16),

            shadow: Color::RGB(6, 4, 2),
            shadow_offset: 2,

            header_gradient_top: Color::RGB(60, 38, 12),
            header_gradient_bottom: Color::RGB(24, 16, 6),

            text: Color::RGB(255, 196, 96),
            text_dim: Color::RGB(160, 110, 50),
            text_muted: Color::RGB(110, 75, 35),
            text_accent: Color::RGB(255, 220, 120),
            text_error: Color::RGB(255, 110, 70),
            text_success: Color::RGB(220, 220, 90),
            text_warning: Color::RGB(255, 170, 40),

            accent: Color::RGB(255, 180, 60),
            border: Color::RGB(110, 70, 20),

            btn_a: Color::RGB(220, 200, 80),
            btn_b: Color::RGB(220, 90, 50),
            btn_x: Color::RGB(255, 200, 80),
            btn_y: Color::RGB(255, 230, 110),
            btn_l: Color::RGB(140, 110, 60),
            btn_r: Color::RGB(140, 110, 60),

            positive: Color::RGB(220, 200, 80),
            negative: Color::RGB(240, 100, 60),
            orange: Color::RGB(255, 150, 50),

            glow_primary: Color::RGBA(255, 180, 60, 70),
            glow_secondary: Color::RGBA(140, 90, 30, 50),
            corner_marker: Color::RGBA(160, 100, 30, 110),
            sweep_line: Color::RGBA(255, 180, 60, 14),
            data_readout: Color::RGBA(140, 90, 30, 90),

            ..layout.into_theme_skeleton()
        }
    }

    /// Bright green phosphor on pitch black -- classic Matrix terminal.
    pub fn matrix() -> Self {
        let layout = LayoutDefaults {
            border_radius: 0,
            border_radius_small: 0,
            font_regular: "CascadiaMono-Regular",
            font_bold: "CascadiaMono-Bold",
            font_display: "CascadiaMono-Bold",
            scanline_strength: 130,
            animated_sweep: true,
            ..LayoutDefaults::default()
        };
        Self {
            id: "matrix",
            ui: UiStyle::Cyberdeck,

            bg: Color::RGB(2, 8, 4),
            bg_lighter: Color::RGB(8, 22, 12),
            bg_selected: Color::RGB(14, 50, 22),
            bg_header: Color::RGB(4, 14, 6),

            card_bg: Color::RGB(8, 22, 12),
            card_border: Color::RGB(40, 110, 50),
            card_highlight: Color::RGB(20, 60, 26),

            shadow: Color::RGB(0, 4, 0),
            shadow_offset: 2,

            header_gradient_top: Color::RGB(14, 38, 18),
            header_gradient_bottom: Color::RGB(4, 14, 6),

            text: Color::RGB(140, 240, 150),
            text_dim: Color::RGB(70, 140, 80),
            text_muted: Color::RGB(40, 90, 50),
            text_accent: Color::RGB(120, 255, 140),
            text_error: Color::RGB(255, 90, 90),
            text_success: Color::RGB(120, 255, 140),
            text_warning: Color::RGB(220, 240, 90),

            accent: Color::RGB(70, 230, 100),
            border: Color::RGB(36, 90, 46),

            btn_a: Color::RGB(120, 255, 140),
            btn_b: Color::RGB(220, 80, 80),
            btn_x: Color::RGB(80, 220, 180),
            btn_y: Color::RGB(220, 240, 90),
            btn_l: Color::RGB(110, 160, 110),
            btn_r: Color::RGB(110, 160, 110),

            positive: Color::RGB(120, 255, 140),
            negative: Color::RGB(240, 90, 90),
            orange: Color::RGB(220, 200, 60),

            glow_primary: Color::RGBA(70, 230, 100, 70),
            glow_secondary: Color::RGBA(40, 110, 60, 50),
            corner_marker: Color::RGBA(40, 130, 60, 110),
            sweep_line: Color::RGBA(70, 230, 100, 16),
            data_readout: Color::RGBA(40, 110, 60, 90),

            ..layout.into_theme_skeleton()
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::by_id(DEFAULT_THEME_ID)
    }
}

/// Layout / typography defaults shared by every preset. Kept separate so
/// presets only need to set palette colors.
struct LayoutDefaults {
    border_radius: u16,
    border_radius_small: u16,
    padding: i32,
    item_height: i32,
    header_height: i32,
    footer_height: i32,
    font_size_normal: u16,
    font_size_small: u16,
    font_size_large: u16,
    font_size_title: u16,
    font_regular: &'static str,
    font_bold: &'static str,
    font_display: &'static str,
    scanline_strength: u8,
    animated_sweep: bool,
    atmosphere: bool,
}

impl Default for LayoutDefaults {
    fn default() -> Self {
        Self {
            border_radius: 8,
            border_radius_small: 4,
            padding: 10,
            item_height: 36,
            header_height: 40,
            footer_height: 36,
            font_size_normal: 16,
            font_size_small: 13,
            font_size_large: 20,
            font_size_title: 24,
            font_regular: "ShareTechMono-Regular",
            font_bold: "ShareTechMono-Regular",
            font_display: "ShareTechMono-Regular",
            scanline_strength: 24,
            animated_sweep: false,
            atmosphere: true,
        }
    }
}

impl LayoutDefaults {
    /// Produce a Theme with all palette colors zero -- used with struct
    /// update syntax (`..layout.into_theme_skeleton()`) so each preset only
    /// has to set palette fields. The black palette is overwritten before
    /// the value is observed.
    fn into_theme_skeleton(self) -> Theme {
        let z = Color::RGB(0, 0, 0);
        Theme {
            id: DEFAULT_THEME_ID,
            ui: UiStyle::Cyberdeck,
            bg: z, bg_lighter: z, bg_selected: z, bg_header: z,
            card_bg: z, card_border: z, card_highlight: z,
            shadow: z, shadow_offset: 0,
            header_gradient_top: z, header_gradient_bottom: z,
            text: z, text_dim: z, text_muted: z, text_accent: z,
            text_error: z, text_success: z, text_warning: z,
            accent: z, border: z,
            btn_a: z, btn_b: z, btn_x: z, btn_y: z, btn_l: z, btn_r: z,
            positive: z, negative: z, orange: z,
            glow_primary: z, glow_secondary: z,
            corner_marker: z, sweep_line: z, data_readout: z,
            border_radius: self.border_radius,
            border_radius_small: self.border_radius_small,
            padding: self.padding,
            item_height: self.item_height,
            header_height: self.header_height,
            footer_height: self.footer_height,
            font_size_normal: self.font_size_normal,
            font_size_small: self.font_size_small,
            font_size_large: self.font_size_large,
            font_size_title: self.font_size_title,
            font_regular: self.font_regular,
            font_bold: self.font_bold,
            font_display: self.font_display,
            scanline_strength: self.scanline_strength,
            animated_sweep: self.animated_sweep,
            atmosphere: self.atmosphere,
        }
    }
}
