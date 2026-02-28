"""Data models for extracted article content."""

from __future__ import annotations

from dataclasses import dataclass, field

import pygame


@dataclass
class ContentBlock:
    """A block of article content.

    type is one of: "text", "heading", "image", "code", "quote"
    """

    type: str
    text: str = ""
    bold: bool = False
    level: int = 1
    url: str = ""
    alt: str = ""
    surface: pygame.Surface | None = None


@dataclass
class Article:
    """Extracted article content."""

    url: str = ""
    title: str = ""
    author: str = ""
    date: str = ""
    domain: str = ""
    blocks: list[ContentBlock] = field(default_factory=list)
