"""HTML article extraction using readability-lxml + BeautifulSoup."""

from __future__ import annotations

import logging
from typing import TYPE_CHECKING
from urllib.parse import urlparse

from cartridge_sdk.reader.models import Article, ContentBlock

if TYPE_CHECKING:
    from cartridge_sdk.net import HttpClient

log = logging.getLogger(__name__)


class ArticleExtractor:
    """Extracts readable article content from a URL."""

    def __init__(self, http: HttpClient) -> None:
        self.http = http

    async def extract(self, url: str) -> Article:
        """Fetch URL and extract article content."""
        domain = urlparse(url).netloc

        resp = await self.http.get_cached(url, ttl_seconds=600)
        if not resp.ok:
            return Article(
                url=url, domain=domain, title="Failed to load",
                blocks=[ContentBlock(type="text", text=f"Could not fetch article (HTTP {resp.status})")],
            )

        html = resp.text()

        try:
            return self._extract_with_readability(html, url, domain)
        except Exception as e:
            log.warning("Readability extraction failed: %s", e)
            try:
                return self._extract_fallback(html, url, domain)
            except Exception as e2:
                log.warning("Fallback extraction failed: %s", e2)
                return Article(
                    url=url, domain=domain, title="Extraction failed",
                    blocks=[ContentBlock(type="text", text="Could not extract article content.")],
                )

    def _extract_with_readability(self, html: str, url: str, domain: str) -> Article:
        from readability import Document

        doc = Document(html)
        title = doc.title()
        summary_html = doc.summary()

        blocks = self._html_to_blocks(summary_html)
        return Article(url=url, title=title, domain=domain, blocks=blocks)

    def _extract_fallback(self, html: str, url: str, domain: str) -> Article:
        from bs4 import BeautifulSoup

        soup = BeautifulSoup(html, "html.parser")

        article = soup.find("article")
        if not article:
            article = soup.find("body") or soup

        title = ""
        title_tag = soup.find("title")
        if title_tag:
            title = title_tag.get_text(strip=True)

        blocks = self._walk_element(article)
        return Article(url=url, title=title, domain=domain, blocks=blocks)

    def _html_to_blocks(self, html: str) -> list[ContentBlock]:
        from bs4 import BeautifulSoup

        soup = BeautifulSoup(html, "html.parser")
        body = soup.find("body") or soup
        return self._walk_element(body)

    def _walk_element(self, element) -> list[ContentBlock]:
        from bs4 import NavigableString, Tag

        blocks: list[ContentBlock] = []

        for child in element.children:
            if isinstance(child, NavigableString):
                text = child.strip()
                if text:
                    blocks.append(ContentBlock(type="text", text=text))
                continue

            if not isinstance(child, Tag):
                continue

            name = child.name.lower()

            if name in ("h1", "h2", "h3", "h4", "h5", "h6"):
                text = child.get_text(strip=True)
                if text:
                    blocks.append(ContentBlock(type="heading", text=text, level=int(name[1])))

            elif name == "img":
                src = child.get("src", "")
                alt = child.get("alt", "")
                if src:
                    blocks.append(ContentBlock(type="image", url=src, alt=alt))

            elif name in ("pre", "code"):
                text = child.get_text()
                if text.strip():
                    blocks.append(ContentBlock(type="code", text=text.strip()))

            elif name == "blockquote":
                text = child.get_text(strip=True)
                if text:
                    blocks.append(ContentBlock(type="quote", text=text))

            elif name in ("p", "div", "section", "span", "li"):
                img = child.find("img")
                if img:
                    src = img.get("src", "")
                    alt = img.get("alt", "")
                    if src:
                        blocks.append(ContentBlock(type="image", url=src, alt=alt))

                text = child.get_text(strip=True)
                if text:
                    has_bold = child.find(["strong", "b"]) is not None
                    blocks.append(ContentBlock(type="text", text=text, bold=has_bold))

            elif name in ("ul", "ol"):
                for li in child.find_all("li", recursive=False):
                    text = li.get_text(strip=True)
                    if text:
                        blocks.append(ContentBlock(type="text", text=f"  \u2022 {text}"))

            elif name == "figure":
                img = child.find("img")
                if img:
                    src = img.get("src", "")
                    alt = img.get("alt", "")
                    if src:
                        blocks.append(ContentBlock(type="image", url=src, alt=alt))
                caption = child.find("figcaption")
                if caption:
                    text = caption.get_text(strip=True)
                    if text:
                        blocks.append(ContentBlock(type="text", text=text))

            else:
                blocks.extend(self._walk_element(child))

        return blocks
