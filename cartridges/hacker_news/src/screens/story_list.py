"""Story list screen with card-style stories and badges."""

from __future__ import annotations

import asyncio
from datetime import date, timedelta
from typing import Callable, TYPE_CHECKING

import pygame

from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import Screen, _get_font
from cartridge_sdk.ui import TabBar, Tab, StatusBar, LoadingIndicator

if TYPE_CHECKING:
    from api import HNApi, Story

CARD_RADIUS = 6
ROW_HEIGHT = 64


class StoryListScreen:
    """Main screen: tab bar + card-style story list."""

    def __init__(self, api: HNApi, on_story_select: Callable) -> None:
        self.api = api
        self.on_story_select = on_story_select

        self.tabs = TabBar(
            tabs=[Tab("Top", "top"), Tab("New", "new"), Tab("Best", "best")],
            on_change=self._on_tab_change,
        )
        self.loading = LoadingIndicator("Loading stories")
        self.status_bar = StatusBar("Hacker News")

        self._stories: dict[str, list] = {}
        self._cursor: int = 0
        self._loading = False
        self._error: str = ""
        self._date: date | None = None
        self._date_stories: list = []

    def _on_tab_change(self, tab: Tab) -> None:
        self._cursor = 0
        if tab.id in self._stories:
            pass
        else:
            asyncio.create_task(self.load_stories())

    async def load_stories(self) -> None:
        self._loading = True
        self.loading.visible = True
        self._error = ""

        tab_id = self.tabs.active_tab.id
        try:
            if tab_id == "top":
                ids = await self.api.get_top_stories()
            elif tab_id == "new":
                ids = await self.api.get_new_stories()
            else:
                ids = await self.api.get_best_stories()

            stories = await self.api.get_stories(ids)
            self._stories[tab_id] = stories
            self.status_bar.right_text = f"{len(stories)} stories"
            self.status_bar.right_color = (100, 220, 100)
        except Exception as e:
            self._error = str(e)
            self.status_bar.right_text = "Error"
            self.status_bar.right_color = (255, 100, 100)
        finally:
            self._loading = False
            self.loading.visible = False

    async def _load_date_stories(self) -> None:
        self._loading = True
        self.loading.visible = True
        try:
            self._date_stories = await self.api.get_front_page_by_date(self._date)
            self.status_bar.right_text = f"{len(self._date_stories)} stories"
            self.status_bar.right_color = (100, 220, 100)
        except Exception:
            self._date_stories = []
            self.status_bar.right_text = "Error"
            self.status_bar.right_color = (255, 100, 100)
        finally:
            self._loading = False
            self.loading.visible = False

    def _go_to_date(self, day: date) -> None:
        today = date.today()
        if day >= today:
            self._date = None
            self.status_bar.title = "Hacker News"
            tab_id = self.tabs.active_tab.id
            if tab_id not in self._stories:
                asyncio.create_task(self.load_stories())
        else:
            self._date = day
            self._cursor = 0
            self.status_bar.title = day.strftime("%b %d, %Y")
            asyncio.create_task(self._load_date_stories())

    def handle_input(self, event: InputEvent) -> None:
        if self._date is None and self.tabs.handle_input(event):
            return
        if event.action not in ("press", "repeat"):
            return

        if event.button == Button.DPAD_LEFT:
            current = self._date or date.today()
            self._go_to_date(current - timedelta(days=1))
            return
        elif event.button == Button.DPAD_RIGHT and self._date is not None:
            self._go_to_date(self._date + timedelta(days=1))
            return

        stories = self._date_stories if self._date else self._stories.get(self.tabs.active_tab.id, [])

        if event.button == Button.DPAD_UP:
            self._cursor = max(0, self._cursor - 1)
        elif event.button == Button.DPAD_DOWN:
            self._cursor = min(max(0, len(stories) - 1), self._cursor + 1)
        elif event.button == Button.A and stories:
            if self._cursor < len(stories):
                self.on_story_select(stories[self._cursor])
        elif event.button == Button.X:
            if self._date:
                asyncio.create_task(self._load_date_stories())
            else:
                asyncio.create_task(self.load_stories())
        elif event.button == Button.L2:
            visible = max(1, 370 // ROW_HEIGHT)
            self._cursor = max(0, self._cursor - visible)
        elif event.button == Button.R2:
            visible = max(1, 370 // ROW_HEIGHT)
            self._cursor = min(max(0, len(stories) - 1), self._cursor + visible)

    def draw(self, screen: Screen) -> None:
        screen.clear()
        theme = screen.theme
        s = screen.surface

        # Status bar (y=0, h=40)
        self.status_bar.draw(s, pygame.Rect(0, 0, 640, 40), theme)

        if self._date is None:
            # Tab bar (y=40, h=32)
            self.tabs.draw(s, pygame.Rect(0, 40, 640, 32), theme)
            content_y = 72
        else:
            content_y = 40

        # Content area
        footer_y = 444
        content_h = footer_y - content_y

        stories = self._date_stories if self._date else self._stories.get(self.tabs.active_tab.id, [])

        if stories:
            self._draw_story_list(screen, stories, content_y, content_h)

        # Loading overlay
        self.loading.draw(s, pygame.Rect(0, content_y, 640, content_h), theme)

        # Footer hints
        self._draw_footer(screen)

    def _draw_story_list(self, screen: Screen, stories: list, y_start: int, height: int) -> None:
        theme = screen.theme
        s = screen.surface

        visible = max(1, height // ROW_HEIGHT)
        n = len(stories)
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
            story = stories[idx]
            is_selected = idx == self._cursor
            self._draw_story_card(screen, story, y, is_selected)
            y += ROW_HEIGHT

        s.set_clip(clip)

        # Scroll indicator
        if n > visible:
            ind_x = 635
            bar_top = y_start + 4
            bar_h = height - 8
            track_color = (min(theme.border[0]+10, 255), min(theme.border[1]+10, 255), min(theme.border[2]+10, 255))
            pygame.draw.line(s, track_color, (ind_x, bar_top), (ind_x, bar_top + bar_h))
            thumb_h = max(8, bar_h * visible // n)
            progress = self._cursor / max(1, n - 1)
            thumb_y = bar_top + int((bar_h - thumb_h) * progress)
            pygame.draw.rect(s, theme.text_dim, (ind_x - 1, thumb_y, 3, thumb_h), border_radius=1)

    def _draw_story_card(self, screen: Screen, story, y: int, is_selected: bool) -> None:
        from api import time_ago
        theme = screen.theme
        s = screen.surface

        card_x = 6
        card_w = 628
        card_h = ROW_HEIGHT - 4

        # Card background
        card_rect = pygame.Rect(card_x, y, card_w, card_h)
        if is_selected:
            pygame.draw.rect(s, theme.card_highlight, card_rect, border_radius=CARD_RADIUS)
            pygame.draw.rect(s, theme.accent, card_rect, 1, border_radius=CARD_RADIUS)
        else:
            pygame.draw.rect(s, theme.card_bg, card_rect, border_radius=CARD_RADIUS)

        # Score area (left side)
        score_x = card_x + 8
        score_w = 46

        # Score color: gradient from dim to orange based on score
        score_intensity = min(1.0, story.score / 500)
        score_color = (
            int(140 + 115 * score_intensity),
            int(140 - 60 * score_intensity),
            int(140 - 100 * score_intensity),
        )

        font_score = _get_font("mono_bold", 16)
        score_str = str(story.score) if story.score < 10000 else f"{story.score // 1000}k"
        score_surf = font_score.render(score_str, True, score_color)
        s.blit(score_surf, (score_x + (score_w - score_surf.get_width()) // 2, y + 4))

        # Arrow indicator
        font_arrow = _get_font("mono", 10)
        arrow_surf = font_arrow.render("\u25B2", True, score_color)
        s.blit(arrow_surf, (score_x + (score_w - arrow_surf.get_width()) // 2,
                            y + 4 + font_score.get_linesize()))

        # Title
        title_x = card_x + score_w + 14
        title_max_w = card_w - score_w - 80  # leave room for comment badge

        font_title = _get_font("mono_bold" if is_selected else "mono", 14)
        title = story.title
        tw = font_title.size(title)[0]
        if tw > title_max_w:
            while len(title) > 1 and font_title.size(title + "..")[0] > title_max_w:
                title = title[:-1]
            title += ".."

        title_color = theme.text if is_selected else theme.text
        title_surf = font_title.render(title, True, title_color)
        s.blit(title_surf, (title_x, y + 4))

        # Story type badges
        badge_x = title_x
        if story.title.startswith("Ask HN:"):
            badge_x += screen.draw_pill("Ask", badge_x, y + 4 + font_title.get_linesize() + 1,
                                        bg_color=(180, 100, 255), font_size=9) + 4
        elif story.title.startswith("Show HN:"):
            badge_x += screen.draw_pill("Show", badge_x, y + 4 + font_title.get_linesize() + 1,
                                        bg_color=(80, 200, 120), font_size=9) + 4
        elif not story.url:
            badge_x += screen.draw_pill("Jobs", badge_x, y + 4 + font_title.get_linesize() + 1,
                                        bg_color=(255, 180, 60), text_color=(30, 30, 30), font_size=9) + 4

        # Meta line: author | time ago | domain
        font_meta = _get_font("mono", 11)
        domain = ""
        if story.url and "/" in story.url:
            parts = story.url.split("/")
            if len(parts) > 2:
                domain = parts[2]
        meta = f"{story.by} \u00B7 {time_ago(story.time)}"
        if domain:
            meta += f" \u00B7 {domain}"
        meta_surf = font_meta.render(meta, True, theme.text_dim)
        s.blit(meta_surf, (badge_x, y + 4 + font_title.get_linesize() + 2))

        # Comment count badge (right side, rounded pill)
        comment_count = story.descendants
        if comment_count > 0:
            comment_str = str(comment_count) if comment_count < 1000 else f"{comment_count // 1000}k"
            badge_color = (60, 60, 80) if not is_selected else (70, 70, 100)
            screen.draw_pill(comment_str,
                             card_x + card_w - 55, y + (card_h - 18) // 2,
                             bg_color=badge_color, text_color=theme.text_dim, font_size=11)

    def _draw_footer(self, screen: Screen) -> None:
        theme = screen.theme
        y = 444
        h = 36
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, h))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        if self._date is None:
            hx += screen.draw_button_hint("L1/R1", "Tab", hx, y + 8, btn_color=theme.btn_l) + 14
        hx += screen.draw_button_hint("A", "Open", hx, y + 8, btn_color=theme.btn_a) + 14
        hx += screen.draw_button_hint("X", "Refresh", hx, y + 8, btn_color=theme.btn_x) + 14
        hx += screen.draw_button_hint("\u25C0/\u25B6", "Day", hx, y + 8, btn_color=theme.btn_l) + 14
