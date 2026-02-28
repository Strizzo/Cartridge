"""CartridgeRunner: pygame init + asyncio game loop."""

from __future__ import annotations

import asyncio
import logging

import pygame

from cartridge_sdk.app import CartridgeApp, AppContext
from cartridge_sdk.input import InputManager
from cartridge_sdk.manifest import AppManifest
from cartridge_sdk.net import HttpClient
from cartridge_sdk.screen import Screen, init_fonts
from cartridge_sdk.storage import AppStorage
from cartridge_sdk.theme import Theme

log = logging.getLogger(__name__)

TARGET_FPS = 30


class CartridgeRunner:
    """Runs a CartridgeApp with pygame and asyncio."""

    def __init__(
        self,
        app: CartridgeApp,
        manifest: AppManifest,
        width: int = 640,
        height: int = 480,
        fullscreen: bool = False,
    ) -> None:
        self.app = app
        self.manifest = manifest
        self.width = width
        self.height = height
        self.fullscreen = fullscreen
        self._running = False
        self._surface: pygame.Surface | None = None
        self._clock: pygame.time.Clock | None = None
        self._input: InputManager | None = None
        self._http: HttpClient | None = None
        self._storage: AppStorage | None = None

    def _request_quit(self) -> None:
        self._running = False

    async def run(self) -> None:
        self._init_pygame()
        self._running = True

        self._storage = AppStorage(self.manifest.id)
        self._http = HttpClient(self._storage.cache_dir)
        theme = Theme()

        ctx = AppContext(
            screen=Screen(self._surface, theme),
            http=self._http,
            storage=self._storage,
            theme=theme,
            quit=self._request_quit,
        )

        try:
            await self.app.on_init(ctx)
            await asyncio.gather(
                self._game_loop(),
                self._io_loop(),
            )
        except asyncio.CancelledError:
            pass
        finally:
            self.app.on_destroy()
            await self._http.close()
            self._cleanup()

    def _init_pygame(self) -> None:
        pygame.init()
        pygame.display.set_caption(self.manifest.name)

        flags = 0
        if self.fullscreen:
            flags |= pygame.FULLSCREEN

        self._surface = pygame.display.set_mode((self.width, self.height), flags)
        self._clock = pygame.time.Clock()
        self._input = InputManager()
        init_fonts()

        log.info("Cartridge started: %s (%dx%d)", self.manifest.name, self.width, self.height)

    def _cleanup(self) -> None:
        pygame.quit()

    async def _game_loop(self) -> None:
        while self._running:
            dt = self._clock.tick(TARGET_FPS) / 1000.0

            raw_events = pygame.event.get()
            for ev in raw_events:
                if ev.type == pygame.QUIT:
                    self._running = False
                    return
                if ev.type == pygame.KEYDOWN and ev.key == pygame.K_ESCAPE:
                    self._running = False
                    return

            input_events = self._input.get_events(raw_events)
            for event in input_events:
                self.app.on_input(event)

            await self.app.on_update(dt)

            screen = Screen(self._surface, self.app.ctx.theme)
            self.app.on_render(screen)

            pygame.display.flip()
            await asyncio.sleep(0)

    async def _io_loop(self) -> None:
        """Keep asyncio alive for background tasks (HTTP, etc.)."""
        while self._running:
            await asyncio.sleep(0.1)
