"""Async image loading and scaling for articles."""

from __future__ import annotations

import io
import logging
from typing import TYPE_CHECKING

import pygame

if TYPE_CHECKING:
    from cartridge_sdk.net import HttpClient

log = logging.getLogger(__name__)

MAX_IMAGES = 15
MAX_IMAGE_BYTES = 2 * 1024 * 1024  # 2MB


class ImageLoader:
    """Fetches and scales images for article rendering."""

    def __init__(self, http: HttpClient) -> None:
        self.http = http
        self._loaded_count = 0

    async def load_image(self, url: str, max_width: int = 600) -> pygame.Surface | None:
        """Fetch an image, scale to fit max_width, return Surface or None."""
        if self._loaded_count >= MAX_IMAGES:
            return None

        try:
            resp = await self.http.get_cached(url, ttl_seconds=3600)
            if not resp.ok:
                return None

            if len(resp.data) > MAX_IMAGE_BYTES:
                log.debug("Skipping large image: %d bytes", len(resp.data))
                return None

            surface = pygame.image.load(io.BytesIO(resp.data))

            w, h = surface.get_size()
            if w > max_width:
                scale = max_width / w
                new_w = max_width
                new_h = int(h * scale)
                surface = pygame.transform.smoothscale(surface, (new_w, new_h))

            surface = surface.convert()
            self._loaded_count += 1
            return surface

        except Exception as e:
            log.debug("Failed to load image %s: %s", url, e)
            return None
