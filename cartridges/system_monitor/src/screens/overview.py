"""Overview dashboard screen -- CPU, Memory, Disk, Network cards."""

from __future__ import annotations

from typing import List, Optional

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font
from cartridge_sdk.ui.chart import SparkLine

from ..stats import (
    SystemStats,
    format_bytes,
    format_rate,
    format_uptime,
    usage_color,
)

# Layout constants
CONTENT_Y = 72
CONTENT_H = 444 - CONTENT_Y  # 372
CARD_PAD = 10
CARD_W = (640 - CARD_PAD * 3) // 2  # ~307
CARD_H = (CONTENT_H - CARD_PAD * 3) // 2 - 14  # leave room for bottom info row
COL1_X = CARD_PAD
COL2_X = CARD_PAD * 2 + CARD_W
ROW1_Y = CONTENT_Y + CARD_PAD
ROW2_Y = ROW1_Y + CARD_H + CARD_PAD


class OverviewScreen:
    """Card-based dashboard with CPU, Memory, Disk, and Network summaries."""

    def __init__(self) -> None:
        self.cpu_sparkline: Optional[SparkLine] = None
        self.mem_sparkline: Optional[SparkLine] = None
        self.stats: Optional[SystemStats] = None
        self.cpu_history: List[float] = []
        self.mem_history: List[float] = []

    # ------------------------------------------------------------------
    # Public API called by main app
    # ------------------------------------------------------------------

    def update_stats(self, stats: SystemStats, cpu_history: List[float], mem_history: List[float]) -> None:
        self.stats = stats
        self.cpu_history = cpu_history
        self.mem_history = mem_history
        self.cpu_sparkline = SparkLine(data=list(cpu_history), color=(100, 180, 255))
        self.mem_sparkline = SparkLine(data=list(mem_history), color=(180, 100, 255))

    def handle_input(self, event: InputEvent) -> bool:
        return False  # no special input handling

    def draw(self, screen: Screen) -> None:
        theme = screen.theme
        surface = screen.surface

        if self.stats is None:
            screen.draw_text("Collecting data...", 240, 240, theme.text_dim, 16)
            return

        s = self.stats

        # --- CPU Card ---
        self._draw_card_bg(surface, theme, COL1_X, ROW1_Y, CARD_W, CARD_H)
        self._draw_cpu_card(screen, theme, COL1_X, ROW1_Y, CARD_W, CARD_H, s)

        # --- Memory Card ---
        self._draw_card_bg(surface, theme, COL2_X, ROW1_Y, CARD_W, CARD_H)
        self._draw_mem_card(screen, theme, COL2_X, ROW1_Y, CARD_W, CARD_H, s)

        # --- Disk Card ---
        self._draw_card_bg(surface, theme, COL1_X, ROW2_Y, CARD_W, CARD_H)
        self._draw_disk_card(screen, theme, COL1_X, ROW2_Y, CARD_W, CARD_H, s)

        # --- Network Card ---
        self._draw_card_bg(surface, theme, COL2_X, ROW2_Y, CARD_W, CARD_H)
        self._draw_net_card(screen, theme, COL2_X, ROW2_Y, CARD_W, CARD_H, s)

        # --- Bottom info row ---
        info_y = ROW2_Y + CARD_H + 8
        uptime_str = format_uptime(s.uptime_seconds)
        screen.draw_text(f"Uptime: {uptime_str}", CARD_PAD, info_y, theme.text_dim, 13)
        proc_str = f"Processes: {s.cpu.process_count}"
        pw = screen.get_text_width(proc_str, 13, False)
        screen.draw_text(proc_str, 640 - CARD_PAD - pw, info_y, theme.text_dim, 13)

    # ------------------------------------------------------------------
    # Card renderers
    # ------------------------------------------------------------------

    def _draw_cpu_card(self, screen: Screen, theme, x: int, y: int, w: int, h: int, s: SystemStats) -> None:
        inner_x = x + 12
        inner_w = w - 24
        cy = y + 10

        # Title + pill
        screen.draw_text("CPU", inner_x, cy, theme.text, 15, bold=True)
        pill_text = f"{s.cpu.overall_percent:.0f}%"
        pill_color = usage_color(s.cpu.overall_percent)
        screen.draw_pill(pill_text, x + w - 62, cy - 1, pill_color, theme.bg, 12)
        cy += 24

        # Progress bar
        screen.draw_progress_bar(
            pygame.Rect(inner_x, cy, inner_w, 14),
            s.cpu.overall_percent / 100.0,
            usage_color(s.cpu.overall_percent),
            theme.bg_lighter,
            radius=4,
        )
        cy += 22

        # Sparkline
        if self.cpu_sparkline and len(self.cpu_history) > 1:
            spark_rect = pygame.Rect(inner_x, cy, inner_w, 50)
            # Draw subtle background
            pygame.draw.rect(screen.surface, theme.bg_lighter, spark_rect, border_radius=4)
            self.cpu_sparkline.draw(screen.surface, spark_rect, theme)
            cy += 56

        # Load average
        la = s.cpu.load_avg
        screen.draw_text(f"Load: {la[0]:.2f}  {la[1]:.2f}  {la[2]:.2f}", inner_x, cy, theme.text_dim, 12)
        cy += 18

        # Core count
        screen.draw_text(f"{s.cpu.core_count} cores", inner_x, cy, theme.text_dim, 12)

    def _draw_mem_card(self, screen: Screen, theme, x: int, y: int, w: int, h: int, s: SystemStats) -> None:
        inner_x = x + 12
        inner_w = w - 24
        cy = y + 10

        # Title + pill
        screen.draw_text("Memory", inner_x, cy, theme.text, 15, bold=True)
        pill_text = f"{s.memory.percent:.0f}%"
        pill_color = usage_color(s.memory.percent)
        screen.draw_pill(pill_text, x + w - 62, cy - 1, pill_color, theme.bg, 12)
        cy += 24

        # Progress bar
        screen.draw_progress_bar(
            pygame.Rect(inner_x, cy, inner_w, 14),
            s.memory.percent / 100.0,
            usage_color(s.memory.percent),
            theme.bg_lighter,
            radius=4,
        )
        cy += 22

        # Sparkline
        if self.mem_sparkline and len(self.mem_history) > 1:
            spark_rect = pygame.Rect(inner_x, cy, inner_w, 50)
            pygame.draw.rect(screen.surface, theme.bg_lighter, spark_rect, border_radius=4)
            self.mem_sparkline.draw(screen.surface, spark_rect, theme)
            cy += 56

        # Usage text
        used_str = format_bytes(s.memory.used)
        total_str = format_bytes(s.memory.total)
        screen.draw_text(f"{used_str} / {total_str}", inner_x, cy, theme.text_dim, 12)
        cy += 18

        # Swap
        if s.memory.swap_total > 0:
            swap_str = f"Swap: {format_bytes(s.memory.swap_used)} / {format_bytes(s.memory.swap_total)}"
            screen.draw_text(swap_str, inner_x, cy, theme.text_dim, 12)

    def _draw_disk_card(self, screen: Screen, theme, x: int, y: int, w: int, h: int, s: SystemStats) -> None:
        inner_x = x + 12
        inner_w = w - 24
        cy = y + 10

        # Title + pill
        screen.draw_text("Disk", inner_x, cy, theme.text, 15, bold=True)
        pill_text = f"{s.disk.percent:.0f}%"
        pill_color = usage_color(s.disk.percent)
        screen.draw_pill(pill_text, x + w - 62, cy - 1, pill_color, theme.bg, 12)
        cy += 24

        # Progress bar
        screen.draw_progress_bar(
            pygame.Rect(inner_x, cy, inner_w, 14),
            s.disk.percent / 100.0,
            usage_color(s.disk.percent),
            theme.bg_lighter,
            radius=4,
        )
        cy += 22

        # Usage text
        used_str = format_bytes(s.disk.used)
        total_str = format_bytes(s.disk.total)
        screen.draw_text(f"{used_str} / {total_str}", inner_x, cy, theme.text_dim, 13)
        cy += 20

        free_str = format_bytes(s.disk.free)
        screen.draw_text(f"Free: {free_str}", inner_x, cy, theme.text_dim, 12)

    def _draw_net_card(self, screen: Screen, theme, x: int, y: int, w: int, h: int, s: SystemStats) -> None:
        inner_x = x + 12
        inner_w = w - 24
        cy = y + 10

        # Title
        screen.draw_text("Network", inner_x, cy, theme.text, 15, bold=True)
        cy += 28

        # Upload rate
        up_color = theme.positive
        dn_color = (100, 180, 255)

        up_str = format_rate(s.network.send_rate)
        dn_str = format_rate(s.network.recv_rate)

        # Up arrow
        screen.draw_text("\u2191", inner_x, cy, up_color, 16, bold=True)
        screen.draw_text(up_str, inner_x + 18, cy, theme.text, 14)
        cy += 24

        # Down arrow
        screen.draw_text("\u2193", inner_x, cy, dn_color, 16, bold=True)
        screen.draw_text(dn_str, inner_x + 18, cy, theme.text, 14)
        cy += 28

        # Totals
        screen.draw_text(f"Sent: {format_bytes(s.network.total_sent)}", inner_x, cy, theme.text_dim, 12)
        cy += 18
        screen.draw_text(f"Recv: {format_bytes(s.network.total_recv)}", inner_x, cy, theme.text_dim, 12)
        cy += 18

        # Connections
        if s.network.connection_count >= 0:
            screen.draw_text(f"Connections: {s.network.connection_count}", inner_x, cy, theme.text_dim, 12)
        else:
            screen.draw_text("Connections: N/A", inner_x, cy, theme.text_dim, 12)

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _draw_card_bg(surface, theme, x: int, y: int, w: int, h: int) -> None:
        """Draw a card background with border and subtle shadow."""
        shadow_rect = pygame.Rect(x + 2, y + 2, w, h)
        pygame.draw.rect(surface, theme.shadow, shadow_rect, border_radius=8)
        card_rect = pygame.Rect(x, y, w, h)
        pygame.draw.rect(surface, theme.card_bg, card_rect, border_radius=8)
        pygame.draw.rect(surface, theme.card_border, card_rect, width=1, border_radius=8)
