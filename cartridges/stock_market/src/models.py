"""Data models for Stock Market."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class StockQuote:
    symbol: str
    name: str
    price: float
    change: float
    change_pct: float
    high: float = 0.0
    low: float = 0.0
    volume: int = 0
    market_cap: str = ""
    last_updated: float = 0.0
    week52_high: float = 0.0
    week52_low: float = 0.0


@dataclass
class WatchlistItem:
    symbol: str
    name: str


@dataclass
class StockInfo:
    """Static stock info for browsing."""
    symbol: str
    name: str
    sector: str
    market_cap_tier: str = "large"  # "mega", "large", "mid"


@dataclass
class PriceHistory:
    """Historical price data."""
    dates: list[str] = field(default_factory=list)
    prices: list[float] = field(default_factory=list)
