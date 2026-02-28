"""Scrollable article reader view with text, images, code, and quotes."""

from __future__ import annotations

import asyncio
import logging
from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui.widget import Widget
from cartridge_sdk.ui.loading import LoadingIndicator
from cartridge_sdk.reader.extractor import ArticleExtractor
from cartridge_sdk.reader.image_loader import ImageLoader
from cartridge_sdk.reader.models import Article, ContentBlock

if TYPE_CHECKING:
    from cartridge_sdk.net import HttpClient
    from cartridge_sdk.theme import Theme

log = logging.getLogger(__name__)

PAD_X = 12
PAD_Y = 8
SCROLL_STEP = 24
PAGE_STEP = 240
IMAGE_PLACEHOLDER_H = 60
VIEWPORT_PREFETCH = 480  # load images within ±2 screens


class ReaderView(Widget):
    """Scrollable article renderer. Fetches a URL, extracts content, renders it.

    Supports text, headings, images (lazy-loaded), code blocks, and quotes.
    Input: DPAD_UP/DOWN scroll, L2/R2 page, B back.
    """

    def __init__(self, on_back: Callable | None = None) -> None:
        self.on_back = on_back
        self.article: Article | None = None
        self._scroll_y: int = 0
        self._total_height: int = 0
        self._items: list[_RenderItem] = []
        self._needs_layout: bool = True
        self._last_width: int = 0

        # State
        self._loading = True
        self._error: str | None = None
        self._loading_indicator = LoadingIndicator("Loading article")
        self._loading_indicator.visible = True

        # Image loading
        self._image_loader: ImageLoader | None = None
        self._pending_images: dict[int, str] = {}  # item_index -> url
        self._loading_images: set[int] = set()  # item indices currently being fetched

    async def load(self, url: str, http: HttpClient) -> None:
        """Fetch and extract article content from a URL."""
        self._loading = True
        self._error = None
        self._scroll_y = 0
        self._items = []
        self._pending_images = {}
        self._loading_images = set()
        self._loading_indicator.visible = True
        self._needs_layout = True

        self._image_loader = ImageLoader(http)
        extractor = ArticleExtractor(http)

        try:
            self.article = await extractor.extract(url)
        except Exception as e:
            log.warning("Article extraction failed: %s", e)
            self._error = str(e)
            self.article = None
        finally:
            self._loading = False
            self._loading_indicator.visible = False
            self._needs_layout = True

    def handle_input(self, event: InputEvent) -> bool:
        if event.action not in ("press", "repeat"):
            return False

        if event.button == Button.B:
            if self.on_back:
                self.on_back()
            return True
        if event.button == Button.DPAD_UP:
            self._scroll_y = max(0, self._scroll_y - SCROLL_STEP)
            return True
        if event.button == Button.DPAD_DOWN:
            self._scroll_y += SCROLL_STEP
            return True
        if event.button == Button.L2:
            self._scroll_y = max(0, self._scroll_y - PAGE_STEP)
            return True
        if event.button == Button.R2:
            self._scroll_y += PAGE_STEP
            return True

        return False

    def draw(self, surface: pygame.Surface, rect: pygame.Rect, theme: Theme) -> None:
        pygame.draw.rect(surface, theme.bg, rect)

        # Loading state
        if self._loading:
            self._loading_indicator.draw(surface, rect, theme)
            return

        # Error state
        if self._error:
            font = _get_font("mono", 14)
            err = font.render(f"Error: {self._error}", True, theme.text_error)
            surface.blit(err, (rect.x + PAD_X, rect.y + PAD_Y))
            return

        if not self.article:
            return

        content_width = rect.width - PAD_X * 2 - 8  # scrollbar margin

        # Re-layout if needed
        if self._needs_layout or content_width != self._last_width:
            self._layout(content_width, theme)
            self._last_width = content_width
            self._needs_layout = False

        # Clamp scroll
        view_h = rect.height
        max_scroll = max(0, self._total_height - view_h)
        self._scroll_y = min(self._scroll_y, max_scroll)

        # Viewport bounds
        vp_top = self._scroll_y
        vp_bottom = vp_top + view_h

        clip = surface.get_clip()
        surface.set_clip(rect)

        # Render visible items
        for idx, item in enumerate(self._items):
            item_top = item.y
            item_bottom = item.y + item.height
            if item_bottom < vp_top or item_top > vp_bottom:
                continue

            draw_y = rect.y + item.y - self._scroll_y
            draw_x = rect.x + PAD_X + item.x_offset

            if item.type == "text":
                font = _get_font(item.font_style, item.font_size)
                rendered = font.render(item.text, True, item.color)
                surface.blit(rendered, (draw_x, draw_y))

            elif item.type == "image":
                if item.surface is not None:
                    surface.blit(item.surface, (draw_x, draw_y))
                else:
                    # Placeholder
                    ph_rect = pygame.Rect(draw_x, draw_y, content_width, IMAGE_PLACEHOLDER_H)
                    pygame.draw.rect(surface, theme.card_bg, ph_rect, border_radius=4)
                    pygame.draw.rect(surface, theme.card_border, ph_rect, 1, border_radius=4)
                    ph_font = _get_font("mono", 12)
                    ph_text = ph_font.render("Loading image...", True, theme.text_dim)
                    surface.blit(ph_text, (
                        ph_rect.x + (ph_rect.width - ph_text.get_width()) // 2,
                        ph_rect.y + (ph_rect.height - ph_text.get_height()) // 2,
                    ))
                    # Trigger lazy load if in prefetch range
                    self._maybe_load_image(idx, vp_top, vp_bottom)

            elif item.type == "code_bg":
                code_rect = pygame.Rect(draw_x - 4, draw_y, content_width + 8, item.height)
                pygame.draw.rect(surface, theme.card_bg, code_rect, border_radius=4)
                pygame.draw.rect(surface, theme.card_border, code_rect, 1, border_radius=4)

            elif item.type == "quote_bar":
                bar_x = draw_x - 6
                pygame.draw.line(
                    surface, theme.accent,
                    (bar_x, draw_y), (bar_x, draw_y + item.height), 3,
                )

        surface.set_clip(clip)

        # Scroll indicator
        if self._total_height > view_h:
            ind_x = rect.right - 5
            bar_top = rect.y + PAD_Y
            bar_h = rect.height - PAD_Y * 2
            track_color = (
                min(theme.border[0] + 10, 255),
                min(theme.border[1] + 10, 255),
                min(theme.border[2] + 10, 255),
            )
            pygame.draw.line(surface, track_color, (ind_x, bar_top), (ind_x, bar_top + bar_h))
            thumb_h = max(8, bar_h * view_h // self._total_height)
            progress = self._scroll_y / max_scroll if max_scroll > 0 else 0
            thumb_y = bar_top + int((bar_h - thumb_h) * progress)
            pygame.draw.rect(surface, theme.text_dim, (ind_x - 1, thumb_y, 3, thumb_h))

    def _layout(self, max_width: int, theme: Theme) -> None:
        """Convert article blocks into positioned render items."""
        self._items = []
        self._pending_images = {}

        if not self.article:
            return

        y = PAD_Y

        font = _get_font("mono", 14)
        font_bold = _get_font("mono_bold", 16)
        font_heading = _get_font("mono_bold", 18)
        font_code = _get_font("mono", 13)
        lh = font.get_linesize()
        lh_bold = font_bold.get_linesize()
        lh_heading = font_heading.get_linesize()
        lh_code = font_code.get_linesize()

        # Article title
        if self.article.title:
            for line in _word_wrap(self.article.title, font_heading, max_width):
                self._items.append(_RenderItem(
                    type="text", y=y, height=lh_heading, text=line,
                    color=theme.text, font_style="mono_bold", font_size=18,
                ))
                y += lh_heading
            y += 4

        # Domain
        if self.article.domain:
            self._items.append(_RenderItem(
                type="text", y=y, height=lh, text=self.article.domain,
                color=theme.text_accent, font_style="mono", font_size=14,
            ))
            y += lh

        y += lh  # spacer after header

        # Content blocks
        for block in self.article.blocks:
            if block.type == "heading":
                y += 8  # extra spacing above heading
                h_size = 16 if block.level >= 3 else 18
                h_font = _get_font("mono_bold", h_size)
                h_lh = h_font.get_linesize()
                for line in _word_wrap(block.text, h_font, max_width):
                    self._items.append(_RenderItem(
                        type="text", y=y, height=h_lh, text=line,
                        color=theme.text, font_style="mono_bold", font_size=h_size,
                    ))
                    y += h_lh
                y += 4  # spacing below heading

            elif block.type == "text":
                style = "mono_bold" if block.bold else "mono"
                size = 14
                f = _get_font(style, size)
                f_lh = f.get_linesize()
                for line in _word_wrap(block.text, f, max_width):
                    self._items.append(_RenderItem(
                        type="text", y=y, height=f_lh, text=line,
                        color=theme.text, font_style=style, font_size=size,
                    ))
                    y += f_lh
                y += 6  # paragraph spacing

            elif block.type == "image":
                item_idx = len(self._items)
                # Start with placeholder height; will be updated when image loads
                self._items.append(_RenderItem(
                    type="image", y=y, height=IMAGE_PLACEHOLDER_H,
                    image_url=block.url, block_ref=block,
                ))
                self._pending_images[item_idx] = block.url
                y += IMAGE_PLACEHOLDER_H + 8

                # Alt text
                if block.alt:
                    for line in _word_wrap(block.alt, font, max_width):
                        self._items.append(_RenderItem(
                            type="text", y=y, height=lh, text=line,
                            color=theme.text_dim, font_style="mono", font_size=14,
                        ))
                        y += lh
                    y += 4

            elif block.type == "code":
                code_lines = block.text.split("\n")
                block_h = len(code_lines) * lh_code + 12  # padding
                # Background card
                self._items.append(_RenderItem(
                    type="code_bg", y=y, height=block_h,
                ))
                y += 6  # top padding inside card
                for cline in code_lines:
                    for wline in _word_wrap(cline, font_code, max_width - 16):
                        self._items.append(_RenderItem(
                            type="text", y=y, height=lh_code, text=wline,
                            color=theme.text, font_style="mono", font_size=13,
                            x_offset=8,
                        ))
                        y += lh_code
                y += 6 + 8  # bottom padding + spacing

            elif block.type == "quote":
                quote_lines = []
                for line in _word_wrap(block.text, font, max_width - 16):
                    quote_lines.append(line)
                block_h = len(quote_lines) * lh
                # Left bar decoration
                self._items.append(_RenderItem(
                    type="quote_bar", y=y, height=block_h,
                    x_offset=0,
                ))
                for qline in quote_lines:
                    self._items.append(_RenderItem(
                        type="text", y=y, height=lh, text=qline,
                        color=theme.text_dim, font_style="mono", font_size=14,
                        x_offset=10,
                    ))
                    y += lh
                y += 8  # spacing after quote

        y += PAD_Y
        self._total_height = y

    def _maybe_load_image(self, item_idx: int, vp_top: int, vp_bottom: int) -> None:
        """Trigger async image load if the item is near the viewport."""
        if item_idx not in self._pending_images:
            return
        if item_idx in self._loading_images:
            return

        item = self._items[item_idx]
        # Check if within prefetch range
        if item.y + item.height < vp_top - VIEWPORT_PREFETCH:
            return
        if item.y > vp_bottom + VIEWPORT_PREFETCH:
            return

        self._loading_images.add(item_idx)
        url = self._pending_images[item_idx]
        asyncio.create_task(self._do_load_image(item_idx, url))

    async def _do_load_image(self, item_idx: int, url: str) -> None:
        """Load an image and update the render item."""
        if self._image_loader is None:
            return

        max_w = self._last_width
        surface = await self._image_loader.load_image(url, max_width=max_w)

        if surface is not None and item_idx < len(self._items):
            item = self._items[item_idx]
            item.surface = surface
            # Update block ref so it's cached
            if item.block_ref is not None:
                item.block_ref.surface = surface

            # Adjust height and reflow items below
            old_h = item.height
            new_h = surface.get_height()
            delta = new_h - old_h
            item.height = new_h

            if delta != 0:
                for i in range(item_idx + 1, len(self._items)):
                    self._items[i].y += delta
                self._total_height += delta

        self._pending_images.pop(item_idx, None)
        self._loading_images.discard(item_idx)


class _RenderItem:
    """A positioned renderable element."""

    __slots__ = (
        "type", "y", "height", "text", "color", "font_style", "font_size",
        "x_offset", "surface", "image_url", "block_ref",
    )

    def __init__(
        self,
        type: str = "text",
        y: int = 0,
        height: int = 0,
        text: str = "",
        color: tuple = (220, 220, 230),
        font_style: str = "mono",
        font_size: int = 14,
        x_offset: int = 0,
        surface: pygame.Surface | None = None,
        image_url: str = "",
        block_ref: ContentBlock | None = None,
    ) -> None:
        self.type = type
        self.y = y
        self.height = height
        self.text = text
        self.color = color
        self.font_style = font_style
        self.font_size = font_size
        self.x_offset = x_offset
        self.surface = surface
        self.image_url = image_url
        self.block_ref = block_ref


def _word_wrap(text: str, font: pygame.font.Font, max_width: int) -> list[str]:
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
