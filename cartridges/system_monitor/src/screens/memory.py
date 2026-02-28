"""Memory detail screen -- RAM breakdown, swap, top processes."""

from __future__ import annotations

from typing import Optional

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font

from ..stats import SystemStats, format_bytes, usage_color

# Layout
CONTENT_Y = 72
CONTENT_BOTTOM = 444
CARD_PAD = 10
INNER_PAD = 12

# Colors for memory segments
COLOR_USED = (240, 80, 90)
COLOR_CACHED = (100, 180, 255)
COLOR_AVAILABLE = (80, 210, 120)
COLOR_SWAP = (255, 140, 40)


class MemoryScreen:
    """RAM breakdown, swap usage, and top memory-consuming processes."""

    def __init__(self) -> None:
        self.stats: Optional[SystemStats] = None

    def update_stats(self, stats: SystemStats) -> None:
        self.stats = stats

    def handle_input(self, event: InputEvent) -> bool:
        return False

    def draw(self, screen: Screen) -> None:
        theme = screen.theme
        surface = screen.surface

        if self.stats is None:
            screen.draw_text("Collecting data...", 240, 240, theme.text_dim, 16)
            return

        s = self.stats
        mem = s.memory

        # ---- RAM Card ----
        ram_card_x = CARD_PAD
        ram_card_y = CONTENT_Y + CARD_PAD
        ram_card_w = 640 - CARD_PAD * 2
        ram_card_h = 150

        self._draw_card(surface, theme, ram_card_x, ram_card_y, ram_card_w, ram_card_h)

        ix = ram_card_x + INNER_PAD
        iy = ram_card_y + 10

        screen.draw_text("RAM Usage", ix, iy, theme.text_accent, 15, bold=True)
        # Overall percentage pill
        pill_color = usage_color(mem.percent)
        screen.draw_pill(f"{mem.percent:.0f}%", ram_card_x + ram_card_w - 64, iy - 1, pill_color, theme.bg, 12)
        iy += 26

        # Segmented bar: used | cached | available
        bar_x = ix
        bar_w = ram_card_w - INNER_PAD * 2
        bar_h = 20
        bar_y = iy
        total = mem.total if mem.total > 0 else 1

        used_frac = mem.used / total
        cached_frac = mem.cached / total
        avail_frac = mem.available / total

        # Background
        pygame.draw.rect(surface, theme.bg_lighter,
                         pygame.Rect(bar_x, bar_y, bar_w, bar_h), border_radius=5)

        # Segments (draw left-to-right)
        seg_x = bar_x
        used_w = int(bar_w * used_frac)
        if used_w > 0:
            pygame.draw.rect(surface, COLOR_USED,
                             pygame.Rect(seg_x, bar_y, used_w, bar_h), border_radius=5)
        seg_x += used_w

        cached_w = int(bar_w * cached_frac)
        if cached_w > 0:
            pygame.draw.rect(surface, COLOR_CACHED,
                             pygame.Rect(seg_x, bar_y, cached_w, bar_h))
        seg_x += cached_w

        avail_w = max(0, bar_w - used_w - cached_w)
        # available is the bg_lighter already showing through

        # Outline
        pygame.draw.rect(surface, theme.card_border,
                         pygame.Rect(bar_x, bar_y, bar_w, bar_h), width=1, border_radius=5)

        iy += bar_h + 10

        # Legend
        legend_items = [
            (COLOR_USED, "Used", mem.used),
            (COLOR_CACHED, "Cached", mem.cached),
            (COLOR_AVAILABLE, "Available", mem.available),
        ]
        lx = ix
        for color, label, val in legend_items:
            # Color swatch
            pygame.draw.rect(surface, color, pygame.Rect(lx, iy + 2, 10, 10), border_radius=2)
            txt = f"{label}: {format_bytes(val)}"
            screen.draw_text(txt, lx + 14, iy, theme.text, 12)
            lx += screen.get_text_width(txt, 12, False) + 28

        iy += 22
        screen.draw_text(f"Total: {format_bytes(mem.total)}", ix, iy, theme.text, 13, bold=True)

        # ---- Swap Card ----
        swap_card_y = ram_card_y + ram_card_h + CARD_PAD
        swap_card_h = 70
        self._draw_card(surface, theme, ram_card_x, swap_card_y, ram_card_w, swap_card_h)

        sy = swap_card_y + 10
        screen.draw_text("Swap", ix, sy, theme.text_accent, 15, bold=True)

        if mem.swap_total > 0:
            swap_pct = mem.swap_percent
            pill_color = usage_color(swap_pct)
            screen.draw_pill(f"{swap_pct:.0f}%", ram_card_x + ram_card_w - 64, sy - 1, pill_color, theme.bg, 12)
            sy += 26

            screen.draw_progress_bar(
                pygame.Rect(ix, sy, ram_card_w - INNER_PAD * 2, 14),
                swap_pct / 100.0,
                COLOR_SWAP,
                theme.bg_lighter,
                radius=4,
            )
            swap_txt = f"{format_bytes(mem.swap_used)} / {format_bytes(mem.swap_total)}"
            tw = screen.get_text_width(swap_txt, 12, False)
            screen.draw_text(swap_txt, ram_card_x + ram_card_w - INNER_PAD - tw, sy - 18, theme.text_dim, 12)
        else:
            sy += 24
            screen.draw_text("No swap configured", ix, sy, theme.text_dim, 13)

        # ---- Top Processes Card ----
        proc_card_y = swap_card_y + swap_card_h + CARD_PAD
        proc_card_h = CONTENT_BOTTOM - proc_card_y - CARD_PAD
        if proc_card_h < 40:
            return
        self._draw_card(surface, theme, ram_card_x, proc_card_y, ram_card_w, proc_card_h)

        py = proc_card_y + 10
        screen.draw_text("Top Processes (by Memory)", ix, py, theme.text_accent, 14, bold=True)
        py += 24

        procs = s.top_mem_processes
        if not procs:
            screen.draw_text("No process data available", ix, py, theme.text_dim, 13)
            return

        # Header
        name_x = ix
        rss_x = ram_card_x + ram_card_w - INNER_PAD - 100
        pct_x = ram_card_x + ram_card_w - INNER_PAD - 40

        screen.draw_text("Process", name_x, py, theme.text_dim, 12, bold=True)
        screen.draw_text("RSS", rss_x, py, theme.text_dim, 12, bold=True)
        py += 18
        pygame.draw.line(surface, theme.border, (ix, py), (ram_card_x + ram_card_w - INNER_PAD, py))
        py += 4

        for i, proc in enumerate(procs[:5]):
            if py + 18 > proc_card_y + proc_card_h - 6:
                break

            # Alternate row bg
            if i % 2 == 0:
                row_rect = pygame.Rect(ix - 4, py - 1, ram_card_w - INNER_PAD * 2 + 8, 18)
                pygame.draw.rect(surface, theme.bg_lighter, row_rect, border_radius=3)

            # Truncate name
            name = proc.name
            max_name_w = rss_x - name_x - 10
            displayed_name = name
            while screen.get_text_width(displayed_name, 12, False) > max_name_w and len(displayed_name) > 3:
                displayed_name = displayed_name[:-1]
            if displayed_name != name:
                displayed_name = displayed_name[:-1] + "\u2026"

            screen.draw_text(displayed_name, name_x, py, theme.text, 12)
            screen.draw_text(format_bytes(proc.rss), rss_x, py, theme.text, 12)
            py += 20

    # ------------------------------------------------------------------
    @staticmethod
    def _draw_card(surface, theme, x: int, y: int, w: int, h: int) -> None:
        pygame.draw.rect(surface, theme.shadow,
                         pygame.Rect(x + 2, y + 2, w, h), border_radius=8)
        pygame.draw.rect(surface, theme.card_bg,
                         pygame.Rect(x, y, w, h), border_radius=8)
        pygame.draw.rect(surface, theme.card_border,
                         pygame.Rect(x, y, w, h), width=1, border_radius=8)
