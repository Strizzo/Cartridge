"""Store screen: browse catalog by category."""

from __future__ import annotations

import asyncio
from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font
from cartridge_sdk.ui import StatusBar, LoadingIndicator
from cartridge_sdk.management import is_installed
from ui_constants import (
    CATEGORY_COLORS, DEFAULT_CATEGORY_COLOR, STATUS_INSTALLED,
    STORE_ROW_HEIGHT, CARD_RADIUS, CARD_MARGIN_X, CARD_WIDTH, CARD_CONTENT_PAD,
)

if TYPE_CHECKING:
    from registry import RegistryApp, Registry, RegistryClient

ROW_HEIGHT = STORE_ROW_HEIGHT


class StoreScreen:
    """Browse the app catalog with category filtering."""

    def __init__(
        self,
        registry_client: RegistryClient,
        on_app_select: Callable[[RegistryApp], None],
        on_installed: Callable[[], None],
    ) -> None:
        self.registry_client = registry_client
        self.on_app_select = on_app_select
        self.on_installed = on_installed

        self.status_bar = StatusBar("Cartridge")
        self.loading = LoadingIndicator("Loading catalog")

        self._registry: Registry | None = None
        self._categories: list[str] = []
        self._cat_idx: int = 0  # 0 = "All"
        self._cursor: int = 0
        self._loading = False

    @property
    def _current_apps(self) -> list[RegistryApp]:
        if not self._registry:
            return []
        if self._cat_idx == 0:
            return self._registry.apps
        cat = self._categories[self._cat_idx - 1]
        return self._registry.filter_by_category(cat)

    async def load(self) -> None:
        self._loading = True
        self.loading.visible = True
        try:
            self._registry = await self.registry_client.fetch()
            self._categories = self._registry.get_categories()
            self.status_bar.right_text = f"{len(self._registry.apps)} apps"
            self.status_bar.right_color = (100, 220, 100)
        except Exception:
            self.status_bar.right_text = "Error"
            self.status_bar.right_color = (255, 100, 100)
        finally:
            self._loading = False
            self.loading.visible = False

    def handle_input(self, event: InputEvent) -> None:
        if event.action not in ("press", "repeat"):
            return

        apps = self._current_apps
        total_cats = 1 + len(self._categories)  # "All" + real categories

        if event.button == Button.L1:
            self._cat_idx = (self._cat_idx - 1) % total_cats
            self._cursor = 0
        elif event.button == Button.R1:
            self._cat_idx = (self._cat_idx + 1) % total_cats
            self._cursor = 0
        elif event.button == Button.DPAD_UP:
            self._cursor = max(0, self._cursor - 1)
        elif event.button == Button.DPAD_DOWN:
            self._cursor = min(max(0, len(apps) - 1), self._cursor + 1)
        elif event.button == Button.L2:
            visible = max(1, 370 // ROW_HEIGHT)
            self._cursor = max(0, self._cursor - visible)
        elif event.button == Button.R2:
            visible = max(1, 370 // ROW_HEIGHT)
            self._cursor = min(max(0, len(apps) - 1), self._cursor + visible)
        elif event.button == Button.A and apps:
            if self._cursor < len(apps):
                self.on_app_select(apps[self._cursor])
        elif event.button == Button.Y:
            self.on_installed()
        elif event.button == Button.X:
            asyncio.create_task(self.load())

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
        # Title
        screen.draw_text("Cartridge", 12, 8, bold=True, font_size=18)
        # App count (right side)
        if self._registry:
            count_text = f"{len(self._registry.apps)} apps"
            cw = screen.get_text_width(count_text, 12)
            screen.draw_text(count_text, 624 - cw, 14, color=theme.text_dim, font_size=12)
        # WiFi indicator
        self._draw_wifi(screen)
        # Header bottom border
        pygame.draw.line(s, theme.border, (0, 39), (640, 39))

        # Category tabs (y=40, h=30)
        self._draw_category_tabs(screen, 40)
        content_y = 70

        footer_y = 444
        content_h = footer_y - content_y

        apps = self._current_apps
        if apps:
            self._draw_app_list(screen, apps, content_y, content_h)

        # Loading overlay
        self.loading.draw(s, pygame.Rect(0, content_y, 640, content_h), theme)

        # Footer
        self._draw_footer(screen)

    def _draw_wifi(self, screen: Screen) -> None:
        """Draw WiFi indicator in the header area."""
        theme = screen.theme
        s = screen.surface
        wifi = self.status_bar._wifi_status

        wifi_font = _get_font("mono", 11)
        cy = 20  # vertical center of header

        # Position: to the left of the app count
        base_x = 500
        if self._registry:
            count_text = f"{len(self._registry.apps)} apps"
            cw = screen.get_text_width(count_text, 12)
            base_x = 624 - cw - 14

        if wifi.connected:
            if wifi.signal_strength > 50:
                dot_color = theme.positive
            elif wifi.signal_strength > 20:
                dot_color = theme.text_warning
            else:
                dot_color = theme.negative
            wifi_label = wifi_font.render("WiFi", True, theme.text_dim)
            wifi_w = 12 + wifi_label.get_width()
            wx = base_x - wifi_w
            pygame.draw.circle(s, dot_color, (wx + 4, cy), 3)
            s.blit(wifi_label, (wx + 12, cy - wifi_label.get_height() // 2))
        else:
            wifi_label = wifi_font.render("No WiFi", True, theme.text_dim)
            wx = base_x - wifi_label.get_width()
            s.blit(wifi_label, (wx, cy - wifi_label.get_height() // 2))

    def _draw_category_tabs(self, screen: Screen, y: int) -> None:
        theme = screen.theme
        s = screen.surface

        pygame.draw.rect(s, theme.bg_header, (0, y, 640, 30))

        labels = ["All"] + [c.capitalize() for c in self._categories]
        x = 8
        for i, label in enumerate(labels):
            is_active = i == self._cat_idx
            font = _get_font("mono_bold" if is_active else "mono", 12)
            tw = font.size(label)[0]
            tab_w = tw + 16

            if is_active:
                # Use category-specific color for active tab
                if i == 0:
                    tab_color = theme.accent
                else:
                    cat = self._categories[i - 1]
                    tab_color = CATEGORY_COLORS.get(cat, DEFAULT_CATEGORY_COLOR)
                pygame.draw.rect(s, tab_color, (x, y + 4, tab_w, 22), border_radius=4)
                rendered = font.render(label, True, (20, 20, 30))
            else:
                pygame.draw.rect(s, theme.card_bg, (x, y + 4, tab_w, 22), border_radius=4)
                rendered = font.render(label, True, theme.text_dim)

            s.blit(rendered, (x + 8, y + 7))
            x += tab_w + 4

        pygame.draw.line(s, theme.border, (0, y + 29), (640, y + 29))

    def _draw_app_list(self, screen: Screen, apps: list, y_start: int, height: int) -> None:
        theme = screen.theme
        s = screen.surface

        visible = max(1, height // ROW_HEIGHT)
        n = len(apps)
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
            app = apps[idx]
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

    def _draw_app_card(self, screen: Screen, app: RegistryApp, y: int, is_selected: bool) -> None:
        theme = screen.theme
        s = screen.surface

        card_x = CARD_MARGIN_X
        card_w = CARD_WIDTH
        card_h = ROW_HEIGHT - 4
        card_rect = pygame.Rect(card_x, y, card_w, card_h)

        # Card background with shadow for selected
        if is_selected:
            screen.draw_card(card_rect, bg=theme.card_highlight, border=theme.accent, radius=CARD_RADIUS, shadow=True)
        else:
            screen.draw_card(card_rect, bg=theme.card_bg, border=theme.card_border, radius=CARD_RADIUS, shadow=False)

        # Colored left border strip (category accent)
        cat_color = CATEGORY_COLORS.get(app.category, DEFAULT_CATEGORY_COLOR)
        strip_rect = pygame.Rect(card_x + 1, y + 6, 3, card_h - 12)
        pygame.draw.rect(s, cat_color, strip_rect, border_radius=1)

        # App name (bold)
        text_x = card_x + CARD_CONTENT_PAD + 4
        font_name = _get_font("mono_bold", 15)
        name_surf = font_name.render(app.name, True, theme.text)
        s.blit(name_surf, (text_x, y + 8))

        # INSTALLED pill (right of name)
        installed = is_installed(app.id)
        if installed:
            pill_x = text_x + name_surf.get_width() + 8
            screen.draw_pill("INSTALLED", pill_x, y + 10, bg_color=STATUS_INSTALLED, text_color=(20, 20, 30), font_size=9)

        # Description (dim, truncated)
        font_desc = _get_font("mono", 12)
        desc = app.description
        max_desc_w = card_w - 40
        tw = font_desc.size(desc)[0]
        if tw > max_desc_w:
            while len(desc) > 1 and font_desc.size(desc + "..")[0] > max_desc_w:
                desc = desc[:-1]
            desc += ".."
        desc_surf = font_desc.render(desc, True, theme.text_dim)
        desc_y = y + 8 + font_name.get_linesize()
        s.blit(desc_surf, (text_x, desc_y))

        # Bottom row: author + version
        font_meta = _get_font("mono", 11)
        meta = f"{app.author}  v{app.version}"
        meta_surf = font_meta.render(meta, True, theme.text_dim)
        meta_y = desc_y + font_desc.get_linesize()
        s.blit(meta_surf, (text_x, meta_y))

        # Category pill (right side)
        cat_label = app.category.upper()
        cat_pill_x = card_x + card_w - 12
        cat_font = _get_font("mono_bold", 9)
        cat_tw = cat_font.size(cat_label)[0] + 12
        cat_pill_x -= cat_tw
        screen.draw_pill(cat_label, cat_pill_x, y + card_h // 2 - 8, bg_color=cat_color, text_color=(20, 20, 30), font_size=9)

    def _draw_footer(self, screen: Screen) -> None:
        theme = screen.theme
        y = 444
        h = 36
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, h))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        hx += screen.draw_button_hint("L1/R1", "Category", hx, y + 8, btn_color=theme.btn_l) + 14
        hx += screen.draw_button_hint("A", "Details", hx, y + 8, btn_color=theme.btn_a) + 14
        hx += screen.draw_button_hint("Y", "Installed", hx, y + 8, btn_color=theme.btn_y) + 14
        hx += screen.draw_button_hint("X", "Refresh", hx, y + 8, btn_color=theme.btn_x) + 14
