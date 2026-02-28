use sdl2::pixels::Color;

/// Visual style configuration for Cartridge apps.
pub struct Theme {
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
}

impl Default for Theme {
    fn default() -> Self {
        Self {
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
        }
    }
}
