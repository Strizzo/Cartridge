"""Columnar data table widget with card-style rows."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui.widget import Widget

if TYPE_CHECKING:
    from cartridge_sdk.theme import Theme

PAD_X = 10
PAD_Y = 6
CARD_MARGIN = 3
CARD_RADIUS = 6


@dataclass
class Column:
    """Table column definition."""

    header: str
    width_pct: float  # 0.0-1.0
    align: str = "left"  # "left", "right", "center"
    color_fn: Callable[[str], tuple | None] | None = None


class Table(Widget):
    """Columnar data display with card-style rows and selection."""

    def __init__(
        self,
        columns: list[Column],
        rows: list[list[str]] | None = None,
        on_select: Callable[[int, list[str]], None] | None = None,
    ) -> None:
        self.columns = columns
        self.rows: list[list[str]] = rows or []
        self.cursor: int = 0
        self.on_select = on_select

    def handle_input(self, event: InputEvent) -> bool:
        if not self.rows:
            return False
        if event.action not in ("press", "repeat"):
            return False

        if event.button == Button.DPAD_UP:
            self.cursor = max(0, self.cursor - 1)
            return True
        if event.button == Button.DPAD_DOWN:
            self.cursor = min(len(self.rows) - 1, self.cursor + 1)
            return True
        if event.button == Button.A and self.on_select:
            self.on_select(self.cursor, self.rows[self.cursor])
            return True

        return False

    def draw(self, surface: pygame.Surface, rect: pygame.Rect, theme: Theme) -> None:
        pygame.draw.rect(surface, theme.bg, rect)

        font_header = _get_font("mono_bold", 13)
        font_cell = _get_font("mono", 14)
        header_h = font_header.get_linesize() + PAD_Y * 2
        row_h = font_cell.get_linesize() + PAD_Y * 2 + CARD_MARGIN

        # Column pixel widths
        available_w = rect.width - PAD_X * 2
        col_widths = [int(c.width_pct * available_w) for c in self.columns]

        clip = surface.get_clip()
        surface.set_clip(rect)

        # Draw header card
        header_card = pygame.Rect(rect.x + CARD_MARGIN, rect.y + 2,
                                  rect.width - CARD_MARGIN * 2 - 8, header_h)
        pygame.draw.rect(surface, theme.bg_lighter, header_card, border_radius=CARD_RADIUS)

        hx = rect.x + PAD_X + CARD_MARGIN
        hy = rect.y + PAD_Y + 2
        for i, col in enumerate(self.columns):
            self._draw_cell(surface, col.header, hx, hy, col_widths[i], col.align,
                            font_header, theme.text_accent)
            hx += col_widths[i]

        # Rows
        if not self.rows:
            msg = font_cell.render("No data", True, theme.text_dim)
            mx = rect.x + (rect.width - msg.get_width()) // 2
            my = rect.y + header_h + 20
            surface.blit(msg, (mx, my))
            surface.set_clip(clip)
            return

        self.cursor = max(0, min(self.cursor, len(self.rows) - 1))

        visible_rows = max(1, (rect.height - header_h - PAD_Y) // row_h)
        if len(self.rows) <= visible_rows:
            start = 0
        else:
            start = max(0, min(self.cursor - 1, len(self.rows) - visible_rows))
        end = min(start + visible_rows, len(self.rows))

        y = rect.y + header_h + 4
        for row_idx in range(start, end):
            row = self.rows[row_idx]
            is_selected = row_idx == self.cursor

            # Card background for each row
            card_x = rect.x + CARD_MARGIN
            card_w = rect.width - CARD_MARGIN * 2 - 8
            card_h = row_h - CARD_MARGIN
            card_rect = pygame.Rect(card_x, y, card_w, card_h)

            if is_selected:
                pygame.draw.rect(surface, theme.card_highlight, card_rect, border_radius=CARD_RADIUS)
                pygame.draw.rect(surface, theme.accent, card_rect, 1, border_radius=CARD_RADIUS)
            else:
                pygame.draw.rect(surface, theme.card_bg, card_rect, border_radius=CARD_RADIUS)

            rx = rect.x + PAD_X + CARD_MARGIN
            for i, col in enumerate(self.columns):
                cell_text = row[i] if i < len(row) else ""
                cell_color = theme.text
                if col.color_fn:
                    custom = col.color_fn(cell_text)
                    if custom:
                        cell_color = custom

                self._draw_cell(surface, cell_text, rx, y + PAD_Y, col_widths[i], col.align,
                                font_cell, cell_color)
                rx += col_widths[i]

            y += row_h

        surface.set_clip(clip)

        # Scroll indicator
        if len(self.rows) > visible_rows:
            ind_x = rect.right - 5
            bar_top = rect.y + header_h + 4
            bar_h = rect.height - header_h - 8
            track_color = (
                min(theme.border[0] + 10, 255),
                min(theme.border[1] + 10, 255),
                min(theme.border[2] + 10, 255),
            )
            pygame.draw.line(surface, track_color, (ind_x, bar_top), (ind_x, bar_top + bar_h))
            thumb_h = max(8, bar_h * visible_rows // len(self.rows))
            progress = self.cursor / (len(self.rows) - 1)
            thumb_y = bar_top + int((bar_h - thumb_h) * progress)
            pygame.draw.rect(surface, theme.text_dim, (ind_x - 1, thumb_y, 3, thumb_h), border_radius=1)

    def _draw_cell(
        self,
        surface: pygame.Surface,
        text: str,
        x: int,
        y: int,
        width: int,
        align: str,
        font: pygame.font.Font,
        color: tuple,
    ) -> None:
        display = text
        tw = font.size(display)[0]
        if tw > width - 4:
            while len(display) > 1 and font.size(display + "..")[0] > width - 4:
                display = display[:-1]
            display += ".."
            tw = font.size(display)[0]

        rendered = font.render(display, True, color)

        if align == "right":
            surface.blit(rendered, (x + width - tw - 4, y))
        elif align == "center":
            surface.blit(rendered, (x + (width - tw) // 2, y))
        else:
            surface.blit(rendered, (x, y))
