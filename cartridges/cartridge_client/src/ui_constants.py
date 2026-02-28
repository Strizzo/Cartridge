"""UI constants for the Cartridge client visual design."""

# Category accent colors
CATEGORY_COLORS: dict[str, tuple[int, int, int]] = {
    "news": (74, 158, 255),         # blue
    "finance": (74, 222, 128),      # green
    "tools": (167, 139, 250),       # purple
    "productivity": (251, 191, 36), # yellow
    "games": (239, 68, 68),         # red
    "social": (249, 146, 60),       # orange
    "media": (236, 72, 153),        # pink
}

DEFAULT_CATEGORY_COLOR: tuple[int, int, int] = (140, 140, 160)

# Status pill colors
STATUS_INSTALLED: tuple[int, int, int] = (74, 222, 128)    # green
STATUS_UPDATE: tuple[int, int, int] = (251, 191, 36)       # amber

# Card dimensions
STORE_ROW_HEIGHT = 80
INSTALLED_ROW_HEIGHT = 64
CARD_RADIUS = 8
CARD_MARGIN_X = 8
CARD_WIDTH = 624
CARD_CONTENT_PAD = 14
