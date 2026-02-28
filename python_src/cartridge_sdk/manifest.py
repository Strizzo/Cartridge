"""Parse cartridge.toml app manifests."""

from __future__ import annotations

import tomllib
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class AppManifest:
    """Parsed app manifest from cartridge.toml."""

    id: str
    name: str
    description: str = ""
    version: str = "0.1.0"
    author: str = ""
    entry_point: str = "src/main.py"
    permissions: dict[str, bool] = field(default_factory=dict)

    @classmethod
    def from_file(cls, path: Path) -> AppManifest:
        with open(path, "rb") as f:
            data = tomllib.load(f)

        app = data.get("app", {})
        entry = app.get("entry", {})
        perms = data.get("permissions", {})

        return cls(
            id=app.get("id", "unknown"),
            name=app.get("name", "Untitled"),
            description=app.get("description", ""),
            version=app.get("version", "0.1.0"),
            author=app.get("author", ""),
            entry_point=entry.get("main", "src/main.py"),
            permissions=perms,
        )

    @classmethod
    def from_dir(cls, directory: Path) -> AppManifest:
        toml_path = directory / "cartridge.toml"
        if not toml_path.exists():
            raise FileNotFoundError(f"No cartridge.toml found in {directory}")
        return cls.from_file(toml_path)
