"""Network screen -- per-interface stats, rates, connections."""

from __future__ import annotations

from typing import Optional

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font

from ..stats import SystemStats, format_bytes, format_rate

# Layout
CONTENT_Y = 72
CONTENT_BOTTOM = 444
CARD_PAD = 10
INNER_PAD = 12


class NetworkScreen:
    """Per-interface network statistics and connection count."""

    def __init__(self) -> None:
        self.stats: Optional[SystemStats] = None
        self._scroll = 0

    def update_stats(self, stats: SystemStats) -> None:
        self.stats = stats

    def handle_input(self, event: InputEvent) -> bool:
        if self.stats is None:
            return False
        if event.button == Button.UP and event.pressed:
            self._scroll = max(0, self._scroll - 1)
            return True
        if event.button == Button.DOWN and event.pressed:
            iface_count = len(self.stats.network.interfaces)
            max_scroll = max(0, iface_count - 4)
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
        net = s.network

        # ---- Summary Card ----
        summary_x = CARD_PAD
        summary_y = CONTENT_Y + CARD_PAD
        summary_w = 640 - CARD_PAD * 2
        summary_h = 80

        self._draw_card(surface, theme, summary_x, summary_y, summary_w, summary_h)

        ix = summary_x + INNER_PAD
        iy = summary_y + 10
        screen.draw_text("Network Overview", ix, iy, theme.text_accent, 15, bold=True)

        # Connection count on right
        if net.connection_count >= 0:
            conn_txt = f"{net.connection_count} connections"
        else:
            conn_txt = "Connections: N/A"
        cw = screen.get_text_width(conn_txt, 12, False)
        screen.draw_text(conn_txt, summary_x + summary_w - INNER_PAD - cw, iy + 2, theme.text_dim, 12)
        iy += 30

        # Total rates
        up_color = theme.positive
        dn_color = (100, 180, 255)

        screen.draw_text("\u2191", ix, iy, up_color, 16, bold=True)
        screen.draw_text(format_rate(net.send_rate), ix + 18, iy, theme.text, 14)

        mid_x = summary_x + summary_w // 2
        screen.draw_text("\u2193", mid_x, iy, dn_color, 16, bold=True)
        screen.draw_text(format_rate(net.recv_rate), mid_x + 18, iy, theme.text, 14)

        # Totals
        total_txt = f"Total: \u2191 {format_bytes(net.total_sent)}  \u2193 {format_bytes(net.total_recv)}"
        tw = screen.get_text_width(total_txt, 12, False)
        screen.draw_text(total_txt, summary_x + summary_w - INNER_PAD - tw, iy + 2, theme.text_dim, 12)

        # ---- Interface Cards ----
        ifaces = net.interfaces
        if not ifaces:
            ny = summary_y + summary_h + CARD_PAD + 20
            screen.draw_text("No network interfaces detected", CARD_PAD + INNER_PAD, ny, theme.text_dim, 14)
            return

        iface_area_y = summary_y + summary_h + CARD_PAD
        available_h = CONTENT_BOTTOM - iface_area_y - CARD_PAD
        card_h = 80
        card_spacing = 8
        visible = max(1, available_h // (card_h + card_spacing))

        # Clamp scroll
        max_scroll = max(0, len(ifaces) - visible)
        self._scroll = min(self._scroll, max_scroll)

        # Sort: put active interfaces first (those with traffic)
        ifaces_sorted = sorted(ifaces, key=lambda i: i.bytes_sent + i.bytes_recv, reverse=True)

        cy = iface_area_y
        for idx in range(self._scroll, min(self._scroll + visible, len(ifaces_sorted))):
            iface = ifaces_sorted[idx]
            self._draw_iface_card(screen, theme, surface, CARD_PAD, cy, summary_w, card_h, iface)
            cy += card_h + card_spacing

        # Scroll indicator
        if max_scroll > 0:
            ind_x = summary_x + summary_w - 4
            ind_total_h = visible * (card_h + card_spacing) - card_spacing
            thumb_h = max(10, int(ind_total_h * visible / len(ifaces)))
            if max_scroll > 0:
                thumb_y = iface_area_y + int((ind_total_h - thumb_h) * (self._scroll / max_scroll))
            else:
                thumb_y = iface_area_y
            pygame.draw.rect(surface, theme.border,
                             pygame.Rect(ind_x, thumb_y, 4, thumb_h),
                             border_radius=2)

    def _draw_iface_card(self, screen: Screen, theme, surface, x: int, y: int, w: int, h: int, iface) -> None:
        """Draw a single interface card."""
        self._draw_card(surface, theme, x, y, w, h)

        ix = x + INNER_PAD
        iy = y + 10

        # Interface name
        screen.draw_text(iface.name, ix, iy, theme.text, 14, bold=True)

        # Activity indicator
        is_active = (iface.send_rate + iface.recv_rate) > 0
        indicator_color = theme.positive if is_active else theme.text_dim
        pygame.draw.circle(surface, indicator_color, (ix + screen.get_text_width(iface.name, 14, True) + 14, iy + 7), 4)

        iy += 24

        # Rates
        up_color = theme.positive
        dn_color = (100, 180, 255)

        screen.draw_text("\u2191", ix, iy, up_color, 14, bold=True)
        screen.draw_text(format_rate(iface.send_rate), ix + 16, iy, theme.text, 13)

        rate_col2 = ix + 160
        screen.draw_text("\u2193", rate_col2, iy, dn_color, 14, bold=True)
        screen.draw_text(format_rate(iface.recv_rate), rate_col2 + 16, iy, theme.text, 13)

        iy += 22

        # Totals
        screen.draw_text(f"Sent: {format_bytes(iface.bytes_sent)}", ix, iy, theme.text_dim, 12)
        screen.draw_text(f"Recv: {format_bytes(iface.bytes_recv)}", rate_col2, iy, theme.text_dim, 12)

    @staticmethod
    def _draw_card(surface, theme, x: int, y: int, w: int, h: int) -> None:
        pygame.draw.rect(surface, theme.shadow,
                         pygame.Rect(x + 2, y + 2, w, h), border_radius=8)
        pygame.draw.rect(surface, theme.card_bg,
                         pygame.Rect(x, y, w, h), border_radius=8)
        pygame.draw.rect(surface, theme.card_border,
                         pygame.Rect(x, y, w, h), width=1, border_radius=8)
