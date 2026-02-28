"""Animated loading indicator."""

from __future__ import annotations

import time
from typing import TYPE_CHECKING

import pygame

from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui.widget import Widget

if TYPE_CHECKING:
    from cartridge_sdk.theme import Theme


class LoadingIndicator(Widget):
    """Loading text with animated dots."""

    def __init__(self, text: str = "Loading") -> None:
        self.text = text
        self.visible = False

    def draw(self, surface: pygame.Surface, rect: pygame.Rect, theme: Theme) -> None:
        if not self.visible:
            return

        # Animated dots: . .. ...
        dots = "." * (int(time.monotonic() * 2) % 3 + 1)
        display = f"{self.text}{dots}"

        font = _get_font("mono", 16)

        # Semi-transparent overlay
        overlay = pygame.Surface(rect.size, pygame.SRCALPHA)
        overlay.fill((*theme.bg, 180))
        surface.blit(overlay, rect.topleft)

        # Centered text
        rendered = font.render(display, True, theme.text_accent)
        x = rect.x + (rect.width - rendered.get_width()) // 2
        y = rect.y + (rect.height - rendered.get_height()) // 2
        surface.blit(rendered, (x, y))
