"""Transient toast notifications."""

from __future__ import annotations

import time
from typing import TYPE_CHECKING

import pygame

from cartridge_sdk.screen import _get_font

if TYPE_CHECKING:
    from cartridge_sdk.theme import Theme


class Toast:
    """A single toast message with auto-dismiss."""

    def __init__(self, text: str, color: tuple = (220, 220, 230), duration: float = 2.5) -> None:
        self.text = text
        self.color = color
        self.created = time.monotonic()
        self.duration = duration

    @property
    def expired(self) -> bool:
        return time.monotonic() - self.created > self.duration

    @property
    def alpha(self) -> float:
        remaining = self.duration - (time.monotonic() - self.created)
        if remaining > 0.5:
            return 1.0
        return max(0.0, remaining / 0.5)


class ToastManager:
    """Manages a stack of toast notifications."""

    def __init__(self, max_toasts: int = 3) -> None:
        self._toasts: list[Toast] = []
        self._max = max_toasts

    def push(self, text: str, color: tuple = (220, 220, 230), duration: float = 2.5) -> None:
        self._toasts.append(Toast(text, color, duration))
        if len(self._toasts) > self._max:
            self._toasts.pop(0)

    def info(self, text: str, theme: Theme | None = None) -> None:
        color = theme.text if theme else (220, 220, 230)
        self.push(text, color)

    def success(self, text: str, theme: Theme | None = None) -> None:
        color = theme.text_success if theme else (100, 220, 100)
        self.push(text, color)

    def warn(self, text: str, theme: Theme | None = None) -> None:
        color = theme.text_warning if theme else (255, 200, 60)
        self.push(text, color)

    def error(self, text: str, theme: Theme | None = None) -> None:
        color = theme.text_error if theme else (255, 100, 100)
        self.push(text, color, duration=4.0)

    def draw(self, surface: pygame.Surface, theme: Theme, screen_w: int = 640, screen_h: int = 480) -> None:
        self._toasts = [t for t in self._toasts if not t.expired]
        if not self._toasts:
            return

        font = _get_font("mono", 14)
        lh = font.get_linesize()
        toast_h = lh + 12
        base_y = screen_h - 46  # above footer area

        for i, toast in enumerate(reversed(self._toasts)):
            y = base_y - (i + 1) * (toast_h + 4)
            alpha = toast.alpha

            # Background
            bg_surface = pygame.Surface((screen_w - 40, toast_h), pygame.SRCALPHA)
            bg_color = (*theme.bg_lighter, int(220 * alpha))
            bg_surface.fill(bg_color)
            surface.blit(bg_surface, (20, y))

            # Border
            border_surface = pygame.Surface((screen_w - 40, toast_h), pygame.SRCALPHA)
            border_color = (*toast.color, int(120 * alpha))
            pygame.draw.rect(border_surface, border_color, border_surface.get_rect(), 1)
            surface.blit(border_surface, (20, y))

            # Text
            text_surface = font.render(toast.text, True, toast.color)
            if alpha < 1.0:
                text_surface.set_alpha(int(255 * alpha))
            tx = 20 + (screen_w - 40 - text_surface.get_width()) // 2
            ty = y + (toast_h - lh) // 2
            surface.blit(text_surface, (tx, ty))
