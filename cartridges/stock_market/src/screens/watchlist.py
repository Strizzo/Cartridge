"""Watchlist screen with card-style rows and sparklines."""

from __future__ import annotations

import asyncio
from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font
from cartridge_sdk.ui import TabBar, Tab, StatusBar, LoadingIndicator
from cartridge_sdk.ui.chart import SparkLine

if TYPE_CHECKING:
    from data import StockDataFetcher, StockQuote
    from models import WatchlistItem, PriceHistory

CARD_RADIUS = 6
CARD_MARGIN = 3
ROW_HEIGHT = 56


class WatchlistScreen:
    """Watchlist + Indices with card-style rows and sparklines."""

    def __init__(
        self,
        data_fetcher: StockDataFetcher,
        watchlist: list[WatchlistItem],
        indices: list[WatchlistItem],
        on_stock_select: Callable,
        on_browse: Callable | None = None,
        crypto: list[WatchlistItem] | None = None,
    ) -> None:
        self.data = data_fetcher
        self.watchlist = watchlist
        self.indices = indices
        self.crypto = crypto or []
        self.on_stock_select = on_stock_select
        self.on_browse = on_browse

        self.tabs = TabBar(
            tabs=[Tab("Watchlist", "watchlist"), Tab("Indices", "indices"), Tab("Crypto", "crypto")],
            on_change=self._on_tab_change,
        )

        self.status_bar = StatusBar("Stock Market")
        self.loading = LoadingIndicator("Fetching quotes")

        self._quotes: dict[str, list] = {}  # tab_id -> quotes
        self._sparklines: dict[str, list[float]] = {}  # symbol -> price history
        self._cursor: int = 0
        self._refresh_timer: float = 0
        self._refresh_interval: float = 60.0

    def _on_tab_change(self, tab: Tab) -> None:
        self._cursor = 0
        if tab.id in self._quotes:
            pass  # already have data
        else:
            asyncio.create_task(self.load())

    async def load(self) -> None:
        self.loading.visible = True

        tab_id = self.tabs.active_tab.id
        if tab_id == "watchlist":
            symbols = [w.symbol for w in self.watchlist]
        elif tab_id == "crypto":
            symbols = [w.symbol for w in self.crypto]
        else:
            symbols = [w.symbol for w in self.indices]

        try:
            quotes = await self.data.get_quotes(symbols)
            self._quotes[tab_id] = quotes
            self.status_bar.right_text = "Live"
            self.status_bar.right_color = (100, 220, 100)

            # Fetch sparkline data for each symbol
            for q in quotes:
                if q.symbol not in self._sparklines and q.price > 0:
                    asyncio.create_task(self._load_sparkline(q.symbol))
        except Exception:
            self.status_bar.right_text = "Error"
            self.status_bar.right_color = (255, 100, 100)
        finally:
            self.loading.visible = False

    async def _load_sparkline(self, symbol: str) -> None:
        try:
            history = await self.data.get_history(symbol, "5d")
            if history.prices:
                self._sparklines[symbol] = history.prices
        except Exception:
            pass

    async def update(self, dt: float) -> None:
        self._refresh_timer += dt
        if self._refresh_timer >= self._refresh_interval:
            self._refresh_timer = 0
            asyncio.create_task(self.load())

    def handle_input(self, event: InputEvent) -> None:
        if self.tabs.handle_input(event):
            return
        if event.action not in ("press", "repeat"):
            return

        tab_id = self.tabs.active_tab.id
        quotes = self._quotes.get(tab_id, [])

        if event.button == Button.DPAD_UP:
            self._cursor = max(0, self._cursor - 1)
        elif event.button == Button.DPAD_DOWN:
            self._cursor = min(max(0, len(quotes) - 1), self._cursor + 1)
        elif event.button == Button.A and quotes:
            if self._cursor < len(quotes):
                self.on_stock_select(quotes[self._cursor])
        elif event.button == Button.X:
            asyncio.create_task(self.load())
        elif event.button == Button.Y and self.on_browse:
            self.on_browse()

    def draw(self, screen: Screen) -> None:
        screen.clear()
        theme = screen.theme
        s = screen.surface

        # Status bar (y=0, h=40)
        self.status_bar.draw(s, pygame.Rect(0, 0, 640, 40), theme)
        # Tab bar (y=40, h=32)
        self.tabs.draw(s, pygame.Rect(0, 40, 640, 32), theme)

        # Content area
        content_y = 72
        footer_y = 444
        content_h = footer_y - content_y

        tab_id = self.tabs.active_tab.id
        quotes = self._quotes.get(tab_id, [])

        if quotes:
            self._draw_quote_list(screen, quotes, content_y, content_h)

        # Loading overlay
        self.loading.draw(s, pygame.Rect(0, content_y, 640, content_h), theme)

        # Footer
        self._draw_footer(screen)

    def _draw_quote_list(self, screen: Screen, quotes: list, y_start: int, height: int) -> None:
        theme = screen.theme
        s = screen.surface

        visible = max(1, height // ROW_HEIGHT)
        n = len(quotes)
        self._cursor = max(0, min(self._cursor, n - 1))

        # Sliding window
        if n <= visible:
            start = 0
        else:
            start = max(0, min(self._cursor - 1, n - visible))
        end = min(start + visible, n)

        clip = s.get_clip()
        s.set_clip(pygame.Rect(0, y_start, 640, height))

        y = y_start + 2
        for idx in range(start, end):
            q = quotes[idx]
            is_selected = idx == self._cursor
            self._draw_quote_row(screen, q, y, is_selected)
            y += ROW_HEIGHT

        s.set_clip(clip)

        # Scroll indicator
        if n > visible:
            ind_x = 635
            bar_top = y_start + 4
            bar_h = height - 8
            track_color = (min(theme.border[0]+10, 255), min(theme.border[1]+10, 255), min(theme.border[2]+10, 255))
            pygame.draw.line(s, track_color, (ind_x, bar_top), (ind_x, bar_top + bar_h))
            thumb_h = max(8, bar_h * visible // n)
            progress = self._cursor / max(1, n - 1)
            thumb_y = bar_top + int((bar_h - thumb_h) * progress)
            pygame.draw.rect(s, theme.text_dim, (ind_x - 1, thumb_y, 3, thumb_h), border_radius=1)

    def _draw_quote_row(self, screen: Screen, q, y: int, is_selected: bool) -> None:
        theme = screen.theme
        s = screen.surface

        card_x = 6
        card_w = 628
        card_h = ROW_HEIGHT - 4

        # Direction
        is_positive = q.change >= 0
        direction_color = theme.positive if is_positive else theme.negative

        # Card background
        card_rect = pygame.Rect(card_x, y, card_w, card_h)
        if is_selected:
            pygame.draw.rect(s, theme.card_highlight, card_rect, border_radius=CARD_RADIUS)
            pygame.draw.rect(s, theme.accent, card_rect, 1, border_radius=CARD_RADIUS)
        else:
            pygame.draw.rect(s, theme.card_bg, card_rect, border_radius=CARD_RADIUS)

        # Colored left border strip
        strip_rect = pygame.Rect(card_x, y + 4, 3, card_h - 8)
        pygame.draw.rect(s, direction_color, strip_rect, border_radius=1)

        # Symbol + name (strip -USD suffix for crypto)
        font_sym = _get_font("mono_bold", 15)
        font_name = _get_font("mono", 11)
        display_symbol = q.symbol.replace("-USD", "") if "-USD" in q.symbol else q.symbol
        sym_surf = font_sym.render(display_symbol, True, theme.text if is_selected else theme.text)
        s.blit(sym_surf, (card_x + 14, y + 6))

        # Name from watchlist
        name = self._get_name(q.symbol) or q.name
        name_surf = font_name.render(name, True, theme.text_dim)
        s.blit(name_surf, (card_x + 14, y + 6 + font_sym.get_linesize()))

        # Sparkline (middle area)
        sparkline_data = self._sparklines.get(q.symbol)
        if sparkline_data and len(sparkline_data) >= 2:
            spark_color = direction_color
            spark_rect = pygame.Rect(card_x + 180, y + 8, 100, card_h - 16)
            sparkline = SparkLine(sparkline_data, spark_color)
            sparkline.draw(s, spark_rect, theme)

        # Price (smart formatting for crypto)
        font_price = _get_font("mono_bold", 15)
        if not q.price:
            price_str = "N/A"
        elif q.price < 0.01:
            price_str = f"${q.price:.6f}"
        elif q.price < 1:
            price_str = f"${q.price:.4f}"
        else:
            price_str = f"${q.price:,.2f}"
        price_surf = font_price.render(price_str, True, theme.text)
        price_x = card_x + card_w - 14 - price_surf.get_width()
        s.blit(price_surf, (price_x, y + 4))

        # Change with arrow
        font_change = _get_font("mono", 12)
        arrow = "\u25B2" if is_positive else "\u25BC"
        sign = "+" if is_positive else ""
        change_str = f"{arrow} {sign}{q.change:.2f} ({sign}{q.change_pct:.1f}%)"
        if not q.price:
            change_str = "---"
        change_surf = font_change.render(change_str, True, direction_color)
        change_x = card_x + card_w - 14 - change_surf.get_width()
        s.blit(change_surf, (change_x, y + 4 + font_price.get_linesize() + 2))

    def _get_name(self, symbol: str) -> str:
        for w in self.watchlist:
            if w.symbol == symbol:
                return w.name
        for w in self.indices:
            if w.symbol == symbol:
                return w.name
        for w in self.crypto:
            if w.symbol == symbol:
                return w.name
        return ""

    def _draw_footer(self, screen: Screen) -> None:
        theme = screen.theme
        y = 444
        h = 36
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, h))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        hx += screen.draw_button_hint("L1/R1", "Tab", hx, y + 8, btn_color=theme.btn_l) + 14
        hx += screen.draw_button_hint("A", "Detail", hx, y + 8, btn_color=theme.btn_a) + 14
        hx += screen.draw_button_hint("X", "Refresh", hx, y + 8, btn_color=theme.btn_x) + 14
        if self.on_browse:
            hx += screen.draw_button_hint("Y", "Browse", hx, y + 8, btn_color=theme.btn_y) + 14
