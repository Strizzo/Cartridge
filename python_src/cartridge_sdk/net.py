"""Async HTTP client with caching."""

from __future__ import annotations

import hashlib
import json
import logging
import time
from dataclasses import dataclass
from pathlib import Path

import aiohttp

log = logging.getLogger(__name__)


@dataclass
class Response:
    """HTTP response wrapper."""

    status: int
    data: bytes
    headers: dict

    def json(self):
        return json.loads(self.data)

    def text(self) -> str:
        return self.data.decode("utf-8", errors="replace")

    @property
    def ok(self) -> bool:
        return 200 <= self.status < 300


class HttpClient:
    """Async HTTP client with in-memory + disk cache."""

    def __init__(self, cache_dir: Path | None = None) -> None:
        self._session: aiohttp.ClientSession | None = None
        self._cache_dir = cache_dir
        self._mem_cache: dict[str, tuple[float, Response]] = {}  # url -> (expires_at, resp)

    async def _ensure_session(self) -> aiohttp.ClientSession:
        if self._session is None or self._session.closed:
            self._session = aiohttp.ClientSession(
                timeout=aiohttp.ClientTimeout(total=15),
            )
        return self._session

    async def get(self, url: str, headers: dict | None = None) -> Response:
        session = await self._ensure_session()
        try:
            async with session.get(url, headers=headers) as resp:
                data = await resp.read()
                return Response(
                    status=resp.status,
                    data=data,
                    headers=dict(resp.headers),
                )
        except Exception as e:
            log.warning("HTTP GET failed: %s - %s", url, e)
            return Response(status=0, data=b"", headers={})

    async def post(self, url: str, body=None, headers: dict | None = None) -> Response:
        session = await self._ensure_session()
        try:
            async with session.post(url, json=body, headers=headers) as resp:
                data = await resp.read()
                return Response(
                    status=resp.status,
                    data=data,
                    headers=dict(resp.headers),
                )
        except Exception as e:
            log.warning("HTTP POST failed: %s - %s", url, e)
            return Response(status=0, data=b"", headers={})

    async def get_cached(self, url: str, ttl_seconds: int = 300, headers: dict | None = None) -> Response:
        now = time.time()

        # Check memory cache
        if url in self._mem_cache:
            expires, cached_resp = self._mem_cache[url]
            if now < expires:
                return cached_resp

        # Check disk cache
        disk_resp = self._disk_load(url, ttl_seconds)
        if disk_resp is not None:
            self._mem_cache[url] = (now + ttl_seconds, disk_resp)
            return disk_resp

        # Fetch fresh
        resp = await self.get(url, headers)
        if resp.ok:
            self._mem_cache[url] = (now + ttl_seconds, resp)
            self._disk_save(url, resp)

        return resp

    def _cache_key(self, url: str) -> str:
        return hashlib.md5(url.encode()).hexdigest()

    def _disk_save(self, url: str, resp: Response) -> None:
        if not self._cache_dir:
            return
        try:
            self._cache_dir.mkdir(parents=True, exist_ok=True)
            key = self._cache_key(url)
            meta = {"url": url, "status": resp.status, "time": time.time()}
            (self._cache_dir / f"{key}.meta").write_text(json.dumps(meta))
            (self._cache_dir / f"{key}.data").write_bytes(resp.data)
        except Exception:
            pass

    def _disk_load(self, url: str, ttl: int) -> Response | None:
        if not self._cache_dir:
            return None
        try:
            key = self._cache_key(url)
            meta_path = self._cache_dir / f"{key}.meta"
            data_path = self._cache_dir / f"{key}.data"
            if not meta_path.exists() or not data_path.exists():
                return None
            meta = json.loads(meta_path.read_text())
            if time.time() - meta["time"] > ttl:
                return None
            data = data_path.read_bytes()
            return Response(status=meta["status"], data=data, headers={})
        except Exception:
            return None

    async def close(self) -> None:
        if self._session and not self._session.closed:
            await self._session.close()
