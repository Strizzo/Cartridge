"""Registry client: fetch and parse the app catalog."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from cartridge_sdk.net import HttpClient


@dataclass
class RegistryApp:
    """A single app entry from the registry."""

    id: str
    name: str
    description: str = ""
    version: str = "0.1.0"
    author: str = ""
    category: str = "other"
    tags: list[str] = field(default_factory=list)
    repo_url: str = ""
    permissions: list[str] = field(default_factory=list)


@dataclass
class Registry:
    """Parsed registry with category helpers."""

    version: int = 1
    apps: list[RegistryApp] = field(default_factory=list)

    def get_categories(self) -> list[str]:
        cats = sorted({a.category for a in self.apps})
        return cats

    def filter_by_category(self, category: str) -> list[RegistryApp]:
        return [a for a in self.apps if a.category == category]


class RegistryClient:
    """Fetches and parses the registry JSON."""

    def __init__(self, http: HttpClient, url: str) -> None:
        self.http = http
        self.url = url

    async def fetch(self) -> Registry:
        resp = await self.http.get_cached(self.url, ttl_seconds=600)
        if not resp.ok:
            return Registry()

        data = resp.json()
        apps = []
        for entry in data.get("apps", []):
            apps.append(RegistryApp(
                id=entry.get("id", ""),
                name=entry.get("name", ""),
                description=entry.get("description", ""),
                version=entry.get("version", "0.1.0"),
                author=entry.get("author", ""),
                category=entry.get("category", "other"),
                tags=entry.get("tags", []),
                repo_url=entry.get("repo_url", ""),
                permissions=entry.get("permissions", []),
            ))
        return Registry(version=data.get("version", 1), apps=apps)
