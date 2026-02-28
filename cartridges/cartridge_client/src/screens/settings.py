"""Settings screen: registry URL, auto-refresh, cache, about."""

from __future__ import annotations

from typing import Callable

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font
from cartridge_sdk.storage import AppStorage
from ui_constants import CARD_RADIUS, CARD_MARGIN_X, CARD_WIDTH

DEFAULT_REGISTRY = "https://raw.githubusercontent.com/Strizzo/cartridge/main/registry.json"

ROW_HEIGHT = 60


class SettingsScreen:
    """Settings with card-style rows."""

    def __init__(
        self,
        storage: AppStorage,
        on_back: Callable[[], None],
        on_registry_changed: Callable[[str], None],
    ) -> None:
        self.storage = storage
        self.on_back = on_back
        self.on_registry_changed = on_registry_changed
        self._cursor: int = 0

        # Load saved settings
        saved = self.storage.load("settings") or {}

        self._items: list[dict] = [
            {
                "key": "registry_url",
                "label": "Registry URL",
                "type": "choice",
                "value": saved.get("registry_url", DEFAULT_REGISTRY),
                "choices": [
                    (DEFAULT_REGISTRY, "Default"),
                ],
            },
            {
                "key": "auto_refresh",
                "label": "Auto Refresh",
                "type": "toggle",
                "value": saved.get("auto_refresh", True),
            },
            {
                "key": "cache_ttl",
                "label": "Cache Duration",
                "type": "choice",
                "value": saved.get("cache_ttl", 600),
                "choices": [
                    (300, "5 min"),
                    (600, "10 min"),
                    (1800, "30 min"),
                    (3600, "1 hr"),
                ],
            },
            {
                "key": "about",
                "label": "About Cartridge",
                "type": "info",
                "value": "v0.1.0",
            },
        ]

    def get_setting(self, key: str) -> object:
        for item in self._items:
            if item["key"] == key:
                return item["value"]
        return None

    def _save(self) -> None:
        data = {}
        for item in self._items:
            if item["type"] != "info":
                data[item["key"]] = item["value"]
        self.storage.save("settings", data)

    def handle_input(self, event: InputEvent) -> None:
        if event.action not in ("press", "repeat"):
            return

        n = len(self._items)

        if event.button == Button.B:
            self._save()
            self.on_back()
        elif event.button == Button.DPAD_UP:
            self._cursor = max(0, self._cursor - 1)
        elif event.button == Button.DPAD_DOWN:
            self._cursor = min(n - 1, self._cursor + 1)
        elif event.button == Button.A:
            self._activate_item()
        elif event.button in (Button.DPAD_LEFT, Button.DPAD_RIGHT):
            self._cycle_item(1 if event.button == Button.DPAD_RIGHT else -1)

    def _activate_item(self) -> None:
        item = self._items[self._cursor]
        if item["type"] == "toggle":
            item["value"] = not item["value"]
            self._save()
        elif item["type"] == "choice":
            self._cycle_item(1)

    def _cycle_item(self, direction: int) -> None:
        item = self._items[self._cursor]
        if item["type"] != "choice":
            return
        choices = item["choices"]
        current = item["value"]
        idx = next((i for i, (v, _) in enumerate(choices) if v == current), 0)
        idx = (idx + direction) % len(choices)
        item["value"] = choices[idx][0]
        self._save()

        if item["key"] == "registry_url":
            self.on_registry_changed(item["value"])

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
        screen.draw_text("Settings", 12, 8, bold=True, font_size=18)
        pygame.draw.line(s, theme.border, (0, 39), (640, 39))

        # Settings rows
        y = 48
        for i, item in enumerate(self._items):
            is_selected = i == self._cursor
            self._draw_setting_row(screen, item, y, is_selected)
            y += ROW_HEIGHT

        # Footer
        self._draw_footer(screen)

    def _draw_setting_row(self, screen: Screen, item: dict, y: int, is_selected: bool) -> None:
        theme = screen.theme
        s = screen.surface

        card_x = CARD_MARGIN_X
        card_w = CARD_WIDTH
        card_h = ROW_HEIGHT - 6

        card_rect = pygame.Rect(card_x, y, card_w, card_h)
        if is_selected:
            screen.draw_card(card_rect, bg=theme.card_highlight, border=theme.accent, radius=CARD_RADIUS, shadow=True)
        else:
            screen.draw_card(card_rect, bg=theme.card_bg, border=theme.card_border, radius=CARD_RADIUS, shadow=False)

        # Label
        screen.draw_text(item["label"], card_x + 16, y + 10, bold=True, font_size=14)

        item_type = item["type"]
        value = item["value"]

        if item_type == "toggle":
            pill_text = "ON" if value else "OFF"
            pill_color = theme.positive if value else theme.negative
            pill_tc = (20, 20, 30)
            screen.draw_pill(pill_text, card_x + card_w - 60, y + 14, bg_color=pill_color, text_color=pill_tc, font_size=11)

        elif item_type == "choice":
            choices = item["choices"]
            current_label = next((label for v, label in choices if v == value), str(value))
            display = f"< {current_label} >"
            dw = screen.get_text_width(display, 13)
            screen.draw_text(display, card_x + card_w - dw - 16, y + 16, color=theme.text_accent, font_size=13)

            # Show full URL below label for registry_url
            if item["key"] == "registry_url" and isinstance(value, str):
                screen.draw_text(value, card_x + 16, y + 30, color=theme.text_dim, font_size=10, max_width=card_w - 32)

        elif item_type == "info":
            screen.draw_text(str(value), card_x + card_w - screen.get_text_width(str(value), 13) - 16, y + 16, color=theme.text_dim, font_size=13)

    def _draw_footer(self, screen: Screen) -> None:
        theme = screen.theme
        y = 444
        h = 36
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, h))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        hx += screen.draw_button_hint("A", "Toggle", hx, y + 8, btn_color=theme.btn_a) + 14
        hx += screen.draw_button_hint("B", "Back", hx, y + 8, btn_color=theme.btn_b) + 14
