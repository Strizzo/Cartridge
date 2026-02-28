"""Stock Market viewer for Cartridge."""

import asyncio
from cartridge_sdk import CartridgeApp, AppContext, Screen, InputEvent

from data import StockDataFetcher, DEFAULT_WATCHLIST, DEFAULT_INDICES, DEFAULT_CRYPTO
from models import WatchlistItem, StockInfo
from screens.watchlist import WatchlistScreen
from screens.detail import StockDetailScreen
from screens.browse import BrowseScreen


class StockApp(CartridgeApp):

    async def on_init(self, ctx: AppContext) -> None:
        await super().on_init(ctx)
        self.data = StockDataFetcher()

        # Load persisted watchlist or use default
        self._watchlist = self._load_watchlist()

        self._screen_stack: list[str] = ["watchlist"]

        self.watchlist_screen = WatchlistScreen(
            self.data, self._watchlist, DEFAULT_INDICES,
            self._open_stock, on_browse=self._open_browse,
            crypto=DEFAULT_CRYPTO,
        )
        self.detail_screen = StockDetailScreen(self.data, self._go_back)
        self.browse_screen = BrowseScreen(
            self._watchlist, self._go_back,
            on_add=self._add_to_watchlist,
            on_remove=self._remove_from_watchlist,
        )

        asyncio.create_task(self.watchlist_screen.load())

    def _load_watchlist(self) -> list[WatchlistItem]:
        data = self.ctx.storage.load("watchlist")
        if data and "items" in data:
            return [WatchlistItem(i["symbol"], i["name"]) for i in data["items"]]
        return list(DEFAULT_WATCHLIST)

    def _save_watchlist(self) -> None:
        self.ctx.storage.save("watchlist", {
            "items": [{"symbol": w.symbol, "name": w.name} for w in self._watchlist]
        })

    def _add_to_watchlist(self, stock: StockInfo) -> None:
        if not any(w.symbol == stock.symbol for w in self._watchlist):
            self._watchlist.append(WatchlistItem(stock.symbol, stock.name))
            self._save_watchlist()
            # Refresh watchlist data
            self.watchlist_screen._quotes.pop("watchlist", None)

    def _remove_from_watchlist(self, symbol: str) -> None:
        self._watchlist[:] = [w for w in self._watchlist if w.symbol != symbol]
        self._save_watchlist()
        self.watchlist_screen._quotes.pop("watchlist", None)

    def _open_stock(self, quote) -> None:
        self.detail_screen.set_quote(quote)
        self._screen_stack.append("detail")

    def _open_browse(self) -> None:
        self._screen_stack.append("browse")

    def _go_back(self) -> None:
        if len(self._screen_stack) > 1:
            popped = self._screen_stack.pop()
            # Refresh watchlist when coming back from browse
            if popped == "browse" and "watchlist" not in self.watchlist_screen._quotes:
                asyncio.create_task(self.watchlist_screen.load())

    def on_input(self, event: InputEvent) -> None:
        current = self._screen_stack[-1]
        if current == "watchlist":
            self.watchlist_screen.handle_input(event)
        elif current == "detail":
            self.detail_screen.handle_input(event)
        elif current == "browse":
            self.browse_screen.handle_input(event)

    async def on_update(self, dt: float) -> None:
        current = self._screen_stack[-1]
        if current == "watchlist":
            await self.watchlist_screen.update(dt)

    def on_render(self, screen: Screen) -> None:
        current = self._screen_stack[-1]
        if current == "watchlist":
            self.watchlist_screen.draw(screen)
        elif current == "detail":
            self.detail_screen.draw(screen)
        elif current == "browse":
            self.browse_screen.draw(screen)
