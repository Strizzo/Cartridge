"""CartridgeApp base class and AppContext."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Callable, TYPE_CHECKING

if TYPE_CHECKING:
    from cartridge_sdk.input import InputEvent
    from cartridge_sdk.net import HttpClient
    from cartridge_sdk.screen import Screen
    from cartridge_sdk.storage import AppStorage
    from cartridge_sdk.theme import Theme


@dataclass
class AppContext:
    """Platform services injected into CartridgeApp.on_init()."""

    screen: Screen
    http: HttpClient
    storage: AppStorage
    theme: Theme
    quit: Callable[[], None] = field(default=lambda: None)


class CartridgeApp:
    """Base class every Cartridge app extends."""

    ctx: AppContext

    async def on_init(self, ctx: AppContext) -> None:
        """Called once at startup. Store ctx, load config, set up state."""
        self.ctx = ctx

    def on_input(self, event: InputEvent) -> None:
        """Called for every button press/release/repeat."""
        pass

    async def on_update(self, dt: float) -> None:
        """Called every frame. dt is seconds since last frame."""
        pass

    def on_render(self, screen: Screen) -> None:
        """Called every frame after on_update. Draw everything here."""
        pass

    def on_suspend(self) -> None:
        """Called when the user switches away from this app."""
        pass

    def on_resume(self) -> None:
        """Called when the user returns to this app."""
        pass

    def on_destroy(self) -> None:
        """Called on exit. Save state, clean up resources."""
        pass
