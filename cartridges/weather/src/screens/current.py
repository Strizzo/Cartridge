"""Current weather screen with large temperature, condition art, and stats."""

from __future__ import annotations

import math
import time
from typing import TYPE_CHECKING, Optional, Callable

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui import StatusBar, LoadingIndicator

from ..models import (
    City,
    CurrentWeather,
    condition_from_code,
    temp_color,
)

if TYPE_CHECKING:
    from cartridge_sdk.screen import Screen
    from cartridge_sdk.theme import Theme


class CurrentWeatherScreen:
    """Renders the full current-conditions view."""

    def __init__(
        self,
        city: City,
        on_tab_change: Callable[[int], None],
    ) -> None:
        self.city = city
        self._on_tab_change = on_tab_change
        self.weather: Optional[CurrentWeather] = None
        self.status_bar = StatusBar("Weather")
        self.status_bar.right_text = "Loading"
        self.status_bar.right_color = (180, 180, 200)
        self.loading = LoadingIndicator("Fetching weather")
        self.loading.visible = True
        self._tick = 0

    # ── input ────────────────────────────────────────────────────────────

    def handle_input(self, event: InputEvent) -> None:
        if event.action not in ("press", "repeat"):
            return
        if event.button == Button.L1:
            self._on_tab_change(-1)
        elif event.button == Button.R1:
            self._on_tab_change(1)

    # ── draw ─────────────────────────────────────────────────────────────

    def draw(self, screen: Screen) -> None:
        theme = screen.theme
        s = screen.surface
        self._tick += 1
        screen.clear()

        # Status bar
        self.status_bar.draw(s, pygame.Rect(0, 0, 640, 40), theme)

        # Tab indicator line
        self._draw_tab_indicator(s, theme, active=0)

        if self.loading.visible and self.weather is None:
            self.loading.draw(s, pygame.Rect(0, 120, 640, 200), theme)
            self._draw_footer(screen)
            return

        if self.weather is None:
            screen.draw_text("No data", 280, 220, color=theme.text_dim, font_size=16)
            self._draw_footer(screen)
            return

        w = self.weather
        cond = condition_from_code(w.weather_code)

        content_y = 76

        # ── City name ────────────────────────────────────────────────────
        screen.draw_text(
            f"{self.city.name}, {self.city.country}",
            20, content_y, color=theme.text_dim, font_size=13,
        )
        content_y += 22

        # ── Main temperature + condition art card ────────────────────────
        card_rect = pygame.Rect(6, content_y, 628, 150)
        screen.draw_card(card_rect, bg=theme.card_bg, border=theme.card_border, radius=10, shadow=True)

        # Large temperature
        tc = temp_color(w.temperature)
        temp_str = f"{w.temperature:+.0f}"
        screen.draw_text(temp_str, 30, content_y + 12, color=tc, font_size=24, bold=True)
        deg_x = 30 + screen.get_text_width(temp_str, 24, True) + 4
        screen.draw_text("\u00b0C", deg_x, content_y + 14, color=tc, font_size=16, bold=True)

        # Feels like
        screen.draw_text(
            f"Feels like {w.feels_like:+.0f}\u00b0",
            30, content_y + 48, color=theme.text_dim, font_size=13,
        )

        # Condition label
        screen.draw_pill(
            cond.label, 30, content_y + 72,
            bg_color=(*cond.color[:3],),
            text_color=(20, 20, 30),
            font_size=11,
        )

        # Sunrise / Sunset
        screen.draw_text(
            f"Sunrise {w.sunrise}   Sunset {w.sunset}",
            30, content_y + 98, color=theme.text_dim, font_size=11,
        )

        # ── Animated ASCII art (right side of card) ──────────────────────
        art_x = 380
        art_y = content_y + 16
        self._draw_weather_art(s, art_x, art_y, cond, w.weather_code, theme)

        content_y += 160

        # ── Stats cards row ──────────────────────────────────────────────
        stats = [
            ("Humidity", f"{w.humidity:.0f}%", (100, 180, 255)),
            ("Wind", f"{w.wind_speed:.0f} km/h", (120, 220, 180)),
            ("Pressure", f"{w.pressure:.0f} hPa", (200, 180, 255)),
        ]
        card_w = 200
        gap = 14
        start_x = (640 - (card_w * 3 + gap * 2)) // 2
        for i, (label, value, accent) in enumerate(stats):
            cx = start_x + i * (card_w + gap)
            r = pygame.Rect(cx, content_y, card_w, 62)
            screen.draw_card(r, bg=theme.card_bg, border=theme.card_border, radius=8, shadow=True)
            screen.draw_text(label, cx + 12, content_y + 8, color=theme.text_dim, font_size=11)
            screen.draw_text(value, cx + 12, content_y + 28, color=accent, font_size=16, bold=True)

        content_y += 72

        # ── 24h Sparkline card ───────────────────────────────────────────
        if w.hourly_temps:
            spark_rect = pygame.Rect(6, content_y, 628, 80)
            screen.draw_card(spark_rect, bg=theme.card_bg, border=theme.card_border, radius=8, shadow=True)
            screen.draw_text("24h Temperature Trend", 18, content_y + 6, color=theme.text_dim, font_size=11)
            data_rect = pygame.Rect(18, content_y + 24, 604, 46)
            screen.draw_sparkline(w.hourly_temps, data_rect, color=theme.accent)
            # Min/max labels
            if len(w.hourly_temps) > 1:
                lo = min(w.hourly_temps)
                hi = max(w.hourly_temps)
                screen.draw_text(
                    f"{lo:+.0f}\u00b0", data_rect.right - 70, content_y + 6,
                    color=(120, 190, 255), font_size=11,
                )
                screen.draw_text(
                    f"{hi:+.0f}\u00b0", data_rect.right - 30, content_y + 6,
                    color=(255, 180, 80), font_size=11,
                )

        self._draw_footer(screen)

    # ── animated weather art ─────────────────────────────────────────────

    def _draw_weather_art(
        self,
        surface: pygame.Surface,
        x: int, y: int,
        cond,
        code: int,
        theme,
    ) -> None:
        font = _get_font("mono", 14)
        for i, line in enumerate(cond.icon_lines):
            if line:
                txt = font.render(line, True, cond.color)
                surface.blit(txt, (x, y + i * 18))

        # Animated overlays
        t = self._tick
        if code in (61, 63, 65, 80, 81, 82):
            # Rain drops
            self._draw_rain(surface, x, y + 80, 200, 30, t)
        elif code in (71, 73, 75, 77):
            # Snow flakes
            self._draw_snow(surface, x, y + 80, 200, 30, t)
        elif code == 0:
            # Sun rays pulse
            self._draw_sun_pulse(surface, x + 100, y + 42, t)
        elif code in (1, 2, 3, 45, 48):
            # Drifting clouds
            self._draw_cloud_drift(surface, x, y + 85, t, theme)

    def _draw_rain(self, surface, x, y, w, h, t):
        color = (80, 150, 255)
        for i in range(12):
            dx = (i * 17 + t * 3) % w
            dy = (i * 13 + t * 5) % h
            pygame.draw.line(surface, color, (x + dx, y + dy), (x + dx - 2, y + dy + 6), 1)

    def _draw_snow(self, surface, x, y, w, h, t):
        color = (210, 220, 255)
        for i in range(10):
            dx = (i * 21 + t * 2) % w
            dy = (i * 17 + t * 3) % h
            pygame.draw.circle(surface, color, (x + dx, y + dy), 2)

    def _draw_sun_pulse(self, surface, cx, cy, t):
        alpha = int(40 + 25 * math.sin(t * 0.08))
        radius = int(36 + 6 * math.sin(t * 0.06))
        glow = pygame.Surface((radius * 2, radius * 2), pygame.SRCALPHA)
        pygame.draw.circle(glow, (255, 230, 80, alpha), (radius, radius), radius)
        surface.blit(glow, (cx - radius, cy - radius))

    def _draw_cloud_drift(self, surface, x, y, t, theme):
        color = (80, 80, 100)
        offset = int(10 * math.sin(t * 0.04))
        font = _get_font("mono", 11)
        txt = font.render("~  ~  ~", True, color)
        surface.blit(txt, (x + offset, y))

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
