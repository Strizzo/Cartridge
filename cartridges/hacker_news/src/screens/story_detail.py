"""Story detail screen with styled comment thread."""

from __future__ import annotations

import asyncio
from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font
from cartridge_sdk.ui import StatusBar, LoadingIndicator

if TYPE_CHECKING:
    from api import HNApi
    from models import Story, Comment

CARD_RADIUS = 8

# Depth colors for comment thread lines (like Reddit)
DEPTH_COLORS = [
    (100, 180, 255),   # blue
    (180, 100, 255),   # purple
    (100, 220, 140),   # green
    (255, 180, 60),    # orange
    (255, 100, 140),   # pink
    (100, 220, 220),   # teal
]


class StoryDetailScreen:
    """Story content + scrollable comment thread with depth indicators."""

    def __init__(self, api: HNApi, on_back: Callable, on_read_article: Callable | None = None) -> None:
        self.api = api
        self.on_back = on_back
        self.on_read_article = on_read_article
        self.story: Story | None = None
        self.comments: list[Comment] = []
        self._scroll: int = 0
        self._loading = False
        self.loading = LoadingIndicator("Loading comments")
        self.status_bar = StatusBar("Story")
        self._lines: list[tuple] = []  # (text, color, indent_px, is_header, depth)
        self._needs_layout = True

    async def load(self, story: Story) -> None:
        self.story = story
        self.comments = []
        self._scroll = 0
        self._loading = True
        self.loading.visible = True
        self._needs_layout = True
        self.status_bar.title = "Story"

        try:
            if story.kids:
                self.comments = await self.api.get_comments(story.kids, max_depth=2)
            self.status_bar.right_text = f"{len(self.comments)} comments"
            self.status_bar.right_color = (100, 220, 100)
        except Exception:
            self.status_bar.right_text = "Error loading"
            self.status_bar.right_color = (255, 100, 100)
        finally:
            self._loading = False
            self.loading.visible = False
            self._needs_layout = True

    def handle_input(self, event: InputEvent) -> None:
        if event.action not in ("press", "repeat"):
            return
        if event.button == Button.B:
            self.on_back()
        elif event.button == Button.X:
            if self.on_read_article and self.story and self.story.url:
                self.on_read_article(self.story.url)
        elif event.button == Button.DPAD_UP:
            self._scroll = max(0, self._scroll - 1)
        elif event.button == Button.DPAD_DOWN:
            self._scroll += 1
        elif event.button == Button.L2:
            self._scroll = max(0, self._scroll - 10)
        elif event.button == Button.R2:
            self._scroll += 10

    def draw(self, screen: Screen) -> None:
        screen.clear()
        theme = screen.theme
        s = screen.surface

        # Status bar
        self.status_bar.draw(s, pygame.Rect(0, 0, 640, 40), theme)

        # Content area
        content_rect = pygame.Rect(0, 40, 640, 404)

        if self._needs_layout:
            self._layout(content_rect.width, theme, screen)
            self._needs_layout = False

        # Loading overlay
        if self.loading.visible:
            self.loading.draw(s, content_rect, theme)
            self._draw_footer(screen)
            return

        # Draw lines
        font = _get_font("mono", 14)
        lh = font.get_linesize()

        visible_lines = max(1, content_rect.height // lh)
        total = len(self._lines)
        max_scroll = max(0, total - visible_lines)
        self._scroll = min(self._scroll, max_scroll)

        clip = s.get_clip()
        s.set_clip(content_rect)

        y = content_rect.y + 6
        for i in range(self._scroll, min(self._scroll + visible_lines, total)):
            text, color, indent, is_header, depth = self._lines[i]
            x = content_rect.x + 10 + indent
            max_w = content_rect.width - 20 - indent

            # Draw depth indicator lines
            if depth > 0:
                for d in range(depth):
                    line_x = content_rect.x + 10 + d * 16 + 4
                    line_color = DEPTH_COLORS[d % len(DEPTH_COLORS)]
                    pygame.draw.line(s, line_color, (line_x, y), (line_x, y + lh), 2)

            # Truncate if needed
            display = text
            if font.size(display)[0] > max_w and max_w > 0:
                while len(display) > 1 and font.size(display + "..")[0] > max_w:
                    display = display[:-1]
                display += ".."

            if is_header:
                rendered = _get_font("mono_bold", 13).render(display, True, color)
            else:
                rendered = font.render(display, True, color)
            s.blit(rendered, (x, y))
            y += lh

        s.set_clip(clip)

        # Scroll indicator
        if total > visible_lines:
            ind_x = content_rect.right - 5
            bar_top = content_rect.y + 6
            bar_h = content_rect.height - 12
            tc = (min(theme.border[0]+10, 255), min(theme.border[1]+10, 255), min(theme.border[2]+10, 255))
            pygame.draw.line(s, tc, (ind_x, bar_top), (ind_x, bar_top + bar_h))
            thumb_h = max(8, bar_h * visible_lines // total)
            progress = self._scroll / max_scroll if max_scroll > 0 else 0
            thumb_y = bar_top + int((bar_h - thumb_h) * progress)
            pygame.draw.rect(s, theme.text_dim, (ind_x - 1, thumb_y, 3, thumb_h), border_radius=1)

        self._draw_footer(screen)

    def _layout(self, width: int, theme, screen: Screen) -> None:
        """Build the flat list of lines for scrolling."""
        self._lines = []
        font = _get_font("mono", 14)
        font_bold = _get_font("mono_bold", 15)
        max_w = width - 30

        if not self.story:
            return

        st = self.story
        from api import time_ago

        # Story title (bold, wrapped)
        for line in _word_wrap(st.title, font_bold, max_w):
            self._lines.append((line, theme.text, 0, False, 0))

        # Meta info
        meta = f"{st.score} pts \u00B7 {st.by} \u00B7 {time_ago(st.time)} \u00B7 {st.descendants} comments"
        self._lines.append((meta, theme.text_dim, 0, False, 0))

        # URL domain
        if st.url:
            domain = st.url.split("/")[2] if len(st.url.split("/")) > 2 else st.url
            self._lines.append((domain, theme.text_accent, 0, False, 0))

        # Story text
        if st.text:
            self._lines.append(("", theme.text, 0, False, 0))
            for line in st.text.split("\n"):
                if not line.strip():
                    self._lines.append(("", theme.text, 0, False, 0))
                else:
                    for wl in _word_wrap(line, font, max_w):
                        self._lines.append((wl, theme.text, 0, False, 0))

        # Separator
        self._lines.append(("", theme.text, 0, False, 0))
        self._lines.append(("\u2500" * 40 + " Comments " + "\u2500" * 10, theme.text_dim, 0, False, 0))
        self._lines.append(("", theme.text, 0, False, 0))

        # Comments
        if not self.comments:
            self._lines.append(("No comments loaded", theme.text_dim, 0, False, 0))
            return

        for c in self.comments:
            indent = c.depth * 16
            comment_w = max(100, max_w - indent)
            depth_color = DEPTH_COLORS[c.depth % len(DEPTH_COLORS)]

            # Author header with depth color
            header = f"{c.by} \u00B7 {time_ago(c.time)}"
            self._lines.append((header, depth_color, indent, True, c.depth))

            # Comment body
            for paragraph in c.text.split("\n"):
                if not paragraph.strip():
                    self._lines.append(("", theme.text, indent, False, c.depth))
                    continue
                for wl in _word_wrap(paragraph, font, comment_w):
                    self._lines.append((wl, theme.text, indent, False, c.depth))

            self._lines.append(("", theme.text, indent, False, c.depth))

    def _draw_footer(self, screen: Screen) -> None:
        theme = screen.theme
        y = 444
        h = 36
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, h))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        hx += screen.draw_button_hint("B", "Back", hx, y + 8, btn_color=theme.btn_b) + 14
        if self.story and self.story.url:
            hx += screen.draw_button_hint("X", "Read Article", hx, y + 8, btn_color=theme.btn_x) + 14
        hx += screen.draw_button_hint("\u2191\u2193", "Scroll", hx, y + 8, btn_color=theme.btn_l) + 14
        hx += screen.draw_button_hint("L2/R2", "Page", hx, y + 8, btn_color=theme.btn_l) + 14


def _word_wrap(text: str, font, max_width: int) -> list[str]:
    if not text:
        return [""]
    words = text.split(" ")
    lines = []
    current = ""
    for word in words:
        test = f"{current} {word}".strip()
        if font.size(test)[0] <= max_width:
            current = test
        else:
            if current:
                lines.append(current)
            if font.size(word)[0] > max_width:
                while word:
                    chunk = word
                    while font.size(chunk)[0] > max_width and len(chunk) > 1:
                        chunk = chunk[:-1]
                    lines.append(chunk)
                    word = word[len(chunk):]
                current = ""
            else:
                current = word
    if current:
        lines.append(current)
    return lines or [""]
