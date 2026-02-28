"""Horizontal tab bar switched by L1/R1."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui.widget import Widget

if TYPE_CHECKING:
    from cartridge_sdk.theme import Theme


@dataclass
class Tab:
    """A single tab."""

    label: str
    id: str


class TabBar(Widget):
    """Horizontal tabs. L1/R1 to switch."""

    def __init__(
        self,
        tabs: list[Tab],
        on_change: Callable[[Tab], None] | None = None,
    ) -> None:
        self.tabs = tabs
        self.active_index: int = 0
        self.on_change = on_change

    @property
    def active_tab(self) -> Tab:
        return self.tabs[self.active_index]

    def handle_input(self, event: InputEvent) -> bool:
        if event.action != "press":
            return False
        if event.button == Button.L1:
            old = self.active_index
            self.active_index = max(0, self.active_index - 1)
            if self.active_index != old and self.on_change:
                self.on_change(self.active_tab)
            return self.active_index != old
        if event.button == Button.R1:
            old = self.active_index
            self.active_index = min(len(self.tabs) - 1, self.active_index + 1)
            if self.active_index != old and self.on_change:
                self.on_change(self.active_tab)
            return self.active_index != old
        return False

    def draw(self, surface: pygame.Surface, rect: pygame.Rect, theme: Theme) -> None:
        # Background
        pygame.draw.rect(surface, theme.bg_header, rect)

        if not self.tabs:
            return

        font = _get_font("mono_bold", 14)
        tab_width = rect.width // len(self.tabs)

        for i, tab in enumerate(self.tabs):
            tx = rect.x + i * tab_width
            is_active = i == self.active_index

            # Tab label
            color = theme.accent if is_active else theme.text_dim
            label = tab.label
            lw = font.size(label)[0]
            lx = tx + (tab_width - lw) // 2
            ly = rect.y + (rect.height - font.get_linesize()) // 2 - 2
            rendered = font.render(label, True, color)
            surface.blit(rendered, (lx, ly))

            # Active indicator bar
            if is_active:
                bar_y = rect.bottom - 3
                pygame.draw.rect(surface, theme.accent, (tx + 4, bar_y, tab_width - 8, 3))

        # Bottom border
        pygame.draw.line(surface, theme.border, (rect.x, rect.bottom - 1), (rect.right, rect.bottom - 1))
