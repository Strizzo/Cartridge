"""Scrollable word-wrapped text view."""

from __future__ import annotations

from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui.widget import Widget

if TYPE_CHECKING:
    from cartridge_sdk.theme import Theme

PAD_X = 12
PAD_Y = 8


class DetailView(Widget):
    """Scrollable text content with word wrap. B to go back."""

    def __init__(
        self,
        title: str = "",
        body: str = "",
        on_back: Callable | None = None,
    ) -> None:
        self.title = title
        self.body = body
        self.on_back = on_back
        self._scroll: int = 0
        self._wrapped_lines: list[tuple[str, str]] = []  # (text, style)
        self._needs_wrap: bool = True
        self._last_width: int = 0

    def set_content(self, title: str, body: str) -> None:
        self.title = title
        self.body = body
        self._scroll = 0
        self._needs_wrap = True

    def handle_input(self, event: InputEvent) -> bool:
        if event.action not in ("press", "repeat"):
            return False

        if event.button == Button.B:
            if self.on_back:
                self.on_back()
            return True
        if event.button == Button.DPAD_UP:
            self._scroll = max(0, self._scroll - 1)
            return True
        if event.button == Button.DPAD_DOWN:
            self._scroll += 1
            return True
        if event.button == Button.L2:
            self._scroll = max(0, self._scroll - 10)
            return True
        if event.button == Button.R2:
            self._scroll += 10
            return True

        return False

    def draw(self, surface: pygame.Surface, rect: pygame.Rect, theme: Theme) -> None:
        pygame.draw.rect(surface, theme.bg, rect)

        content_width = rect.width - PAD_X * 2 - 8  # scrollbar margin

        # Re-wrap if needed
        if self._needs_wrap or content_width != self._last_width:
            self._wrapped_lines = self._wrap_text(content_width, theme)
            self._last_width = content_width
            self._needs_wrap = False

        font = _get_font("mono", 14)
        font_bold = _get_font("mono_bold", 16)
        lh = font.get_linesize()
        lh_bold = font_bold.get_linesize()

        # Visible lines
        visible_lines = max(1, (rect.height - PAD_Y * 2) // lh)
        total_lines = len(self._wrapped_lines)
        max_scroll = max(0, total_lines - visible_lines)
        self._scroll = min(self._scroll, max_scroll)

        clip = surface.get_clip()
        surface.set_clip(rect)

        y = rect.y + PAD_Y
        for i in range(self._scroll, min(self._scroll + visible_lines, total_lines)):
            text, style = self._wrapped_lines[i]
            if style == "title":
                rendered = font_bold.render(text, True, theme.text)
                surface.blit(rendered, (rect.x + PAD_X, y))
                y += lh_bold
            elif style == "dim":
                rendered = font.render(text, True, theme.text_dim)
                surface.blit(rendered, (rect.x + PAD_X, y))
                y += lh
            else:
                rendered = font.render(text, True, theme.text)
                surface.blit(rendered, (rect.x + PAD_X, y))
                y += lh

        surface.set_clip(clip)

        # Scroll indicator
        if total_lines > visible_lines:
            ind_x = rect.right - 5
            bar_top = rect.y + PAD_Y
            bar_h = rect.height - PAD_Y * 2
            track_color = (
                min(theme.border[0] + 10, 255),
                min(theme.border[1] + 10, 255),
                min(theme.border[2] + 10, 255),
            )
            pygame.draw.line(surface, track_color, (ind_x, bar_top), (ind_x, bar_top + bar_h))
            thumb_h = max(8, bar_h * visible_lines // total_lines)
            progress = self._scroll / max_scroll if max_scroll > 0 else 0
            thumb_y = bar_top + int((bar_h - thumb_h) * progress)
            pygame.draw.rect(surface, theme.text_dim, (ind_x - 1, thumb_y, 3, thumb_h))

    def _wrap_text(self, max_width: int, theme: Theme) -> list[tuple[str, str]]:
        lines: list[tuple[str, str]] = []
        font = _get_font("mono", 14)
        font_bold = _get_font("mono_bold", 16)

        # Title lines
        if self.title:
            for tline in _word_wrap(self.title, font_bold, max_width):
                lines.append((tline, "title"))
            lines.append(("", "normal"))  # spacer

        # Body lines
        for paragraph in self.body.split("\n"):
            if not paragraph.strip():
                lines.append(("", "normal"))
                continue
            for wline in _word_wrap(paragraph, font, max_width):
                lines.append((wline, "normal"))

        return lines


def _word_wrap(text: str, font: pygame.font.Font, max_width: int) -> list[str]:
    if not text:
        return [""]
    words = text.split(" ")
    lines = []
    current = ""
    for word in words:
        test = f"{current} {word}".strip()
        if font.size(test)[0] <= max_width:
            current = test
        else:
            if current:
                lines.append(current)
            # Handle long words that don't fit
            if font.size(word)[0] > max_width:
                while word:
                    chunk = word
                    while font.size(chunk)[0] > max_width and len(chunk) > 1:
                        chunk = chunk[:-1]
                    lines.append(chunk)
                    word = word[len(chunk):]
                current = ""
            else:
                current = word
    if current:
        lines.append(current)
    return lines or [""]
