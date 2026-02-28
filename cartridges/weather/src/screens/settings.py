"""Settings screen – city selection."""

from __future__ import annotations

from typing import TYPE_CHECKING, Callable, Optional

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui import StatusBar

from ..models import CITIES, City

if TYPE_CHECKING:
    from cartridge_sdk.screen import Screen
    from cartridge_sdk.theme import Theme


ROW_HEIGHT = 46
CONTENT_TOP = 76
CONTENT_BOTTOM = 444
VISIBLE_ROWS = (CONTENT_BOTTOM - CONTENT_TOP) // ROW_HEIGHT  # ~8


class SettingsScreen:
    """City picker rendered as a selectable card list."""

    def __init__(
        self,
        selected_key: str,
        on_tab_change: Callable[[int], None],
        on_city_selected: Callable[[City], None],
    ) -> None:
        self._on_tab_change = on_tab_change
        self._on_city_selected = on_city_selected
        self.selected_key = selected_key
        self._cursor = 0
        self._scroll = 0
        self.status_bar = StatusBar("Settings")
        self.status_bar.right_text = ""

        # Initialize cursor to the currently selected city
        for i, c in enumerate(CITIES):
            if c.key == selected_key:
                self._cursor = i
                break
        self._ensure_visible()

    # ── input ────────────────────────────────────────────────────────────

    def handle_input(self, event: InputEvent) -> None:
        if event.action not in ("press", "repeat"):
            return
        if event.button == Button.L1:
            self._on_tab_change(-1)
        elif event.button == Button.R1:
            self._on_tab_change(1)
        elif event.button == Button.DPAD_UP:
            self._cursor = max(0, self._cursor - 1)
            self._ensure_visible()
        elif event.button == Button.DPAD_DOWN:
            self._cursor = min(len(CITIES) - 1, self._cursor + 1)
            self._ensure_visible()
        elif event.button == Button.A:
            city = CITIES[self._cursor]
            self.selected_key = city.key
            self._on_city_selected(city)

    def _ensure_visible(self) -> None:
        if self._cursor < self._scroll:
            self._scroll = self._cursor
        elif self._cursor >= self._scroll + VISIBLE_ROWS:
            self._scroll = self._cursor - VISIBLE_ROWS + 1

    # ── draw ─────────────────────────────────────────────────────────────

    def draw(self, screen: Screen) -> None:
        theme = screen.theme
        s = screen.surface
        screen.clear()

        # Status bar
        self.status_bar.draw(s, pygame.Rect(0, 0, 640, 40), theme)

        # Tab indicator
        self._draw_tab_indicator(s, theme, active=2)

        # Section title
        screen.draw_text(
            "Select a city", 20, CONTENT_TOP - 2,
            color=theme.text_dim, font_size=12,
        )

        # City cards
        for vi in range(VISIBLE_ROWS + 1):
            idx = self._scroll + vi
            if idx >= len(CITIES):
                break
            y = CONTENT_TOP + 18 + vi * ROW_HEIGHT
            if y + ROW_HEIGHT - 4 > CONTENT_BOTTOM:
                break
            self._draw_city_card(screen, CITIES[idx], y, idx)

        # Scroll indicator
        if len(CITIES) > VISIBLE_ROWS:
            self._draw_scroll_indicator(s, theme, len(CITIES))

        self._draw_footer(screen)

    # ── single city card ─────────────────────────────────────────────────

    def _draw_city_card(
        self, screen: Screen, city: City, y: int, idx: int,
    ) -> None:
        theme = screen.theme
        s = screen.surface

        is_selected = idx == self._cursor
        is_current = city.key == self.selected_key

        card_x = 6
        card_w = 628
        card_h = ROW_HEIGHT - 4
        card_rect = pygame.Rect(card_x, y, card_w, card_h)

        if is_selected:
            pygame.draw.rect(s, theme.card_highlight, card_rect, border_radius=6)
            pygame.draw.rect(s, theme.accent, card_rect, 1, border_radius=6)
        else:
            pygame.draw.rect(s, theme.card_bg, card_rect, border_radius=6)

        # City name
        name_color = theme.text if is_selected else theme.text
        screen.draw_text(
            city.name, card_x + 16, y + 6,
            color=name_color, font_size=14, bold=is_selected,
        )

        # Country + coordinates
        screen.draw_text(
            f"{city.country}  ({city.latitude:.2f}, {city.longitude:.2f})",
            card_x + 16, y + 26,
            color=theme.text_dim, font_size=11,
        )

        # Active indicator
        if is_current:
            screen.draw_pill(
                "Active", card_x + card_w - 80, y + 12,
                bg_color=theme.positive,
                text_color=(20, 20, 30),
                font_size=10,
            )

    # ── scroll indicator ─────────────────────────────────────────────────

    def _draw_scroll_indicator(self, s, theme, total: int):
        bar_x = 634
        bar_y = CONTENT_TOP + 18
        bar_h = CONTENT_BOTTOM - bar_y - 4
        if total <= 0:
            return
        thumb_h = max(16, int(bar_h * VISIBLE_ROWS / total))
        thumb_y = bar_y + int((bar_h - thumb_h) * self._scroll / max(1, total - VISIBLE_ROWS))
        pygame.draw.rect(s, theme.border, (bar_x, bar_y, 4, bar_h), border_radius=2)
        pygame.draw.rect(s, theme.accent, (bar_x, thumb_y, 4, thumb_h), border_radius=2)

    # ── tab indicator ────────────────────────────────────────────────────

    def _draw_tab_indicator(self, s, theme, active: int):
        tab_y = 42
        tab_w = 213
        labels = ["Current", "Forecast", "Settings"]
        for i, label in enumerate(labels):
            tx = i * tab_w
            is_active = i == active
            col = theme.text if is_active else theme.text_dim
            font = _get_font("mono_bold" if is_active else "mono", 12)
            txt = font.render(label, True, col)
            surface_x = tx + (tab_w - txt.get_width()) // 2
            s.blit(txt, (surface_x, tab_y + 4))
            if is_active:
                pygame.draw.rect(s, theme.accent, (tx + 20, tab_y + 24, tab_w - 40, 2), border_radius=1)
        pygame.draw.line(s, theme.border, (0, tab_y + 28), (640, tab_y + 28))

    # ── footer ───────────────────────────────────────────────────────────

    def _draw_footer(self, screen: Screen) -> None:
        theme = screen.theme
        s = screen.surface
        y = 444
        pygame.draw.rect(s, theme.bg_header, (0, y, 640, 36))
        pygame.draw.line(s, theme.border, (0, y), (640, y))
        hx = 10
        hx += screen.draw_button_hint("L1/R1", "Tab", hx, y + 8, btn_color=theme.btn_l) + 14
        hx += screen.draw_button_hint("\u2191\u2193", "Navigate", hx, y + 8, btn_color=theme.btn_a) + 14
        hx += screen.draw_button_hint("A", "Select", hx, y + 8, btn_color=theme.btn_a) + 14
