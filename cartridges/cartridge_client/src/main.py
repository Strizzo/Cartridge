"""Cartridge Client - App store and launcher."""

import asyncio
from cartridge_sdk import CartridgeApp, AppContext, Screen, InputEvent
from cartridge_sdk.input import Button
from cartridge_sdk.management import is_installed, remove_cartridge, request_launch

from registry import RegistryClient, RegistryApp
from screens.store import StoreScreen
from screens.installed import InstalledScreen
from screens.detail import DetailScreen
from screens.settings import SettingsScreen

# Default registry URL - can be overridden via settings
REGISTRY_URL = "https://raw.githubusercontent.com/Strizzo/cartridge/main/registry.json"


class CartridgeClientApp(CartridgeApp):

    async def on_init(self, ctx: AppContext) -> None:
        await super().on_init(ctx)

        # Load saved settings to get registry URL override
        saved_settings = ctx.storage.load("settings") or {}
        registry_url = saved_settings.get("registry_url", REGISTRY_URL)

        self._registry_client = RegistryClient(ctx.http, registry_url)
        self._registry_apps: dict[str, RegistryApp] = {}

        self._screen_stack: list[str] = ["store"]

        self.store_screen = StoreScreen(
            registry_client=self._registry_client,
            on_app_select=self._open_detail_from_store,
            on_installed=self._open_installed,
        )
        self.installed_screen = InstalledScreen(
            on_launch=self._launch_app,
            on_detail=self._open_detail_from_installed,
            on_remove=self._remove_app,
            on_back=self._go_back_to_store,
        )
        self.detail_screen = DetailScreen(
            on_launch=self._launch_app,
            on_back=self._go_back,
        )
        self.settings_screen = SettingsScreen(
            storage=ctx.storage,
            on_back=self._go_back,
            on_registry_changed=self._update_registry_url,
        )

        asyncio.create_task(self.store_screen.load())

    def _open_detail_from_store(self, app: RegistryApp) -> None:
        self._registry_apps[app.id] = app
        self.detail_screen.set_app(app)
        self._screen_stack.append("detail")

    def _open_installed(self) -> None:
        self._screen_stack = ["installed"]
        self.installed_screen.refresh()

    def _open_detail_from_installed(self, app_id: str) -> None:
        if app_id in self._registry_apps:
            self.detail_screen.set_app(self._registry_apps[app_id])
            self._screen_stack.append("detail")

    def _go_back_to_store(self) -> None:
        self._screen_stack = ["store"]

    def _go_back(self) -> None:
        if len(self._screen_stack) > 1:
            self._screen_stack.pop()

    def _launch_app(self, app_id: str) -> None:
        if is_installed(app_id):
            request_launch(app_id)
            self.ctx.quit()

    def _remove_app(self, app_id: str) -> None:
        remove_cartridge(app_id)

    def _update_registry_url(self, url: str) -> None:
        self._registry_client.url = url
        asyncio.create_task(self.store_screen.load())

    def on_input(self, event: InputEvent) -> None:
        # START opens settings from any screen
        if (
            event.button == Button.START
            and event.action == "press"
            and self._screen_stack[-1] != "settings"
        ):
            self._screen_stack.append("settings")
            return

        current = self._screen_stack[-1]
        if current == "store":
            self.store_screen.handle_input(event)
        elif current == "installed":
            self.installed_screen.handle_input(event)
        elif current == "detail":
            self.detail_screen.handle_input(event)
        elif current == "settings":
            self.settings_screen.handle_input(event)

    async def on_update(self, dt: float) -> None:
        current = self._screen_stack[-1]
        if current == "store":
            self.store_screen.update(dt)
        elif current == "installed":
            self.installed_screen.update(dt)
        elif current == "detail":
            self.detail_screen.update(dt)

    def on_render(self, screen: Screen) -> None:
        current = self._screen_stack[-1]
        if current == "store":
            self.store_screen.draw(screen)
        elif current == "installed":
            self.installed_screen.draw(screen)
        elif current == "detail":
            self.detail_screen.draw(screen)
        elif current == "settings":
            self.settings_screen.draw(screen)
