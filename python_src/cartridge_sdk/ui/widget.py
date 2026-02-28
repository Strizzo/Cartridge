"""Base class for all UI widgets."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pygame

if TYPE_CHECKING:
    from cartridge_sdk.input import InputEvent
    from cartridge_sdk.theme import Theme


class Widget:
    """Base class for all Cartridge UI widgets."""

    def handle_input(self, event: InputEvent) -> bool:
        """Process input. Returns True if the widget consumed the event."""
        return False

    def update(self, dt: float) -> None:
        """Called each frame for animations, timers, etc."""
        pass

    def draw(self, surface: pygame.Surface, rect: pygame.Rect, theme: Theme) -> None:
        """Render the widget within the given rectangle."""
        pass
