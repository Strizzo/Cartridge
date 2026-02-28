"""Scoped key-value storage for app data."""

from __future__ import annotations

import json
from pathlib import Path


class AppStorage:
    """Scoped persistence for a single app."""

    def __init__(self, app_id: str, base_dir: Path | None = None) -> None:
        self.app_id = app_id
        base = base_dir or Path.home() / ".cartridges"
        self.data_dir = base / app_id / "data"
        self.cache_dir = base / app_id / "cache"
        self.data_dir.mkdir(parents=True, exist_ok=True)
        self.cache_dir.mkdir(parents=True, exist_ok=True)

    def save(self, key: str, data: dict) -> None:
        path = self.data_dir / f"{key}.json"
        path.write_text(json.dumps(data, indent=2))

    def load(self, key: str) -> dict | None:
        path = self.data_dir / f"{key}.json"
        if not path.exists():
            return None
        try:
            return json.loads(path.read_text())
        except (json.JSONDecodeError, OSError):
            return None

    def delete(self, key: str) -> None:
        path = self.data_dir / f"{key}.json"
        if path.exists():
            path.unlink()

    def list_keys(self) -> list[str]:
        return [p.stem for p in self.data_dir.glob("*.json")]
