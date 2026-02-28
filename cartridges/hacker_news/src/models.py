"""Data models for Hacker News."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Story:
    id: int
    title: str
    url: str = ""
    score: int = 0
    by: str = ""
    time: int = 0
    descendants: int = 0
    kids: list[int] = field(default_factory=list)
    text: str = ""


@dataclass
class Comment:
    id: int
    by: str = ""
    text: str = ""
    time: int = 0
    kids: list[int] = field(default_factory=list)
    parent: int = 0
    depth: int = 0
