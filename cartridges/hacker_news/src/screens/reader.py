"""Article reader screen for Hacker News."""

from __future__ import annotations

from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen
from cartridge_sdk.ui import StatusBar, ReaderView

if TYPE_CHECKING:
    from cartridge_sdk.net import HttpClient


class ReaderScreen:
    """Full-screen article reader wrapping ReaderView."""

    def __init__(self, on_back: Callable) -> None:
        self.on_back = on_back
        self.status_bar = StatusBar("Article")
        self.reader_view = ReaderView(on_back=on_back)

    async def load(self, url: str, http: HttpClient) -> None:
        domain = url.split("/")[2] if len(url.split("/")) > 2 else url
        self.status_bar.title = domain
        await self.reader_view.load(url, http)

    def handle_input(self, event: InputEvent) -> None:
        self.reader_view.handle_input(event)

    def draw(self, screen: Screen) -> None:
        screen.clear()
        theme = screen.theme
        s = screen.surface

        # Status bar
        self.status_bar.draw(s, pygame.Rect(0, 0, 640, 40), theme)

        # Reader content
        content_rect = pygame.Rect(0, 40, 640, 404)
        self.reader_view.draw(s, content_rect, theme)

        # Footer
        self._draw_footer(screen)

    def _draw_footer(self, screen: Screen) -> None:
        theme = screen.theme
        y = 444
        h = 36
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, h))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        hx += screen.draw_button_hint("B", "Back", hx, y + 8, btn_color=theme.btn_b) + 14
        hx += screen.draw_button_hint("\u2191\u2193", "Scroll", hx, y + 8, btn_color=theme.btn_l) + 14
        hx += screen.draw_button_hint("L2/R2", "Page", hx, y + 8, btn_color=theme.btn_l) + 14
