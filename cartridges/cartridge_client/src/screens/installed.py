"""Installed screen: list installed cartridges, launch, remove."""

from __future__ import annotations

from typing import Callable

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.manifest import AppManifest
from cartridge_sdk.screen import Screen, _get_font
from cartridge_sdk.ui import StatusBar
from cartridge_sdk.management import list_installed
from ui_constants import (
    CATEGORY_COLORS, DEFAULT_CATEGORY_COLOR,
    INSTALLED_ROW_HEIGHT, CARD_RADIUS, CARD_MARGIN_X, CARD_WIDTH, CARD_CONTENT_PAD,
)

ROW_HEIGHT = INSTALLED_ROW_HEIGHT


class InstalledScreen:
    """Shows installed cartridges with launch and remove actions."""

    def __init__(
        self,
        on_launch: Callable[[str], None],
        on_detail: Callable[[str], None],
        on_remove: Callable[[str], None],
        on_back: Callable[[], None],
    ) -> None:
        self.on_launch = on_launch
        self.on_detail = on_detail
        self.on_remove = on_remove
        self.on_back = on_back

        self.status_bar = StatusBar("Installed")
        self._apps: list[AppManifest] = []
        self._cursor: int = 0

    def refresh(self) -> None:
        self._apps = list_installed()
        self.status_bar.right_text = f"{len(self._apps)} apps"
        self._cursor = min(self._cursor, max(0, len(self._apps) - 1))

    def handle_input(self, event: InputEvent) -> None:
        if event.action not in ("press", "repeat"):
            return

        n = len(self._apps)

        if event.button == Button.B:
            self.on_back()
        elif event.button == Button.DPAD_UP:
            self._cursor = max(0, self._cursor - 1)
        elif event.button == Button.DPAD_DOWN:
            self._cursor = min(max(0, n - 1), self._cursor + 1)
        elif event.button == Button.L2:
            visible = max(1, 404 // ROW_HEIGHT)
            self._cursor = max(0, self._cursor - visible)
        elif event.button == Button.R2:
            visible = max(1, 404 // ROW_HEIGHT)
            self._cursor = min(max(0, n - 1), self._cursor + visible)
        elif event.button == Button.A and n > 0 and self._cursor < n:
            self.on_launch(self._apps[self._cursor].id)
        elif event.button == Button.X and n > 0 and self._cursor < n:
            self.on_detail(self._apps[self._cursor].id)
        elif event.button == Button.Y and n > 0 and self._cursor < n:
            self.on_remove(self._apps[self._cursor].id)
            self.refresh()

    def update(self, dt: float) -> None:
        self.status_bar.update(dt)

    def draw(self, screen: Screen) -> None:
        screen.clear()
        theme = screen.theme
        s = screen.surface

        # Gradient header
        screen.draw_gradient_rect(
            (0, 0, 640, 40),
            theme.header_gradient_top,
            theme.header_gradient_bottom,
        )
        screen.draw_text("Installed", 12, 8, bold=True, font_size=18)
        if self._apps:
            count_text = f"{len(self._apps)} apps"
            cw = screen.get_text_width(count_text, 12)
            screen.draw_text(count_text, 624 - cw, 14, color=theme.text_dim, font_size=12)
        pygame.draw.line(s, theme.border, (0, 39), (640, 39))

        content_y = 40
        footer_y = 444
        content_h = footer_y - content_y

        if self._apps:
            self._draw_app_list(screen, content_y, content_h)
        else:
            # Empty state in a card panel
            card_w, card_h = 400, 80
            cx = (640 - card_w) // 2
            cy = content_y + (content_h - card_h) // 2
            screen.draw_card((cx, cy, card_w, card_h), bg=theme.card_bg, border=theme.card_border, radius=CARD_RADIUS)
            screen.draw_text(
                "No cartridges installed",
                640 // 2 - screen.get_text_width("No cartridges installed", 14) // 2,
                cy + 16, color=theme.text_dim, font_size=14,
            )
            hint = "Browse the Store to find apps"
            screen.draw_text(
                hint,
                640 // 2 - screen.get_text_width(hint, 12) // 2,
                cy + 44, color=theme.text_dim, font_size=12,
            )

        self._draw_footer(screen)

    def _draw_app_list(self, screen: Screen, y_start: int, height: int) -> None:
        theme = screen.theme
        s = screen.surface

        visible = max(1, height // ROW_HEIGHT)
        n = len(self._apps)
        self._cursor = max(0, min(self._cursor, n - 1))

        if n <= visible:
            start = 0
        else:
            start = max(0, min(self._cursor - 1, n - visible))
        end = min(start + visible, n)

        clip = s.get_clip()
        s.set_clip(pygame.Rect(0, y_start, 640, height))

        y = y_start + 2
        for idx in range(start, end):
            app = self._apps[idx]
            is_selected = idx == self._cursor
            self._draw_app_card(screen, app, y, is_selected)
            y += ROW_HEIGHT

        s.set_clip(clip)

        # Scroll indicator
        if n > visible:
            ind_x = 635
            bar_top = y_start + 4
            bar_h = height - 8
            track_color = (
                min(theme.border[0] + 10, 255),
                min(theme.border[1] + 10, 255),
                min(theme.border[2] + 10, 255),
            )
            pygame.draw.line(s, track_color, (ind_x, bar_top), (ind_x, bar_top + bar_h))
            thumb_h = max(8, bar_h * visible // n)
            progress = self._cursor / max(1, n - 1)
            thumb_y = bar_top + int((bar_h - thumb_h) * progress)
            pygame.draw.rect(s, theme.text_dim, (ind_x - 1, thumb_y, 3, thumb_h), border_radius=1)

    def _draw_app_card(self, screen: Screen, app: AppManifest, y: int, is_selected: bool) -> None:
        theme = screen.theme
        s = screen.surface

        card_x = CARD_MARGIN_X
        card_w = CARD_WIDTH
        card_h = ROW_HEIGHT - 4

        card_rect = pygame.Rect(card_x, y, card_w, card_h)
        if is_selected:
            screen.draw_card(card_rect, bg=theme.card_highlight, border=theme.accent, radius=CARD_RADIUS, shadow=True)
        else:
            screen.draw_card(card_rect, bg=theme.card_bg, border=theme.card_border, radius=CARD_RADIUS, shadow=False)

        # Colored left border strip
        cat_color = DEFAULT_CATEGORY_COLOR
        strip_rect = pygame.Rect(card_x + 1, y + 5, 3, card_h - 10)
        pygame.draw.rect(s, cat_color, strip_rect, border_radius=1)

        text_x = card_x + CARD_CONTENT_PAD + 4

        # App name
        font_name = _get_font("mono_bold", 15)
        name_surf = font_name.render(app.name, True, theme.text)
        s.blit(name_surf, (text_x, y + 8))

        # Version pill
        pill_x = text_x + name_surf.get_width() + 8
        screen.draw_pill(f"v{app.version}", pill_x, y + 10, bg_color=(60, 80, 140), text_color=(200, 210, 240), font_size=9)

        # Description
        font_desc = _get_font("mono", 12)
        desc = app.description
        max_w = card_w - 40
        tw = font_desc.size(desc)[0]
        if tw > max_w:
            while len(desc) > 1 and font_desc.size(desc + "..")[0] > max_w:
                desc = desc[:-1]
            desc += ".."
        desc_surf = font_desc.render(desc, True, theme.text_dim)
        s.blit(desc_surf, (text_x, y + 8 + font_name.get_linesize()))

    def _draw_footer(self, screen: Screen) -> None:
        theme = screen.theme
        y = 444
        h = 36
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, h))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        hx += screen.draw_button_hint("A", "Launch", hx, y + 8, btn_color=theme.btn_a) + 14
        hx += screen.draw_button_hint("X", "Details", hx, y + 8, btn_color=theme.btn_x) + 14
        hx += screen.draw_button_hint("Y", "Remove", hx, y + 8, btn_color=theme.btn_y) + 14
        hx += screen.draw_button_hint("B", "Back", hx, y + 8, btn_color=theme.btn_b) + 14
