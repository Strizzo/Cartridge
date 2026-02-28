"""Screen abstraction wrapping pygame.Surface with font cache."""

from __future__ import annotations

import os
from typing import TYPE_CHECKING

import pygame

if TYPE_CHECKING:
    from cartridge_sdk.theme import Theme

# Font cache
_fonts: dict[tuple[str, int], pygame.font.Font] = {}

_PREFERRED_FONTS = [
    "Menlo",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "monospace",
]


def _get_font(style: str, size: int) -> pygame.font.Font:
    key = (style, size)
    if key in _fonts:
        return _fonts[key]

    bold = style.endswith("_bold")

    for name in _PREFERRED_FONTS:
        path = pygame.font.match_font(name, bold=bold)
        if path and os.path.isfile(path):
            font = pygame.font.Font(path, size)
            _fonts[key] = font
            return font

    font = pygame.font.SysFont("monospace", size, bold=bold)
    _fonts[key] = font
    return font


def init_fonts() -> None:
    """Pre-warm the font cache. Call after pygame.init()."""
    for size in (13, 14, 16, 20, 24):
        _get_font("mono", size)
        _get_font("mono_bold", size)


class Screen:
    """High-level drawing surface for CartridgeApp.on_render()."""

    WIDTH = 640
    HEIGHT = 480

    def __init__(self, surface: pygame.Surface, theme: Theme) -> None:
        self._surface = surface
        self.theme = theme
        self.width = self.WIDTH
        self.height = self.HEIGHT

    @property
    def surface(self) -> pygame.Surface:
        return self._surface

    def clear(self, color: tuple | None = None) -> None:
        self._surface.fill(color or self.theme.bg)

    def draw_text(
        self,
        text: str,
        x: int,
        y: int,
        color: tuple | None = None,
        font_size: int = 16,
        bold: bool = False,
        max_width: int | None = None,
    ) -> int:
        """Render text. Returns rendered width."""
        style = "mono_bold" if bold else "mono"
        font = _get_font(style, font_size)
        color = color or self.theme.text

        display_text = text
        if max_width is not None:
            original_len = len(display_text)
            while len(display_text) > 0 and font.size(display_text + "..")[0] > max_width:
                display_text = display_text[:-1]
            if len(display_text) < original_len:
                display_text += ".."

        rendered = font.render(display_text, True, color)
        self._surface.blit(rendered, (x, y))
        return rendered.get_width()

    def draw_rect(
        self,
        rect: tuple | pygame.Rect,
        color: tuple | None = None,
        filled: bool = True,
        border_radius: int = 0,
        width: int = 0,
    ) -> None:
        color = color or self.theme.border
        if filled and width == 0:
            pygame.draw.rect(self._surface, color, rect, border_radius=border_radius)
        else:
            pygame.draw.rect(self._surface, color, rect, width or 1, border_radius=border_radius)

    def draw_line(
        self,
        start: tuple,
        end: tuple,
        color: tuple | None = None,
        width: int = 1,
    ) -> None:
        pygame.draw.line(self._surface, color or self.theme.border, start, end, width)

    def draw_image(
        self,
        image: pygame.Surface,
        x: int,
        y: int,
        w: int | None = None,
        h: int | None = None,
    ) -> None:
        if w is not None and h is not None:
            image = pygame.transform.scale(image, (w, h))
        self._surface.blit(image, (x, y))

    def get_text_width(self, text: str, font_size: int = 16, bold: bool = False) -> int:
        style = "mono_bold" if bold else "mono"
        return _get_font(style, font_size).size(text)[0]

    def get_line_height(self, font_size: int = 16, bold: bool = False) -> int:
        style = "mono_bold" if bold else "mono"
        return _get_font(style, font_size).get_linesize()

    # --- New drawing primitives ---

    def draw_rounded_rect(
        self,
        rect: tuple | pygame.Rect,
        color: tuple,
        radius: int = 8,
        shadow: bool = False,
    ) -> None:
        """Draw a rounded rectangle with optional shadow."""
        r = pygame.Rect(rect)
        if shadow:
            off = self.theme.shadow_offset
            shadow_rect = r.move(off, off)
            pygame.draw.rect(self._surface, self.theme.shadow, shadow_rect, border_radius=radius)
        pygame.draw.rect(self._surface, color, r, border_radius=radius)

    def draw_card(
        self,
        rect: tuple | pygame.Rect,
        bg: tuple | None = None,
        border: tuple | None = None,
        radius: int = 8,
        shadow: bool = True,
    ) -> None:
        """Draw a card panel with bg, border, and optional shadow."""
        r = pygame.Rect(rect)
        bg = bg or self.theme.card_bg
        border = border or self.theme.card_border

        if shadow:
            off = self.theme.shadow_offset
            shadow_rect = r.move(off, off)
            pygame.draw.rect(self._surface, self.theme.shadow, shadow_rect, border_radius=radius)

        pygame.draw.rect(self._surface, bg, r, border_radius=radius)
        pygame.draw.rect(self._surface, border, r, 1, border_radius=radius)

    def draw_gradient_rect(
        self,
        rect: tuple | pygame.Rect,
        color_top: tuple,
        color_bottom: tuple,
    ) -> None:
        """Vertical gradient fill between two colors."""
        r = pygame.Rect(rect)
        if r.height <= 0:
            return
        for y in range(r.height):
            t = y / max(1, r.height - 1)
            c = (
                int(color_top[0] + (color_bottom[0] - color_top[0]) * t),
                int(color_top[1] + (color_bottom[1] - color_top[1]) * t),
                int(color_top[2] + (color_bottom[2] - color_top[2]) * t),
            )
            pygame.draw.line(self._surface, c, (r.x, r.y + y), (r.x + r.width - 1, r.y + y))

    def draw_sparkline(
        self,
        data: list[float],
        rect: tuple | pygame.Rect,
        color: tuple | None = None,
        baseline_color: tuple | None = None,
    ) -> None:
        """Draw a mini line chart from data points within rect."""
        r = pygame.Rect(rect)
        if len(data) < 2 or r.width < 4 or r.height < 4:
            return

        color = color or self.theme.accent
        mn = min(data)
        mx = max(data)
        rng = mx - mn if mx != mn else 1.0

        points = []
        for i, v in enumerate(data):
            px = r.x + int(i / (len(data) - 1) * (r.width - 1))
            py = r.y + r.height - 1 - int(((v - mn) / rng) * (r.height - 1))
            points.append((px, py))

        if baseline_color:
            # Draw a faint midline
            mid_y = r.y + r.height // 2
            pygame.draw.line(self._surface, baseline_color, (r.x, mid_y), (r.x + r.width - 1, mid_y))

        if len(points) >= 2:
            pygame.draw.lines(self._surface, color, False, points, 2)

    def draw_progress_bar(
        self,
        rect: tuple | pygame.Rect,
        progress: float,
        fill_color: tuple | None = None,
        bg_color: tuple | None = None,
        radius: int = 3,
    ) -> None:
        """Horizontal progress bar. progress is 0.0 to 1.0."""
        r = pygame.Rect(rect)
        bg_color = bg_color or self.theme.bg_lighter
        fill_color = fill_color or self.theme.accent

        pygame.draw.rect(self._surface, bg_color, r, border_radius=radius)
        fill_w = max(0, int(r.width * min(1.0, max(0.0, progress))))
        if fill_w > 0:
            fill_rect = pygame.Rect(r.x, r.y, fill_w, r.height)
            pygame.draw.rect(self._surface, fill_color, fill_rect, border_radius=radius)

    def draw_button_hint(
        self,
        label: str,
        action: str,
        x: int,
        y: int,
        btn_color: tuple | None = None,
        font_size: int = 12,
    ) -> int:
        """Draw a styled button hint like [A] Open. Returns total width."""
        btn_color = btn_color or self.theme.accent
        font = _get_font("mono_bold", font_size)
        font_action = _get_font("mono", font_size)

        # Button badge
        label_surf = font.render(label, True, (20, 20, 30))
        badge_w = label_surf.get_width() + 10
        badge_h = label_surf.get_height() + 4
        badge_rect = pygame.Rect(x, y, badge_w, badge_h)
        pygame.draw.rect(self._surface, btn_color, badge_rect, border_radius=4)
        self._surface.blit(label_surf, (x + 5, y + 2))

        # Action text
        action_surf = font_action.render(action, True, self.theme.text_dim)
        self._surface.blit(action_surf, (x + badge_w + 5, y + 2))

        return badge_w + 5 + action_surf.get_width()

    def draw_pill(
        self,
        text: str,
        x: int,
        y: int,
        bg_color: tuple,
        text_color: tuple = (255, 255, 255),
        font_size: int = 11,
    ) -> int:
        """Draw a rounded pill/badge with text. Returns total width."""
        font = _get_font("mono_bold", font_size)
        text_surf = font.render(text, True, text_color)
        pill_w = text_surf.get_width() + 12
        pill_h = text_surf.get_height() + 4
        pill_rect = pygame.Rect(x, y, pill_w, pill_h)
        pygame.draw.rect(self._surface, bg_color, pill_rect, border_radius=pill_h // 2)
        self._surface.blit(text_surf, (x + 6, y + 2))
        return pill_w
