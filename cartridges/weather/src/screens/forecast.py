"""5-day forecast screen with scrollable day cards."""

from __future__ import annotations

import math
from typing import TYPE_CHECKING, Optional, Callable

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui import StatusBar, LoadingIndicator

from ..models import (
    City,
    ForecastData,
    DayForecast,
    condition_from_code,
    temp_color,
)

if TYPE_CHECKING:
    from cartridge_sdk.screen import Screen
    from cartridge_sdk.theme import Theme


ROW_HEIGHT = 74
CONTENT_TOP = 76
CONTENT_BOTTOM = 444
VISIBLE_ROWS = (CONTENT_BOTTOM - CONTENT_TOP) // ROW_HEIGHT  # ~4


class ForecastScreen:
    """Renders the 5-day forecast view."""

    def __init__(
        self,
        city: City,
        on_tab_change: Callable[[int], None],
    ) -> None:
        self.city = city
        self._on_tab_change = on_tab_change
        self.forecast: Optional[ForecastData] = None
        self.status_bar = StatusBar("Forecast")
        self.status_bar.right_text = "Loading"
        self.status_bar.right_color = (180, 180, 200)
        self.loading = LoadingIndicator("Fetching forecast")
        self.loading.visible = True
        self._cursor = 0
        self._scroll = 0
        self._tick = 0

    # ── input ────────────────────────────────────────────────────────────

    def handle_input(self, event: InputEvent) -> None:
        if event.action not in ("press", "repeat"):
            return
        if event.button == Button.L1:
            self._on_tab_change(-1)
        elif event.button == Button.R1:
            self._on_tab_change(1)
        elif event.button == Button.DPAD_UP:
            if self.forecast and self.forecast.days:
                self._cursor = max(0, self._cursor - 1)
                self._ensure_visible()
        elif event.button == Button.DPAD_DOWN:
            if self.forecast and self.forecast.days:
                self._cursor = min(len(self.forecast.days) - 1, self._cursor + 1)
                self._ensure_visible()

    def _ensure_visible(self) -> None:
        if self._cursor < self._scroll:
            self._scroll = self._cursor
        elif self._cursor >= self._scroll + VISIBLE_ROWS:
            self._scroll = self._cursor - VISIBLE_ROWS + 1

    # ── draw ─────────────────────────────────────────────────────────────

    def draw(self, screen: Screen) -> None:
        theme = screen.theme
        s = screen.surface
        self._tick += 1
        screen.clear()

        # Status bar
        self.status_bar.draw(s, pygame.Rect(0, 0, 640, 40), theme)

        # Tab indicator
        self._draw_tab_indicator(s, theme, active=1)

        if self.loading.visible and self.forecast is None:
            self.loading.draw(s, pygame.Rect(0, 120, 640, 200), theme)
            self._draw_footer(screen)
            return

        if self.forecast is None or not self.forecast.days:
            screen.draw_text("No forecast data", 250, 220, color=theme.text_dim, font_size=16)
            self._draw_footer(screen)
            return

        days = self.forecast.days

        # City header
        screen.draw_text(
            f"{self.city.name} \u2014 5 Day Forecast",
            20, CONTENT_TOP - 2, color=theme.text_dim, font_size=12,
        )

        # Draw visible day cards
        for vi in range(VISIBLE_ROWS + 1):
            idx = self._scroll + vi
            if idx >= len(days):
                break
            y = CONTENT_TOP + 18 + vi * ROW_HEIGHT
            if y + ROW_HEIGHT - 4 > CONTENT_BOTTOM:
                break
            self._draw_day_card(screen, days[idx], y, idx == self._cursor)

        # Scroll indicator
        if len(days) > VISIBLE_ROWS:
            self._draw_scroll_indicator(s, theme, len(days))

        self._draw_footer(screen)

    # ── single day card ──────────────────────────────────────────────────

    def _draw_day_card(
        self, screen: Screen, day: DayForecast, y: int, selected: bool,
    ) -> None:
        theme = screen.theme
        s = screen.surface

        card_x = 6
        card_w = 628
        card_h = ROW_HEIGHT - 4
        card_rect = pygame.Rect(card_x, y, card_w, card_h)

        if selected:
            pygame.draw.rect(s, theme.card_highlight, card_rect, border_radius=8)
            pygame.draw.rect(s, theme.accent, card_rect, 1, border_radius=8)
        else:
            pygame.draw.rect(s, theme.card_bg, card_rect, border_radius=8)

        cond = condition_from_code(day.weather_code)

        # Day name + date
        screen.draw_text(
            f"{day.weekday}", card_x + 14, y + 10,
            color=theme.text, font_size=14, bold=True,
        )
        screen.draw_text(
            day.date, card_x + 14, y + 32,
            color=theme.text_dim, font_size=11,
        )

        # Condition pill
        screen.draw_pill(
            cond.label, card_x + 14, y + 50,
            bg_color=cond.color,
            text_color=(20, 20, 30),
            font_size=10,
        )

        # Mini weather icon (first 2 lines of art)
        font_art = _get_font("mono", 10)
        for i, line in enumerate(cond.icon_lines[:3]):
            if line:
                txt = font_art.render(line, True, cond.color)
                s.blit(txt, (card_x + 200, y + 8 + i * 14))

        # High / Low temps
        hi_color = temp_color(day.temp_max)
        lo_color = temp_color(day.temp_min)
        screen.draw_text(
            f"{day.temp_max:+.0f}\u00b0", card_x + 380, y + 10,
            color=hi_color, font_size=16, bold=True,
        )
        screen.draw_text("Hi", card_x + 380, y + 34, color=theme.text_dim, font_size=10)

        screen.draw_text(
            f"{day.temp_min:+.0f}\u00b0", card_x + 450, y + 10,
            color=lo_color, font_size=16, bold=True,
        )
        screen.draw_text("Lo", card_x + 450, y + 34, color=theme.text_dim, font_size=10)

        # Precipitation + wind
        if day.precipitation > 0:
            screen.draw_text(
                f"{day.precipitation:.1f}mm", card_x + 530, y + 10,
                color=(100, 180, 255), font_size=12,
            )
        else:
            screen.draw_text(
                "0mm", card_x + 530, y + 10,
                color=theme.text_dim, font_size=12,
            )
        screen.draw_text(
            f"{day.wind_max:.0f}km/h", card_x + 530, y + 30,
            color=(120, 220, 180), font_size=11,
        )

        # Sunrise / Sunset small text
        screen.draw_text(
            f"\u2191{day.sunrise} \u2193{day.sunset}", card_x + 510, y + 50,
            color=theme.text_dim, font_size=10,
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
        hx += screen.draw_button_hint("\u2191\u2193", "Scroll", hx, y + 8, btn_color=theme.btn_a) + 14
