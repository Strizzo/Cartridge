"""Theme dataclass for Cartridge apps."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass
class Theme:
    """Visual style configuration. Apps can override colors."""

    name: str = "default"

    # Core palette
    bg: tuple = (18, 18, 24)
    bg_lighter: tuple = (30, 30, 42)
    bg_selected: tuple = (40, 50, 80)
    bg_header: tuple = (24, 24, 36)

    # Card / panel colors
    card_bg: tuple = (28, 28, 40)
    card_border: tuple = (55, 55, 75)
    card_highlight: tuple = (45, 55, 85)

    # Shadows
    shadow: tuple = (8, 8, 12)
    shadow_offset: int = 2

    # Gradient header
    header_gradient_top: tuple = (35, 35, 55)
    header_gradient_bottom: tuple = (24, 24, 36)

    text: tuple = (220, 220, 230)
    text_dim: tuple = (120, 120, 140)
    text_accent: tuple = (100, 180, 255)
    text_error: tuple = (255, 100, 100)
    text_success: tuple = (100, 220, 100)
    text_warning: tuple = (255, 200, 60)

    accent: tuple = (100, 180, 255)
    border: tuple = (50, 50, 70)

    # Face button colors (for hint badges)
    btn_a: tuple = (80, 200, 80)       # green  - confirm
    btn_b: tuple = (220, 80, 80)       # red    - back
    btn_x: tuple = (80, 140, 240)      # blue   - action
    btn_y: tuple = (230, 200, 60)      # yellow - alt action
    btn_l: tuple = (140, 140, 160)     # shoulder buttons
    btn_r: tuple = (140, 140, 160)

    # Semantic colors
    positive: tuple = (80, 210, 120)
    negative: tuple = (240, 80, 90)
    orange: tuple = (255, 140, 40)

    # Border radius defaults
    border_radius: int = 8
    border_radius_small: int = 4

    # Layout constants
    padding: int = 10
    item_height: int = 36
    header_height: int = 40
    footer_height: int = 36

    # Font sizes
    font_size_normal: int = 16
    font_size_small: int = 13
    font_size_large: int = 20
    font_size_title: int = 24
