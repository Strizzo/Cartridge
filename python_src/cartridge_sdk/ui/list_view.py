"""Scrollable list with cursor selection and card-style items."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui.widget import Widget

if TYPE_CHECKING:
    from cartridge_sdk.screen import Screen
    from cartridge_sdk.theme import Theme

PAD_X = 10
PAD_Y = 6
CARD_MARGIN = 3
CARD_RADIUS = 6


@dataclass
class ListItem:
    """An item in a ListView."""

    id: str
    primary_text: str
    secondary_text: str = ""
    right_text: str = ""
    metadata: Any = None


class ListView(Widget):
    """Scrollable list with card-style items and selection callback."""

    def __init__(
        self,
        items: list[ListItem] | None = None,
        on_select: Callable[[ListItem], None] | None = None,
        item_height: int = 48,
        render_item: Callable | None = None,
    ) -> None:
        self.items: list[ListItem] = items or []
        self.cursor: int = 0
        self.on_select = on_select
        self.item_height = item_height
        self.render_item = render_item

    def handle_input(self, event: InputEvent) -> bool:
        if not self.items:
            return False
        if event.action not in ("press", "repeat"):
            return False

        if event.button == Button.DPAD_UP:
            self.cursor = max(0, self.cursor - 1)
            return True
        if event.button == Button.DPAD_DOWN:
            self.cursor = min(len(self.items) - 1, self.cursor + 1)
            return True
        if event.button == Button.L2:
            visible = max(1, self._last_visible_count)
            self.cursor = max(0, self.cursor - visible)
            return True
        if event.button == Button.R2:
            visible = max(1, self._last_visible_count)
            self.cursor = min(len(self.items) - 1, self.cursor + visible)
            return True
        if event.button == Button.A and self.on_select:
            self.on_select(self.items[self.cursor])
            return True

        return False

    _last_visible_count: int = 10

    def draw(self, surface: pygame.Surface, rect: pygame.Rect, theme: Theme) -> None:
        n = len(self.items)
        visible_count = max(1, rect.height // self.item_height)
        self._last_visible_count = visible_count

        # Background
        pygame.draw.rect(surface, theme.bg, rect)

        if n == 0:
            font = _get_font("mono", 14)
            msg = font.render("No items", True, theme.text_dim)
            mx = rect.x + (rect.width - msg.get_width()) // 2
            my = rect.y + rect.height // 2 - msg.get_height() // 2
            surface.blit(msg, (mx, my))
            return

        # Clamp cursor
        self.cursor = max(0, min(self.cursor, n - 1))

        # Sliding window
        window_start = _window_start(self.cursor, n, visible_count)
        window_end = min(window_start + visible_count, n)

        clip = surface.get_clip()
        surface.set_clip(rect)

        y = rect.y
        for i in range(window_start, window_end):
            item = self.items[i]
            is_selected = i == self.cursor

            if self.render_item:
                self.render_item(surface, rect, item, is_selected, y, self.item_height, theme)
            else:
                self._draw_default_item(surface, rect, item, is_selected, y, theme)

            y += self.item_height

        surface.set_clip(clip)

        # Scroll indicator
        if n > visible_count:
            _draw_scroll_indicator(surface, rect, self.cursor, n, theme)

    def _draw_default_item(
        self,
        surface: pygame.Surface,
        rect: pygame.Rect,
        item: ListItem,
        is_selected: bool,
        y: int,
        theme: Theme,
    ) -> None:
        font_primary = _get_font("mono", 15)
        font_secondary = _get_font("mono", 12)
        font_right = _get_font("mono", 13)

        card_x = rect.x + CARD_MARGIN
        card_w = rect.width - CARD_MARGIN * 2 - 8
        card_h = self.item_height - CARD_MARGIN

        # Card background
        card_rect = pygame.Rect(card_x, y + 1, card_w, card_h)
        if is_selected:
            pygame.draw.rect(surface, theme.card_highlight, card_rect, border_radius=CARD_RADIUS)
            pygame.draw.rect(surface, theme.accent, card_rect, 1, border_radius=CARD_RADIUS)
        else:
            pygame.draw.rect(surface, theme.card_bg, card_rect, border_radius=CARD_RADIUS)

        max_text_w = card_w - PAD_X * 2 - 70

        # Primary text
        text_x = card_x + PAD_X + 4
        primary = item.primary_text
        pw = font_primary.size(primary)[0]
        if pw > max_text_w:
            while len(primary) > 1 and font_primary.size(primary + "..")[0] > max_text_w:
                primary = primary[:-1]
            primary += ".."

        primary_color = theme.text if is_selected else theme.text_dim
        rendered = font_primary.render(primary, True, primary_color)
        text_y = y + 5 if item.secondary_text else y + (card_h - rendered.get_height()) // 2
        surface.blit(rendered, (text_x, text_y))

        # Secondary text
        if item.secondary_text:
            sec = font_secondary.render(item.secondary_text, True, theme.text_dim)
            surface.blit(sec, (text_x, text_y + font_primary.get_linesize() + 1))

        # Right-aligned text
        if item.right_text:
            right_surf = font_right.render(item.right_text, True, theme.text_dim)
            rx = card_x + card_w - PAD_X - right_surf.get_width()
            ry = y + (card_h - right_surf.get_height()) // 2 + 1
            surface.blit(right_surf, (rx, ry))


def _window_start(cursor: int, total: int, visible: int) -> int:
    if total <= visible:
        return 0
    start = cursor - 1
    start = max(0, start)
    start = min(start, total - visible)
    return start


def _draw_scroll_indicator(
    surface: pygame.Surface,
    rect: pygame.Rect,
    cursor: int,
    total: int,
    theme: Theme,
) -> None:
    ind_x = rect.right - 5
    bar_top = rect.y + PAD_Y
    bar_h = rect.height - PAD_Y * 2

    if total <= 1 or bar_h <= 0:
        return

    # Track
    track_color = (
        min(theme.border[0] + 10, 255),
        min(theme.border[1] + 10, 255),
        min(theme.border[2] + 10, 255),
    )
    pygame.draw.line(surface, track_color, (ind_x, bar_top), (ind_x, bar_top + bar_h))

    # Thumb
    thumb_h = max(8, bar_h // total)
    progress = cursor / (total - 1)
    thumb_y = bar_top + int((bar_h - thumb_h) * progress)
    pygame.draw.rect(surface, theme.text_dim, (ind_x - 1, thumb_y, 3, thumb_h), border_radius=1)
