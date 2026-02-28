"""Hacker News Firebase API client."""

from __future__ import annotations

import asyncio
import calendar
import html
import re
from datetime import date
from typing import TYPE_CHECKING

from models import Story, Comment

if TYPE_CHECKING:
    from cartridge_sdk.net import HttpClient

BASE_URL = "https://hacker-news.firebaseio.com/v0"
ALGOLIA_URL = "https://hn.algolia.com/api/v1/search"


class HNApi:
    def __init__(self, http: HttpClient) -> None:
        self.http = http

    async def get_top_stories(self, limit: int = 30) -> list[int]:
        resp = await self.http.get_cached(f"{BASE_URL}/topstories.json", ttl_seconds=120)
        if resp.ok:
            return resp.json()[:limit]
        return []

    async def get_new_stories(self, limit: int = 30) -> list[int]:
        resp = await self.http.get_cached(f"{BASE_URL}/newstories.json", ttl_seconds=120)
        if resp.ok:
            return resp.json()[:limit]
        return []

    async def get_best_stories(self, limit: int = 30) -> list[int]:
        resp = await self.http.get_cached(f"{BASE_URL}/beststories.json", ttl_seconds=120)
        if resp.ok:
            return resp.json()[:limit]
        return []

    async def get_item(self, item_id: int) -> dict | None:
        resp = await self.http.get_cached(f"{BASE_URL}/item/{item_id}.json", ttl_seconds=300)
        if resp.ok:
            return resp.json()
        return None

    async def get_stories(self, story_ids: list[int]) -> list[Story]:
        tasks = [self.get_item(sid) for sid in story_ids]
        results = await asyncio.gather(*tasks, return_exceptions=True)
        stories = []
        for r in results:
            if isinstance(r, dict) and r is not None:
                stories.append(Story(
                    id=r.get("id", 0),
                    title=r.get("title", ""),
                    url=r.get("url", ""),
                    score=r.get("score", 0),
                    by=r.get("by", ""),
                    time=r.get("time", 0),
                    descendants=r.get("descendants", 0),
                    kids=r.get("kids", []),
                    text=r.get("text", ""),
                ))
        return stories

    async def get_front_page_by_date(self, day: date, limit: int = 30) -> list[Story]:
        start = calendar.timegm(day.timetuple())
        end = start + 86400
        url = (
            f"{ALGOLIA_URL}?tags=front_page"
            f"&numericFilters=created_at_i>{start},created_at_i<{end}"
            f"&hitsPerPage={limit}"
        )
        resp = await self.http.get_cached(url, ttl_seconds=3600)
        if not resp.ok:
            return []
        hits = resp.json().get("hits", [])
        stories = []
        for h in hits:
            stories.append(Story(
                id=int(h.get("objectID", 0)),
                title=h.get("title", ""),
                url=h.get("url", ""),
                score=h.get("points", 0) or 0,
                by=h.get("author", ""),
                time=h.get("created_at_i", 0),
                descendants=h.get("num_comments", 0) or 0,
            ))
        return stories

    async def get_comments(
        self,
        comment_ids: list[int],
        depth: int = 0,
        max_depth: int = 3,
    ) -> list[Comment]:
        if depth > max_depth or not comment_ids:
            return []

        # Limit concurrent fetches
        batch = comment_ids[:20]
        tasks = [self.get_item(cid) for cid in batch]
        results = await asyncio.gather(*tasks, return_exceptions=True)

        comments: list[Comment] = []
        for r in results:
            if not isinstance(r, dict) or r is None:
                continue
            if r.get("deleted") or r.get("dead"):
                continue

            comment = Comment(
                id=r.get("id", 0),
                by=r.get("by", "[deleted]"),
                text=strip_html(r.get("text", "")),
                time=r.get("time", 0),
                kids=r.get("kids", []),
                parent=r.get("parent", 0),
                depth=depth,
            )
            comments.append(comment)

            # Recurse into children
            if comment.kids and depth < max_depth:
                children = await self.get_comments(comment.kids, depth + 1, max_depth)
                comments.extend(children)

        return comments


def strip_html(text: str) -> str:
    """Strip HTML tags and decode entities."""
    text = re.sub(r"<br\s*/?>", "\n", text, flags=re.IGNORECASE)
    text = re.sub(r"<p>", "\n\n", text, flags=re.IGNORECASE)
    text = re.sub(r"<[^>]+>", "", text)
    text = html.unescape(text)
    return text.strip()


def time_ago(timestamp: int) -> str:
    """Convert unix timestamp to relative time string."""
    import time
    diff = int(time.time()) - timestamp
    if diff < 60:
        return "just now"
    if diff < 3600:
        m = diff // 60
        return f"{m}m ago"
    if diff < 86400:
        h = diff // 3600
        return f"{h}h ago"
    d = diff // 86400
    return f"{d}d ago"
