"""Weather cartridge – main entry point."""

from __future__ import annotations

import asyncio
from typing import Optional

from cartridge_sdk import CartridgeApp, AppContext, Screen, InputEvent
from cartridge_sdk.input import Button, InputEvent

from .api import WeatherApi
from .models import City, CITY_MAP, DEFAULT_CITY_KEY, CurrentWeather, ForecastData
from .screens.current import CurrentWeatherScreen
from .screens.forecast import ForecastScreen
from .screens.settings import SettingsScreen


TAB_NAMES = ["current", "forecast", "settings"]


class WeatherApp(CartridgeApp):
    """Main application class for the Weather cartridge."""

    async def on_init(self, ctx: AppContext) -> None:
        await super().on_init(ctx)

        self._ctx = ctx
        self._api = WeatherApi(ctx.http)

        # Load persisted city or default
        saved = ctx.storage.load("settings")
        city_key = DEFAULT_CITY_KEY
        if saved and isinstance(saved, dict) and saved.get("city") in CITY_MAP:
            city_key = saved["city"]
        self._city: City = CITY_MAP[city_key]

        # Tab state
        self._tab_index = 0  # 0=current, 1=forecast, 2=settings

        # Build screens
        self._build_screens()

        # Kick off initial data load
        asyncio.create_task(self._load_all())

    # ── screen construction ──────────────────────────────────────────────

    def _build_screens(self) -> None:
        self._current_screen = CurrentWeatherScreen(
            self._city,
            on_tab_change=self._switch_tab_relative,
        )
        self._forecast_screen = ForecastScreen(
            self._city,
            on_tab_change=self._switch_tab_relative,
        )
        self._settings_screen = SettingsScreen(
            self._city.key,
            on_tab_change=self._switch_tab_relative,
            on_city_selected=self._on_city_selected,
        )

    # ── tab switching ────────────────────────────────────────────────────

    def _switch_tab_relative(self, delta: int) -> None:
        new = self._tab_index + delta
        if 0 <= new < len(TAB_NAMES):
            self._tab_index = new

    # ── city selection callback ──────────────────────────────────────────

    def _on_city_selected(self, city: City) -> None:
        if city.key == self._city.key:
            return
        self._city = city
        self._ctx.storage.save("settings", {"city": city.key})

        # Rebuild weather screens with new city
        self._current_screen = CurrentWeatherScreen(
            city,
            on_tab_change=self._switch_tab_relative,
        )
        self._forecast_screen = ForecastScreen(
            city,
            on_tab_change=self._switch_tab_relative,
        )
        self._settings_screen.selected_key = city.key

        # Reload data
        asyncio.create_task(self._load_all())

    # ── data loading ─────────────────────────────────────────────────────

    async def _load_all(self) -> None:
        await asyncio.gather(
            self._load_current(),
            self._load_forecast(),
        )

    async def _load_current(self) -> None:
        scr = self._current_screen
        scr.loading.visible = True
        scr.status_bar.right_text = "Loading"
        scr.status_bar.right_color = (180, 180, 200)
        try:
            data = await self._api.fetch_current(self._city)
            scr.weather = data
            scr.status_bar.right_text = "Updated"
            scr.status_bar.right_color = (100, 220, 100)
        except Exception:
            scr.status_bar.right_text = "Error"
            scr.status_bar.right_color = (255, 100, 100)
        finally:
            scr.loading.visible = False

    async def _load_forecast(self) -> None:
        scr = self._forecast_screen
        scr.loading.visible = True
        scr.status_bar.right_text = "Loading"
        scr.status_bar.right_color = (180, 180, 200)
        try:
            data = await self._api.fetch_forecast(self._city)
            scr.forecast = data
            scr.status_bar.right_text = "Updated"
            scr.status_bar.right_color = (100, 220, 100)
        except Exception:
            scr.status_bar.right_text = "Error"
            scr.status_bar.right_color = (255, 100, 100)
        finally:
            scr.loading.visible = False

    # ── input routing ────────────────────────────────────────────────────

    def on_input(self, event: InputEvent) -> None:
        tab = TAB_NAMES[self._tab_index]
        if tab == "current":
            self._current_screen.handle_input(event)
        elif tab == "forecast":
            self._forecast_screen.handle_input(event)
        elif tab == "settings":
            self._settings_screen.handle_input(event)

    # ── render routing ───────────────────────────────────────────────────

    def on_render(self, screen: Screen) -> None:
        tab = TAB_NAMES[self._tab_index]
        if tab == "current":
            self._current_screen.draw(screen)
        elif tab == "forecast":
            self._forecast_screen.draw(screen)
        elif tab == "settings":
            self._settings_screen.draw(screen)
