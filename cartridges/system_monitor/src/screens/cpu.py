"""CPU detail screen -- per-core bars, load, frequency, processes."""

from __future__ import annotations

from typing import Optional

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font

from ..stats import SystemStats, usage_color

# Layout
CONTENT_Y = 72
CONTENT_BOTTOM = 444
CARD_PAD = 10
INNER_PAD = 12
MAX_VISIBLE_CORES = 16


class CpuScreen:
    """Per-core CPU usage, load averages, frequency, and process count."""

    def __init__(self) -> None:
        self.stats: Optional[SystemStats] = None
        self._scroll = 0  # scroll offset for many cores

    def update_stats(self, stats: SystemStats) -> None:
        self.stats = stats

    def handle_input(self, event: InputEvent) -> bool:
        if self.stats is None:
            return False
        core_count = len(self.stats.cpu.per_core_percent)
        # Scrolling with D-pad up/down when many cores
        if event.button == Button.UP and event.pressed:
            self._scroll = max(0, self._scroll - 1)
            return True
        if event.button == Button.DOWN and event.pressed:
            max_scroll = max(0, core_count - MAX_VISIBLE_CORES)
            self._scroll = min(max_scroll, self._scroll + 1)
            return True
        return False

    def draw(self, screen: Screen) -> None:
        theme = screen.theme
        surface = screen.surface

        if self.stats is None:
            screen.draw_text("Collecting data...", 240, 240, theme.text_dim, 16)
            return

        s = self.stats
        cores = s.cpu.per_core_percent

        # --- Info card (right side) ---
        info_card_w = 200
        info_card_x = 640 - CARD_PAD - info_card_w
        info_card_y = CONTENT_Y + CARD_PAD
        info_card_h = 150

        # Shadow + card
        pygame.draw.rect(surface, theme.shadow,
                         pygame.Rect(info_card_x + 2, info_card_y + 2, info_card_w, info_card_h),
                         border_radius=8)
        pygame.draw.rect(surface, theme.card_bg,
                         pygame.Rect(info_card_x, info_card_y, info_card_w, info_card_h),
                         border_radius=8)
        pygame.draw.rect(surface, theme.card_border,
                         pygame.Rect(info_card_x, info_card_y, info_card_w, info_card_h),
                         width=1, border_radius=8)

        ix = info_card_x + INNER_PAD
        iy = info_card_y + 10
        screen.draw_text("System Info", ix, iy, theme.text_accent, 14, bold=True)
        iy += 24

        # Overall CPU
        screen.draw_text(f"Overall: {s.cpu.overall_percent:.1f}%", ix, iy, theme.text, 13)
        iy += 20

        # Load averages
        la = s.cpu.load_avg
        screen.draw_text("Load Avg:", ix, iy, theme.text_dim, 12)
        iy += 17
        screen.draw_text(f" 1m:  {la[0]:.2f}", ix, iy, theme.text, 12)
        iy += 16
        screen.draw_text(f" 5m:  {la[1]:.2f}", ix, iy, theme.text, 12)
        iy += 16
        screen.draw_text(f"15m:  {la[2]:.2f}", ix, iy, theme.text, 12)
        iy += 20

        # Frequency
        if s.cpu.freq_mhz is not None:
            if s.cpu.freq_mhz >= 1000:
                freq_str = f"{s.cpu.freq_mhz / 1000:.2f} GHz"
            else:
                freq_str = f"{s.cpu.freq_mhz:.0f} MHz"
            screen.draw_text(f"Freq: {freq_str}", ix, iy, theme.text, 12)

        # Process count below info card
        proc_y = info_card_y + info_card_h + 10
        screen.draw_text(f"Processes: {s.cpu.process_count}", info_card_x + INNER_PAD, proc_y, theme.text_dim, 13)

        # Cores label
        screen.draw_text(f"Cores: {s.cpu.core_count}", info_card_x + INNER_PAD, proc_y + 20, theme.text_dim, 13)

        # --- Per-core bars (left side) ---
        bars_x = CARD_PAD
        bars_w = info_card_x - CARD_PAD * 2
        bar_area_y = CONTENT_Y + CARD_PAD
        bar_h = 18
        bar_spacing = 4
        available_h = CONTENT_BOTTOM - bar_area_y - CARD_PAD

        # How many bars can we show?
        visible = min(len(cores), available_h // (bar_h + bar_spacing))
        if visible < 1:
            visible = 1

        # Clamp scroll
        max_scroll = max(0, len(cores) - visible)
        self._scroll = min(self._scroll, max_scroll)

        # Draw card background behind bars
        bars_card_h = visible * (bar_h + bar_spacing) + CARD_PAD * 2 - bar_spacing
        pygame.draw.rect(surface, theme.shadow,
                         pygame.Rect(bars_x + 2, bar_area_y + 2, bars_w, bars_card_h),
                         border_radius=8)
        pygame.draw.rect(surface, theme.card_bg,
                         pygame.Rect(bars_x, bar_area_y, bars_w, bars_card_h),
                         border_radius=8)
        pygame.draw.rect(surface, theme.card_border,
                         pygame.Rect(bars_x, bar_area_y, bars_w, bars_card_h),
                         width=1, border_radius=8)

        # Draw bars
        label_w = 65
        by = bar_area_y + CARD_PAD
        for i in range(self._scroll, min(self._scroll + visible, len(cores))):
            pct = cores[i]
            color = usage_color(pct)
            label = f"Core {i}"
            screen.draw_text(label, bars_x + INNER_PAD, by + 1, theme.text, 12)

            bar_rect = pygame.Rect(bars_x + INNER_PAD + label_w, by, bars_w - INNER_PAD * 2 - label_w - 50, bar_h)
            screen.draw_progress_bar(bar_rect, pct / 100.0, color, theme.bg_lighter, radius=4)

            # Percentage text
            pct_str = f"{pct:.0f}%"
            pw = screen.get_text_width(pct_str, 12, False)
            screen.draw_text(pct_str, bar_rect.right + 8, by + 1, color, 12, bold=True)

            by += bar_h + bar_spacing

        # Scroll indicator
        if max_scroll > 0:
            indicator_x = bars_x + bars_w - 6
            indicator_total_h = bars_card_h - CARD_PAD * 2
            thumb_h = max(10, int(indicator_total_h * visible / len(cores)))
            thumb_y = bar_area_y + CARD_PAD + int(
                (indicator_total_h - thumb_h) * (self._scroll / max_scroll) if max_scroll > 0 else 0
            )
            pygame.draw.rect(surface, theme.border,
                             pygame.Rect(indicator_x, thumb_y, 4, thumb_h),
                             border_radius=2)
