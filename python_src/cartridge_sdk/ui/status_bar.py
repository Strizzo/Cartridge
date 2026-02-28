"""Top status bar widget."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pygame

from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui.widget import Widget
from cartridge_sdk.ui.wifi import WifiStatus, get_wifi_status

if TYPE_CHECKING:
    from cartridge_sdk.theme import Theme


class StatusBar(Widget):
    """Top bar showing app title, WiFi status, and optional status text."""

    def __init__(self, title: str = "") -> None:
        self.title = title
        self.right_text: str = ""
        self.right_color: tuple | None = None
        self._wifi_status: WifiStatus = get_wifi_status()
        self._wifi_timer: float = 0.0

    def update(self, dt: float) -> None:
        """Call each frame to refresh WiFi status periodically."""
        self._wifi_timer += dt
        if self._wifi_timer >= 10.0:
            self._wifi_timer = 0.0
            self._wifi_status = get_wifi_status()

    def draw(self, surface: pygame.Surface, rect: pygame.Rect, theme: Theme) -> None:
        # Background
        pygame.draw.rect(surface, theme.bg_header, rect)

        pad = 12
        font = _get_font("mono_bold", 16)

        # Title (left)
        title_surf = font.render(self.title, True, theme.text)
        surface.blit(title_surf, (rect.x + pad, rect.y + (rect.height - title_surf.get_height()) // 2))

        # Right text (status)
        right_edge = rect.right - pad
        if self.right_text:
            right_font = _get_font("mono", 13)
            color = self.right_color or theme.text_dim
            right_surf = right_font.render(self.right_text, True, color)
            rx = right_edge - right_surf.get_width()
            ry = rect.y + (rect.height - right_surf.get_height()) // 2
            surface.blit(right_surf, (rx, ry))
            right_edge = rx - 14

        # WiFi indicator (to the left of right_text)
        wifi = self._wifi_status
        wifi_font = _get_font("mono", 11)
        cy = rect.y + rect.height // 2

        if wifi.connected:
            if wifi.signal_strength > 50:
                dot_color = theme.positive
            elif wifi.signal_strength > 20:
                dot_color = theme.text_warning
            else:
                dot_color = theme.negative
            wifi_label = wifi_font.render("WiFi", True, theme.text_dim)
            wifi_w = 8 + 4 + wifi_label.get_width()
            wx = right_edge - wifi_w
            pygame.draw.circle(surface, dot_color, (wx + 4, cy), 3)
            surface.blit(wifi_label, (wx + 12, cy - wifi_label.get_height() // 2))
        else:
            wifi_label = wifi_font.render("No WiFi", True, theme.text_dim)
            wx = right_edge - wifi_label.get_width()
            surface.blit(wifi_label, (wx, cy - wifi_label.get_height() // 2))

        # Bottom border
        pygame.draw.line(surface, theme.border, (rect.x, rect.bottom - 1), (rect.right, rect.bottom - 1))
