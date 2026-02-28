"""App detail screen: card-based info with install/launch/remove."""

from __future__ import annotations

import asyncio
from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font
from cartridge_sdk.ui import StatusBar, LoadingIndicator
from cartridge_sdk.management import is_installed, install_from_github, remove_cartridge
from ui_constants import (
    CATEGORY_COLORS, DEFAULT_CATEGORY_COLOR, STATUS_INSTALLED,
    CARD_RADIUS, CARD_MARGIN_X, CARD_WIDTH,
)

if TYPE_CHECKING:
    from registry import RegistryApp

SCROLL_STEP = 24


class DetailScreen:
    """Scrollable app detail with card-based sections."""

    def __init__(
        self,
        on_launch: Callable[[str], None],
        on_back: Callable[[], None],
    ) -> None:
        self.on_launch = on_launch
        self.on_back = on_back

        self.status_bar = StatusBar("")
        self.loading = LoadingIndicator("Installing")
        self._app: RegistryApp | None = None
        self._scroll: int = 0
        self._content_height: int = 0
        self._installing = False

    def set_app(self, app: RegistryApp) -> None:
        self._app = app
        self._scroll = 0
        self._installing = False
        self.status_bar.title = app.name
        self.loading.visible = False

    def _is_installed(self) -> bool:
        return self._app is not None and is_installed(self._app.id)

    def handle_input(self, event: InputEvent) -> None:
        if event.action not in ("press", "repeat"):
            return
        if self._installing:
            return

        if event.button == Button.B:
            self.on_back()
        elif event.button == Button.DPAD_UP:
            self._scroll = max(0, self._scroll - SCROLL_STEP)
        elif event.button == Button.DPAD_DOWN:
            max_scroll = max(0, self._content_height - 360)
            self._scroll = min(max_scroll, self._scroll + SCROLL_STEP)
        elif event.button == Button.L2:
            self._scroll = max(0, self._scroll - SCROLL_STEP * 8)
        elif event.button == Button.R2:
            max_scroll = max(0, self._content_height - 360)
            self._scroll = min(max_scroll, self._scroll + SCROLL_STEP * 8)
        elif event.button == Button.A and self._app:
            if self._is_installed():
                self.on_launch(self._app.id)
            else:
                asyncio.create_task(self._install())
        elif event.button == Button.Y and self._app and self._is_installed():
            remove_cartridge(self._app.id)

    async def _install(self) -> None:
        if not self._app or not self._app.repo_url:
            return
        self._installing = True
        self.loading.text = "Installing"
        self.loading.visible = True
        try:
            await asyncio.to_thread(install_from_github, self._app.repo_url)
        except Exception:
            pass
        finally:
            self._installing = False
            self.loading.visible = False

    def update(self, dt: float) -> None:
        self.status_bar.update(dt)

    def draw(self, screen: Screen) -> None:
        screen.clear()
        theme = screen.theme
        s = screen.surface

        if not self._app:
            return

        app = self._app
        installed = self._is_installed()
        cat_color = CATEGORY_COLORS.get(app.category, DEFAULT_CATEGORY_COLOR)

        # Gradient header
        screen.draw_gradient_rect(
            (0, 0, 640, 40),
            theme.header_gradient_top,
            theme.header_gradient_bottom,
        )
        screen.draw_text(app.name, 12, 8, bold=True, font_size=18)
        pygame.draw.line(s, theme.border, (0, 39), (640, 39))

        # Content area
        content_y = 44
        footer_y = 444
        content_h = footer_y - content_y

        clip = s.get_clip()
        s.set_clip(pygame.Rect(0, content_y, 640, content_h))

        y = content_y + 4 - self._scroll

        # ── Header card ──────────────────────────────────────────────
        header_h = 72
        header_rect = pygame.Rect(CARD_MARGIN_X, y, CARD_WIDTH, header_h)
        screen.draw_card(header_rect, bg=theme.card_bg, border=cat_color, radius=CARD_RADIUS)

        # Colored top accent line
        pygame.draw.rect(s, cat_color, (CARD_MARGIN_X + 4, y, CARD_WIDTH - 8, 3), border_radius=1)

        # Status pill
        status_text = "INSTALLED" if installed else "NOT INSTALLED"
        status_color = STATUS_INSTALLED if installed else (80, 80, 100)
        status_text_color = (20, 20, 30) if installed else (160, 160, 180)
        screen.draw_pill(status_text, CARD_MARGIN_X + 14, y + 12, bg_color=status_color, text_color=status_text_color, font_size=10)

        # Metadata line: author, version, category
        meta_y = y + 36
        mx = CARD_MARGIN_X + 14
        meta_font = _get_font("mono", 12)

        author_surf = meta_font.render(app.author, True, theme.text_dim)
        s.blit(author_surf, (mx, meta_y))
        mx += author_surf.get_width() + 10

        dot_surf = meta_font.render("\u00B7", True, theme.text_dim)
        s.blit(dot_surf, (mx, meta_y))
        mx += dot_surf.get_width() + 10

        ver_surf = meta_font.render(f"v{app.version}", True, theme.text_dim)
        s.blit(ver_surf, (mx, meta_y))
        mx += ver_surf.get_width() + 10

        s.blit(dot_surf, (mx, meta_y))
        mx += dot_surf.get_width() + 10

        # Category pill
        screen.draw_pill(app.category.upper(), mx, meta_y - 1, bg_color=cat_color, text_color=(20, 20, 30), font_size=9)

        y += header_h + 10

        # ── Description card ─────────────────────────────────────────
        if app.description:
            # Word-wrap description
            desc_lines = self._wrap_text(app.description, 70)
            desc_line_h = 18
            desc_card_h = 30 + len(desc_lines) * desc_line_h + 10

            desc_rect = pygame.Rect(CARD_MARGIN_X, y, CARD_WIDTH, desc_card_h)
            screen.draw_card(desc_rect, bg=theme.card_bg, border=theme.card_border, radius=CARD_RADIUS)

            screen.draw_text("Description", CARD_MARGIN_X + 14, y + 10, color=theme.text_accent, bold=True, font_size=13)
            dy = y + 30
            for line in desc_lines:
                screen.draw_text(line, CARD_MARGIN_X + 14, dy, color=theme.text, font_size=13)
                dy += desc_line_h

            y += desc_card_h + 8

        # ── Tags card ────────────────────────────────────────────────
        if app.tags:
            tags_card_h = 56
            tags_rect = pygame.Rect(CARD_MARGIN_X, y, CARD_WIDTH, tags_card_h)
            screen.draw_card(tags_rect, bg=theme.card_bg, border=theme.card_border, radius=CARD_RADIUS)

            screen.draw_text("Tags", CARD_MARGIN_X + 14, y + 10, color=theme.text_accent, bold=True, font_size=13)
            tx = CARD_MARGIN_X + 14
            for tag in app.tags:
                pw = screen.draw_pill(tag, tx, y + 32, bg_color=(50, 60, 90), text_color=(180, 190, 220), font_size=10)
                tx += pw + 6

            y += tags_card_h + 8

        # ── Permissions card ─────────────────────────────────────────
        if app.permissions:
            perm_line_h = 22
            perm_card_h = 30 + len(app.permissions) * perm_line_h + 6

            perm_rect = pygame.Rect(CARD_MARGIN_X, y, CARD_WIDTH, perm_card_h)
            screen.draw_card(perm_rect, bg=theme.card_bg, border=theme.card_border, radius=CARD_RADIUS)

            screen.draw_text("Permissions", CARD_MARGIN_X + 14, y + 10, color=theme.text_accent, bold=True, font_size=13)
            py_ = y + 32
            for perm in app.permissions:
                screen.draw_pill(perm, CARD_MARGIN_X + 14, py_, bg_color=(60, 50, 80), text_color=(190, 170, 230), font_size=10)
                py_ += perm_line_h

            y += perm_card_h + 8

        # ── Repository card ──────────────────────────────────────────
        if app.repo_url:
            repo_card_h = 52
            repo_rect = pygame.Rect(CARD_MARGIN_X, y, CARD_WIDTH, repo_card_h)
            screen.draw_card(repo_rect, bg=theme.card_bg, border=theme.card_border, radius=CARD_RADIUS)

            screen.draw_text("Repository", CARD_MARGIN_X + 14, y + 8, color=theme.text_accent, bold=True, font_size=13)
            screen.draw_text(app.repo_url, CARD_MARGIN_X + 14, y + 28, color=theme.text_dim, font_size=11, max_width=CARD_WIDTH - 28)

            y += repo_card_h + 8

        # Track total content height for scrolling
        self._content_height = (y + self._scroll) - content_y

        s.set_clip(clip)

        # Scroll indicator
        if self._content_height > content_h:
            ind_x = 635
            bar_top = content_y + 4
            bar_h = content_h - 8
            max_scroll = max(1, self._content_height - content_h)
            thumb_h = max(8, bar_h * content_h // self._content_height)
            progress = self._scroll / max_scroll
            thumb_y = bar_top + int((bar_h - thumb_h) * progress)
            track_color = (
                min(theme.border[0] + 10, 255),
                min(theme.border[1] + 10, 255),
                min(theme.border[2] + 10, 255),
            )
            pygame.draw.line(s, track_color, (ind_x, bar_top), (ind_x, bar_top + bar_h))
            pygame.draw.rect(s, theme.text_dim, (ind_x - 1, thumb_y, 3, thumb_h), border_radius=1)

        # Loading overlay
        self.loading.draw(s, pygame.Rect(0, content_y, 640, content_h), theme)

        # Footer
        self._draw_footer(screen, installed)

    def _wrap_text(self, text: str, max_chars: int) -> list[str]:
        """Word-wrap text at max_chars per line."""
        words = text.split()
        lines: list[str] = []
        line = ""
        for word in words:
            if line and len(line) + 1 + len(word) > max_chars:
                lines.append(line)
                line = word
            else:
                line = f"{line} {word}" if line else word
        if line:
            lines.append(line)
        return lines

    def _draw_footer(self, screen: Screen, installed: bool) -> None:
        theme = screen.theme
        y = 444
        h = 36
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, h))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        if installed:
            hx += screen.draw_button_hint("A", "Launch", hx, y + 8, btn_color=theme.btn_a) + 14
            hx += screen.draw_button_hint("Y", "Remove", hx, y + 8, btn_color=theme.btn_y) + 14
        else:
            hx += screen.draw_button_hint("A", "Install", hx, y + 8, btn_color=theme.btn_a) + 14
        hx += screen.draw_button_hint("B", "Back", hx, y + 8, btn_color=theme.btn_b) + 14
