"""System Monitor -- main entry point."""

from __future__ import annotations

import asyncio
from collections import deque
from typing import Deque, Optional

import pygame

from cartridge_sdk import CartridgeApp, AppContext, Screen, InputEvent
from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.ui import StatusBar, TabBar, Tab

from .stats import (
    PSUTIL_AVAILABLE,
    StatsCollector,
    SystemStats,
    format_rate,
)
from .screens.overview import OverviewScreen
from .screens.cpu import CpuScreen
from .screens.memory import MemoryScreen
from .screens.network import NetworkScreen

# History length: 60 seconds / 2 second refresh = 30 data points
HISTORY_LEN = 30
REFRESH_INTERVAL = 2.0


class SystemMonitorApp(CartridgeApp):
    """Real-time system monitor with CPU, memory, disk, and network stats."""

    def __init__(self) -> None:
        super().__init__()

        # UI chrome
        self.status_bar = StatusBar("System Monitor")
        self.status_bar.right_text = "2.0s"
        self.tabs = TabBar(
            tabs=[
                Tab("Overview", "overview"),
                Tab("CPU", "cpu"),
                Tab("Memory", "mem"),
                Tab("Network", "net"),
            ],
            on_change=self._on_tab_change,
        )

        # Screens
        self._overview = OverviewScreen()
        self._cpu_screen = CpuScreen()
        self._mem_screen = MemoryScreen()
        self._net_screen = NetworkScreen()
        self._active_tab = "overview"

        # Data
        self._collector = StatsCollector()
        self._stats: Optional[SystemStats] = None
        self._cpu_history: Deque[float] = deque(maxlen=HISTORY_LEN)
        self._mem_history: Deque[float] = deque(maxlen=HISTORY_LEN)

        # Timing
        self._refresh_timer: float = REFRESH_INTERVAL  # fire immediately on first update
        self._collecting: bool = False

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    async def on_start(self, ctx: AppContext) -> None:
        pass

    async def on_update(self, dt: float) -> None:
        self._refresh_timer += dt
        if self._refresh_timer >= REFRESH_INTERVAL and not self._collecting:
            self._refresh_timer = 0.0
            self._collecting = True
            try:
                stats = await asyncio.to_thread(self._collector.collect)
                self._apply_stats(stats)
            except Exception:
                pass
            finally:
                self._collecting = False

    async def on_input(self, event: InputEvent) -> None:
        # TabBar consumes L1/R1
        if self.tabs.handle_input(event):
            return

        # Manual refresh on X
        if event.button == Button.X and event.pressed and not self._collecting:
            self._refresh_timer = REFRESH_INTERVAL  # force refresh next update
            return

        # Delegate to active screen
        handler = self._active_screen()
        if handler is not None:
            handler.handle_input(event)

    async def on_render(self, screen: Screen) -> None:
        screen.clear()
        theme = screen.theme
        surface = screen.surface

        # Status bar
        self.status_bar.draw(surface, pygame.Rect(0, 0, 640, 40), theme)

        # Tab bar
        self.tabs.draw(surface, pygame.Rect(0, 40, 640, 32), theme)

        # psutil check
        if not PSUTIL_AVAILABLE:
            self._draw_no_psutil(screen)
        else:
            # Active screen content
            handler = self._active_screen()
            if handler is not None:
                handler.draw(screen)

        # Footer
        self._draw_footer(screen)

    # ------------------------------------------------------------------
    # Internals
    # ------------------------------------------------------------------

    def _on_tab_change(self, tab_id: str) -> None:
        self._active_tab = tab_id

    def _active_screen(self):
        return {
            "overview": self._overview,
            "cpu": self._cpu_screen,
            "mem": self._mem_screen,
            "net": self._net_screen,
        }.get(self._active_tab)

    def _apply_stats(self, stats: SystemStats) -> None:
        self._stats = stats

        # Update history buffers
        self._cpu_history.append(stats.cpu.overall_percent)
        self._mem_history.append(stats.memory.percent)

        # Update refresh indicator
        self.status_bar.right_text = f"{REFRESH_INTERVAL:.1f}s"

        # Push data to screens
        self._overview.update_stats(stats, list(self._cpu_history), list(self._mem_history))
        self._cpu_screen.update_stats(stats)
        self._mem_screen.update_stats(stats)
        self._net_screen.update_stats(stats)

    def _draw_no_psutil(self, screen: Screen) -> None:
        theme = screen.theme
        center_y = 220
        screen.draw_text("psutil not installed", 200, center_y, theme.negative, 18, bold=True)
        screen.draw_text(
            "Install it with: pip install psutil",
            170,
            center_y + 30,
            theme.text_dim,
            14,
        )

    def _draw_footer(self, screen: Screen) -> None:
        theme = screen.theme
        y = 444
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, 36))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))
        hx = 10
        hx += screen.draw_button_hint("L1/R1", "Tab", hx, y + 8, btn_color=theme.btn_l) + 14
        hx += screen.draw_button_hint("X", "Refresh", hx, y + 8, btn_color=theme.btn_x) + 14
        if self._active_tab in ("cpu", "net"):
            hx += screen.draw_button_hint("\u2191\u2193", "Scroll", hx, y + 8, btn_color=theme.btn_l) + 14
