"""Browse stocks by sector."""

from __future__ import annotations

import asyncio
from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font
from cartridge_sdk.ui import TabBar, Tab, StatusBar

if TYPE_CHECKING:
    from models import WatchlistItem

from data import SECTORS, get_stocks_by_sector, STOCK_UNIVERSE
from models import StockInfo

CARD_RADIUS = 6
ROW_HEIGHT = 44

# Short sector labels for tabs
SECTOR_LABELS = {
    "Technology": "Tech",
    "Finance": "Finance",
    "Healthcare": "Health",
    "Energy": "Energy",
    "Consumer": "Consumer",
    "Industrial": "Industry",
    "Communication": "Comm",
    "Crypto": "Crypto",
}

TIER_COLORS = {
    "mega": (255, 200, 60),
    "large": (100, 180, 255),
    "mid": (140, 140, 160),
}


class BrowseScreen:
    """Browse stock universe by sector with add/remove from watchlist."""

    def __init__(
        self,
        watchlist: list[WatchlistItem],
        on_back: Callable,
        on_add: Callable[[StockInfo], None],
        on_remove: Callable[[str], None],
    ) -> None:
        self.watchlist = watchlist
        self.on_back = on_back
        self.on_add = on_add
        self.on_remove = on_remove

        self._sector_idx: int = 0
        self._cursor: int = 0
        self._stocks: list[StockInfo] = []
        self._update_stocks()

    def _update_stocks(self) -> None:
        sector = SECTORS[self._sector_idx]
        self._stocks = get_stocks_by_sector(sector)
        self._cursor = min(self._cursor, max(0, len(self._stocks) - 1))

    def _is_in_watchlist(self, symbol: str) -> bool:
        return any(w.symbol == symbol for w in self.watchlist)

    def handle_input(self, event: InputEvent) -> None:
        if event.action not in ("press", "repeat"):
            return

        if event.button == Button.B:
            self.on_back()
        elif event.button == Button.L1:
            self._sector_idx = (self._sector_idx - 1) % len(SECTORS)
            self._cursor = 0
            self._update_stocks()
        elif event.button == Button.R1:
            self._sector_idx = (self._sector_idx + 1) % len(SECTORS)
            self._cursor = 0
            self._update_stocks()
        elif event.button == Button.DPAD_UP:
            self._cursor = max(0, self._cursor - 1)
        elif event.button == Button.DPAD_DOWN:
            self._cursor = min(max(0, len(self._stocks) - 1), self._cursor + 1)
        elif event.button == Button.A and self._stocks:
            stock = self._stocks[self._cursor]
            if not self._is_in_watchlist(stock.symbol):
                self.on_add(stock)
        elif event.button == Button.X and self._stocks:
            stock = self._stocks[self._cursor]
            if self._is_in_watchlist(stock.symbol):
                self.on_remove(stock.symbol)

    def draw(self, screen: Screen) -> None:
        screen.clear()
        theme = screen.theme
        s = screen.surface

        # Header
        screen.draw_gradient_rect((0, 0, 640, 40), theme.header_gradient_top, theme.header_gradient_bottom)
        screen.draw_text("Browse Stocks", 16, 8, bold=True, font_size=20)

        count_str = f"{len(STOCK_UNIVERSE)} stocks"
        cw = screen.get_text_width(count_str, 12)
        screen.draw_text(count_str, 624 - cw, 14, color=theme.text_dim, font_size=12)

        # Sector tabs
        y = 44
        tab_x = 10
        for i, sector in enumerate(SECTORS):
            label = SECTOR_LABELS.get(sector, sector)
            is_active = i == self._sector_idx
            tw = screen.get_text_width(label, 11, bold=is_active)
            tab_w = tw + 12
            if is_active:
                pygame.draw.rect(s, theme.accent, (tab_x, y, tab_w, 20), border_radius=4)
                screen.draw_text(label, tab_x + 6, y + 3, color=(20, 20, 30), font_size=11, bold=True)
            else:
                pygame.draw.rect(s, theme.card_bg, (tab_x, y, tab_w, 20), border_radius=4)
                screen.draw_text(label, tab_x + 6, y + 3, color=theme.text_dim, font_size=11)
            tab_x += tab_w + 4

        # Stock list
        list_y = 70
        list_h = 374
        self._draw_stock_list(screen, list_y, list_h)

        # Footer
        self._draw_footer(screen)

    def _draw_stock_list(self, screen: Screen, y_start: int, height: int) -> None:
        theme = screen.theme
        s = screen.surface

        stocks = self._stocks
        n = len(stocks)

        if n == 0:
            screen.draw_text("No stocks in this sector", 240, y_start + 40, color=theme.text_dim, font_size=14)
            return

        visible = max(1, height // ROW_HEIGHT)
        self._cursor = max(0, min(self._cursor, n - 1))

        if n <= visible:
            start = 0
        else:
            start = max(0, min(self._cursor - 1, n - visible))
        end = min(start + visible, n)

        clip = s.get_clip()
        s.set_clip(pygame.Rect(0, y_start, 640, height))

        y = y_start + 2
        for idx in range(start, end):
            stock = stocks[idx]
            is_selected = idx == self._cursor
            in_watchlist = self._is_in_watchlist(stock.symbol)

            card_x = 6
            card_w = 628
            card_h = ROW_HEIGHT - 4
            card_rect = pygame.Rect(card_x, y, card_w, card_h)

            if is_selected:
                pygame.draw.rect(s, theme.card_highlight, card_rect, border_radius=CARD_RADIUS)
                pygame.draw.rect(s, theme.accent, card_rect, 1, border_radius=CARD_RADIUS)
            else:
                pygame.draw.rect(s, theme.card_bg, card_rect, border_radius=CARD_RADIUS)

            # Symbol
            font_sym = _get_font("mono_bold", 15)
            sym_surf = font_sym.render(stock.symbol, True, theme.text)
            s.blit(sym_surf, (card_x + 12, y + 4))

            # Name
            font_name = _get_font("mono", 12)
            name_surf = font_name.render(stock.name, True, theme.text_dim)
            s.blit(name_surf, (card_x + 12, y + 4 + font_sym.get_linesize()))

            # Market cap tier badge
            tier_color = TIER_COLORS.get(stock.market_cap_tier, (140, 140, 160))
            tier_label = stock.market_cap_tier.upper()
            screen.draw_pill(tier_label, card_x + card_w - 140, y + (card_h - 16) // 2,
                             bg_color=tier_color, text_color=(20, 20, 30), font_size=10)

            # Watchlist status
            if in_watchlist:
                screen.draw_pill("IN LIST", card_x + card_w - 70, y + (card_h - 16) // 2,
                                 bg_color=theme.positive, text_color=(20, 20, 30), font_size=10)

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

    def _draw_footer(self, screen: Screen) -> None:
        theme = screen.theme
        y = 444
        h = 36
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, h))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        hx += screen.draw_button_hint("B", "Back", hx, y + 8, btn_color=theme.btn_b) + 14
        hx += screen.draw_button_hint("L1/R1", "Sector", hx, y + 8, btn_color=theme.btn_l) + 14

        stock = self._stocks[self._cursor] if self._stocks else None
        if stock:
            if self._is_in_watchlist(stock.symbol):
                hx += screen.draw_button_hint("X", "Remove", hx, y + 8, btn_color=theme.btn_x) + 14
            else:
                hx += screen.draw_button_hint("A", "Add", hx, y + 8, btn_color=theme.btn_a) + 14
