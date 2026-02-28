"""Chart widgets: SparkLine and LineChart."""

from __future__ import annotations

from typing import TYPE_CHECKING

import pygame

from cartridge_sdk.input import InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui.widget import Widget

if TYPE_CHECKING:
    from cartridge_sdk.theme import Theme


class SparkLine(Widget):
    """Mini inline line chart for embedding in rows."""

    def __init__(self, data: list[float] | None = None, color: tuple | None = None) -> None:
        self.data: list[float] = data or []
        self.color = color

    def draw(self, surface: pygame.Surface, rect: pygame.Rect, theme: Theme) -> None:
        data = self.data
        if len(data) < 2:
            return

        color = self.color or theme.accent
        mn = min(data)
        mx = max(data)
        rng = mx - mn if mx != mn else 1.0

        points = []
        for i, v in enumerate(data):
            px = rect.x + int(i / (len(data) - 1) * (rect.width - 1))
            py = rect.y + rect.height - 1 - int(((v - mn) / rng) * (rect.height - 1))
            points.append((px, py))

        if len(points) >= 2:
            pygame.draw.lines(surface, color, False, points, 2)


class LineChart(Widget):
    """Full chart with axis labels and grid lines."""

    def __init__(
        self,
        data: list[float] | None = None,
        labels: list[str] | None = None,
        color: tuple | None = None,
        title: str = "",
    ) -> None:
        self.data: list[float] = data or []
        self.labels: list[str] = labels or []
        self.color = color
        self.title = title

    def draw(self, surface: pygame.Surface, rect: pygame.Rect, theme: Theme) -> None:
        data = self.data
        if len(data) < 2:
            font = _get_font("mono", 13)
            msg = font.render("No chart data", True, theme.text_dim)
            surface.blit(msg, (rect.x + rect.width // 2 - msg.get_width() // 2,
                               rect.y + rect.height // 2 - msg.get_height() // 2))
            return

        color = self.color or theme.accent
        font_label = _get_font("mono", 11)
        font_title = _get_font("mono_bold", 13)

        # Margins
        left_margin = 60
        right_margin = 12
        top_margin = 8
        bottom_margin = 24

        if self.title:
            title_surf = font_title.render(self.title, True, theme.text_dim)
            surface.blit(title_surf, (rect.x + left_margin, rect.y + 2))
            top_margin += font_title.get_linesize()

        chart_x = rect.x + left_margin
        chart_y = rect.y + top_margin
        chart_w = rect.width - left_margin - right_margin
        chart_h = rect.height - top_margin - bottom_margin

        if chart_w <= 0 or chart_h <= 0:
            return

        mn = min(data)
        mx = max(data)
        rng = mx - mn if mx != mn else 1.0

        # Grid lines and Y labels (5 lines)
        grid_color = (
            min(theme.border[0] + 8, 255),
            min(theme.border[1] + 8, 255),
            min(theme.border[2] + 8, 255),
        )
        num_grid = 4
        for i in range(num_grid + 1):
            gy = chart_y + int(i / num_grid * chart_h)
            pygame.draw.line(surface, grid_color, (chart_x, gy), (chart_x + chart_w, gy))

            val = mx - (i / num_grid) * rng
            if abs(val) >= 1000:
                label_text = f"${val:,.0f}"
            elif abs(val) >= 1:
                label_text = f"${val:.2f}"
            else:
                label_text = f"${val:.4f}"
            label_surf = font_label.render(label_text, True, theme.text_dim)
            surface.blit(label_surf, (rect.x + 2, gy - label_surf.get_height() // 2))

        # Chart line
        points = []
        for i, v in enumerate(data):
            px = chart_x + int(i / (len(data) - 1) * (chart_w - 1))
            py = chart_y + chart_h - 1 - int(((v - mn) / rng) * (chart_h - 1))
            points.append((px, py))

        # Fill area under the line (subtle)
        if len(points) >= 2:
            fill_points = list(points) + [
                (points[-1][0], chart_y + chart_h),
                (points[0][0], chart_y + chart_h),
            ]
            fill_color = (color[0], color[1], color[2], 25)
            fill_surface = pygame.Surface((chart_w, chart_h), pygame.SRCALPHA)
            shifted_points = [(p[0] - chart_x, p[1] - chart_y) for p in fill_points]
            if len(shifted_points) >= 3:
                pygame.draw.polygon(fill_surface, fill_color, shifted_points)
                surface.blit(fill_surface, (chart_x, chart_y))

            pygame.draw.lines(surface, color, False, points, 2)

            # End dot
            last = points[-1]
            pygame.draw.circle(surface, color, last, 3)

        # X-axis labels
        if self.labels:
            n_labels = min(5, len(self.labels))
            step = max(1, len(self.labels) // n_labels)
            for i in range(0, len(self.labels), step):
                lx = chart_x + int(i / max(1, len(self.labels) - 1) * (chart_w - 1))
                label_surf = font_label.render(self.labels[i], True, theme.text_dim)
                surface.blit(label_surf, (lx - label_surf.get_width() // 2,
                                          chart_y + chart_h + 4))

        # Border
        pygame.draw.rect(surface, grid_color, (chart_x, chart_y, chart_w, chart_h), 1)
