"""Individual stock detail screen with chart."""

from __future__ import annotations

import asyncio
from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font
from cartridge_sdk.ui.chart import LineChart

if TYPE_CHECKING:
    from data import StockDataFetcher
    from models import StockQuote, PriceHistory

CARD_RADIUS = 8
PERIODS = [("1W", "5d"), ("1M", "1mo"), ("3M", "3mo"), ("6M", "6mo")]


class StockDetailScreen:
    """Shows detailed info for a single stock with chart."""

    def __init__(self, data_fetcher: StockDataFetcher, on_back: Callable) -> None:
        self.data = data_fetcher
        self.on_back = on_back
        self.quote: StockQuote | None = None
        self._history: PriceHistory | None = None
        self._chart = LineChart()
        self._period_idx: int = 1  # default 1M
        self._loading_history = False

    def set_quote(self, quote: StockQuote) -> None:
        self.quote = quote
        self._history = None
        self._period_idx = 1
        asyncio.create_task(self._load_history())

    async def _load_history(self) -> None:
        if not self.quote:
            return
        self._loading_history = True
        try:
            period = PERIODS[self._period_idx][1]
            history = await self.data.get_history(self.quote.symbol, period)
            self._history = history
            self._chart.data = history.prices
            self._chart.labels = history.dates
        except Exception:
            self._history = None
        finally:
            self._loading_history = False

    def handle_input(self, event: InputEvent) -> None:
        if event.action != "press":
            return
        if event.button == Button.B:
            self.on_back()
        elif event.button == Button.L1:
            self._period_idx = max(0, self._period_idx - 1)
            asyncio.create_task(self._load_history())
        elif event.button == Button.R1:
            self._period_idx = min(len(PERIODS) - 1, self._period_idx + 1)
            asyncio.create_task(self._load_history())

    def draw(self, screen: Screen) -> None:
        screen.clear()
        theme = screen.theme
        s = screen.surface

        if not self.quote:
            screen.draw_text("No data", 20, 20, color=theme.text_dim)
            return

        q = self.quote
        is_positive = q.change >= 0
        change_color = theme.positive if is_positive else theme.negative
        sign = "+" if is_positive else ""

        # Header with gradient
        screen.draw_gradient_rect((0, 0, 640, 70), theme.header_gradient_top, theme.header_gradient_bottom)
        pygame.draw.line(s, theme.border, (0, 70), (640, 70))

        # Symbol and name
        screen.draw_text(q.symbol, 20, 8, bold=True, font_size=24)
        name = q.name
        screen.draw_text(name, 20, 38, color=theme.text_dim, font_size=14)

        # Price (top right)
        price_str = f"${q.price:,.2f}" if q.price else "N/A"
        pw = screen.get_text_width(price_str, 24, bold=True)
        screen.draw_text(price_str, 620 - pw, 8, font_size=24, bold=True)

        # Change (below price, right aligned)
        arrow = "\u25B2" if is_positive else "\u25BC"
        change_str = f"{arrow} {sign}{q.change:.2f} ({sign}{q.change_pct:.1f}%)"
        if not q.price:
            change_str = "---"
        cw = screen.get_text_width(change_str, 14)
        screen.draw_text(change_str, 620 - cw, 40, color=change_color, font_size=14)

        # Period tabs
        y = 78
        tab_x = 20
        for i, (label, _) in enumerate(PERIODS):
            is_active = i == self._period_idx
            tw = screen.get_text_width(label, 12, bold=is_active)
            tab_w = tw + 16
            if is_active:
                pygame.draw.rect(s, theme.accent, (tab_x, y, tab_w, 22), border_radius=4)
                screen.draw_text(label, tab_x + 8, y + 3, color=(20, 20, 30), font_size=12, bold=True)
            else:
                pygame.draw.rect(s, theme.card_bg, (tab_x, y, tab_w, 22), border_radius=4)
                screen.draw_text(label, tab_x + 8, y + 3, color=theme.text_dim, font_size=12)
            tab_x += tab_w + 6

        # L1/R1 hint for periods
        hint_font = _get_font("mono", 10)
        hint_surf = hint_font.render("L1/R1 switch", True, theme.text_dim)
        s.blit(hint_surf, (620 - hint_surf.get_width(), y + 5))

        # Chart area
        chart_y = 108
        chart_h = 180
        chart_rect = pygame.Rect(8, chart_y, 624, chart_h)
        screen.draw_card(chart_rect, radius=CARD_RADIUS, shadow=True)

        if self._loading_history:
            font = _get_font("mono", 13)
            msg = font.render("Loading chart...", True, theme.text_dim)
            s.blit(msg, (chart_rect.x + chart_rect.width // 2 - msg.get_width() // 2,
                         chart_rect.y + chart_rect.height // 2 - msg.get_height() // 2))
        else:
            inner = pygame.Rect(chart_rect.x + 4, chart_rect.y + 4,
                                chart_rect.width - 8, chart_rect.height - 8)
            self._chart.color = change_color
            self._chart.draw(s, inner, theme)

        # Stats cards below chart
        stats_y = chart_y + chart_h + 12

        # Day range
        self._draw_stat_card(screen, 8, stats_y, 304, 60, "Day Range",
                             f"${q.low:,.2f}" if q.low else "---",
                             f"${q.high:,.2f}" if q.high else "---",
                             q.low, q.high, q.price)

        # 52-week range
        self._draw_stat_card(screen, 328, stats_y, 304, 60, "52-Week Range",
                             f"${q.week52_low:,.2f}" if q.week52_low else "---",
                             f"${q.week52_high:,.2f}" if q.week52_high else "---",
                             q.week52_low, q.week52_high, q.price)

        # Last updated
        if q.last_updated:
            import datetime
            dt = datetime.datetime.fromtimestamp(q.last_updated)
            screen.draw_text(
                f"Updated {dt.strftime('%H:%M:%S')}",
                20, 420, color=theme.text_dim, font_size=11,
            )

        # Footer
        self._draw_footer(screen)

    def _draw_stat_card(
        self, screen: Screen, x: int, y: int, w: int, h: int,
        title: str, low_str: str, high_str: str,
        low_val: float, high_val: float, current: float,
    ) -> None:
        theme = screen.theme

        screen.draw_card((x, y, w, h), radius=6, shadow=False)

        screen.draw_text(title, x + 10, y + 6, color=theme.text_dim, font_size=11)

        # Low and high labels
        screen.draw_text(low_str, x + 10, y + 22, font_size=12)
        hw = screen.get_text_width(high_str, 12)
        screen.draw_text(high_str, x + w - 10 - hw, y + 22, font_size=12)

        # Progress bar showing current position
        if low_val and high_val and high_val > low_val and current:
            progress = (current - low_val) / (high_val - low_val)
            progress = max(0.0, min(1.0, progress))
            bar_rect = (x + 10, y + 42, w - 20, 6)
            screen.draw_progress_bar(bar_rect, progress, fill_color=theme.accent, radius=3)

    def _draw_footer(self, screen: Screen) -> None:
        theme = screen.theme
        y = 444
        h = 36
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, h))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        hx += screen.draw_button_hint("B", "Back", hx, y + 8, btn_color=theme.btn_b) + 14
        hx += screen.draw_button_hint("L1/R1", "Period", hx, y + 8, btn_color=theme.btn_l) + 14
