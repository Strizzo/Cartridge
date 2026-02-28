"""Hacker News client for Cartridge."""

import asyncio
from cartridge_sdk import CartridgeApp, AppContext, Screen, InputEvent
from cartridge_sdk.ui import ListItem

from api import HNApi
from screens.story_list import StoryListScreen
from screens.story_detail import StoryDetailScreen
from screens.reader import ReaderScreen


class HNApp(CartridgeApp):

    async def on_init(self, ctx: AppContext) -> None:
        await super().on_init(ctx)
        self.api = HNApi(ctx.http)

        self._screen_stack: list[str] = ["list"]

        self.story_list = StoryListScreen(self.api, self._open_story)
        self.story_detail = StoryDetailScreen(self.api, self._go_back, self._open_article)
        self.reader = ReaderScreen(self._go_back)

        asyncio.create_task(self.story_list.load_stories())

    def _open_story(self, story) -> None:
        self._screen_stack.append("detail")
        asyncio.create_task(self.story_detail.load(story))

    def _open_article(self, url: str) -> None:
        self._screen_stack.append("reader")
        asyncio.create_task(self.reader.load(url, self.ctx.http))

    def _go_back(self) -> None:
        if len(self._screen_stack) > 1:
            self._screen_stack.pop()

    def on_input(self, event: InputEvent) -> None:
        current = self._screen_stack[-1]
        if current == "list":
            self.story_list.handle_input(event)
        elif current == "detail":
            self.story_detail.handle_input(event)
        elif current == "reader":
            self.reader.handle_input(event)

    def on_render(self, screen: Screen) -> None:
        current = self._screen_stack[-1]
        if current == "list":
            self.story_list.draw(screen)
        elif current == "detail":
            self.story_detail.draw(screen)
        elif current == "reader":
            self.reader.draw(screen)
