"""Cartridge management: install, remove, launch, list."""

from __future__ import annotations

import io
import logging
import shutil
import urllib.request
import zipfile
from pathlib import Path

from cartridge_sdk.manifest import AppManifest

log = logging.getLogger(__name__)

CARTRIDGES_BASE = Path.home() / ".cartridges"
INSTALLED_DIR = CARTRIDGES_BASE / "installed"
LAUNCH_FILE = CARTRIDGES_BASE / ".launch"


def list_installed() -> list[AppManifest]:
    """Scan installed directory for cartridges with valid manifests."""
    if not INSTALLED_DIR.exists():
        return []
    manifests = []
    for child in sorted(INSTALLED_DIR.iterdir()):
        toml = child / "cartridge.toml"
        if child.is_dir() and toml.exists():
            try:
                manifests.append(AppManifest.from_file(toml))
            except Exception as e:
                log.warning("Skipping %s: %s", child.name, e)
    return manifests


def get_installed_path(app_id: str) -> Path | None:
    """Return the install directory for an app, or None if not installed."""
    path = INSTALLED_DIR / app_id
    if path.exists() and (path / "cartridge.toml").exists():
        return path
    return None


def is_installed(app_id: str) -> bool:
    return get_installed_path(app_id) is not None


def install_from_github(github_url: str, branch: str = "main") -> AppManifest:
    """Download a cartridge from GitHub, validate, and install.

    Accepts URLs like:
      https://github.com/owner/repo
      https://github.com/owner/repo/tree/branch
    """
    # Normalize URL
    url = github_url.rstrip("/")
    if "/tree/" in url:
        parts = url.split("/tree/")
        url = parts[0]
        branch = parts[1].split("/")[0]

    zip_url = f"{url}/archive/refs/heads/{branch}.zip"
    log.info("Downloading %s", zip_url)

    req = urllib.request.Request(zip_url, headers={"User-Agent": "Cartridge/0.1"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        data = resp.read()

    with zipfile.ZipFile(io.BytesIO(data)) as zf:
        # Find the root directory in the zip (GitHub zips have repo-branch/ prefix)
        names = zf.namelist()
        if not names:
            raise ValueError("Empty zip archive")
        root_prefix = names[0].split("/")[0] + "/"

        # Extract to temp dir, then validate
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            zf.extractall(tmp_path)
            extracted = tmp_path / root_prefix.rstrip("/")

            toml_path = extracted / "cartridge.toml"
            if not toml_path.exists():
                raise FileNotFoundError("No cartridge.toml found in repository")

            manifest = AppManifest.from_file(toml_path)

            # Copy to installed dir
            dest = INSTALLED_DIR / manifest.id
            if dest.exists():
                shutil.rmtree(dest)
            INSTALLED_DIR.mkdir(parents=True, exist_ok=True)
            shutil.copytree(extracted, dest)

    log.info("Installed %s v%s to %s", manifest.name, manifest.version, dest)
    return manifest


def remove_cartridge(app_id: str) -> bool:
    """Remove an installed cartridge. Returns True if it existed."""
    path = get_installed_path(app_id)
    if path is None:
        return False
    shutil.rmtree(path)
    log.info("Removed %s", app_id)
    return True


def request_launch(app_id: str) -> None:
    """Write a .launch file so the home loop knows which app to run next."""
    path = get_installed_path(app_id)
    if path is None:
        raise ValueError(f"App {app_id} is not installed")
    CARTRIDGES_BASE.mkdir(parents=True, exist_ok=True)
    LAUNCH_FILE.write_text(str(path))
    log.info("Launch requested: %s", app_id)
