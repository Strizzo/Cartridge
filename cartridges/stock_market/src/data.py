"""Stock data fetcher using yfinance."""

from __future__ import annotations

import asyncio
import logging
import time

from models import StockQuote, WatchlistItem, StockInfo, PriceHistory

log = logging.getLogger(__name__)

DEFAULT_WATCHLIST = [
    WatchlistItem("AAPL", "Apple"),
    WatchlistItem("GOOGL", "Alphabet"),
    WatchlistItem("MSFT", "Microsoft"),
    WatchlistItem("AMZN", "Amazon"),
    WatchlistItem("NVDA", "NVIDIA"),
    WatchlistItem("TSLA", "Tesla"),
    WatchlistItem("META", "Meta"),
]

DEFAULT_INDICES = [
    WatchlistItem("^GSPC", "S&P 500"),
    WatchlistItem("^IXIC", "NASDAQ"),
    WatchlistItem("^DJI", "Dow Jones"),
    WatchlistItem("^RUT", "Russell 2000"),
]

DEFAULT_CRYPTO = [
    WatchlistItem("BTC-USD", "Bitcoin"),
    WatchlistItem("ETH-USD", "Ethereum"),
    WatchlistItem("SOL-USD", "Solana"),
    WatchlistItem("XRP-USD", "XRP"),
    WatchlistItem("ADA-USD", "Cardano"),
    WatchlistItem("DOGE-USD", "Dogecoin"),
    WatchlistItem("AVAX-USD", "Avalanche"),
    WatchlistItem("DOT-USD", "Polkadot"),
]


# Curated stock universe organized by sector
STOCK_UNIVERSE: list[StockInfo] = [
    # Technology
    StockInfo("AAPL", "Apple", "Technology", "mega"),
    StockInfo("MSFT", "Microsoft", "Technology", "mega"),
    StockInfo("GOOGL", "Alphabet", "Technology", "mega"),
    StockInfo("AMZN", "Amazon", "Technology", "mega"),
    StockInfo("NVDA", "NVIDIA", "Technology", "mega"),
    StockInfo("META", "Meta Platforms", "Technology", "mega"),
    StockInfo("TSLA", "Tesla", "Technology", "mega"),
    StockInfo("AVGO", "Broadcom", "Technology", "mega"),
    StockInfo("ORCL", "Oracle", "Technology", "large"),
    StockInfo("CRM", "Salesforce", "Technology", "large"),
    StockInfo("AMD", "AMD", "Technology", "large"),
    StockInfo("INTC", "Intel", "Technology", "large"),
    StockInfo("ADBE", "Adobe", "Technology", "large"),
    StockInfo("CSCO", "Cisco", "Technology", "large"),
    StockInfo("QCOM", "Qualcomm", "Technology", "large"),
    StockInfo("IBM", "IBM", "Technology", "large"),
    StockInfo("NOW", "ServiceNow", "Technology", "large"),
    StockInfo("UBER", "Uber", "Technology", "large"),
    StockInfo("SQ", "Block", "Technology", "mid"),
    StockInfo("SNAP", "Snap", "Technology", "mid"),

    # Finance
    StockInfo("JPM", "JPMorgan Chase", "Finance", "mega"),
    StockInfo("V", "Visa", "Finance", "mega"),
    StockInfo("MA", "Mastercard", "Finance", "mega"),
    StockInfo("BAC", "Bank of America", "Finance", "large"),
    StockInfo("WFC", "Wells Fargo", "Finance", "large"),
    StockInfo("GS", "Goldman Sachs", "Finance", "large"),
    StockInfo("MS", "Morgan Stanley", "Finance", "large"),
    StockInfo("BLK", "BlackRock", "Finance", "large"),
    StockInfo("AXP", "American Express", "Finance", "large"),
    StockInfo("SCHW", "Charles Schwab", "Finance", "large"),
    StockInfo("C", "Citigroup", "Finance", "large"),
    StockInfo("USB", "U.S. Bancorp", "Finance", "mid"),
    StockInfo("PNC", "PNC Financial", "Finance", "mid"),

    # Healthcare
    StockInfo("UNH", "UnitedHealth", "Healthcare", "mega"),
    StockInfo("JNJ", "Johnson & Johnson", "Healthcare", "mega"),
    StockInfo("LLY", "Eli Lilly", "Healthcare", "mega"),
    StockInfo("ABBV", "AbbVie", "Healthcare", "large"),
    StockInfo("MRK", "Merck", "Healthcare", "large"),
    StockInfo("PFE", "Pfizer", "Healthcare", "large"),
    StockInfo("TMO", "Thermo Fisher", "Healthcare", "large"),
    StockInfo("ABT", "Abbott Labs", "Healthcare", "large"),
    StockInfo("DHR", "Danaher", "Healthcare", "large"),
    StockInfo("BMY", "Bristol-Myers", "Healthcare", "large"),
    StockInfo("AMGN", "Amgen", "Healthcare", "large"),
    StockInfo("GILD", "Gilead Sciences", "Healthcare", "large"),
    StockInfo("MRNA", "Moderna", "Healthcare", "mid"),

    # Energy
    StockInfo("XOM", "Exxon Mobil", "Energy", "mega"),
    StockInfo("CVX", "Chevron", "Energy", "mega"),
    StockInfo("COP", "ConocoPhillips", "Energy", "large"),
    StockInfo("SLB", "Schlumberger", "Energy", "large"),
    StockInfo("EOG", "EOG Resources", "Energy", "large"),
    StockInfo("MPC", "Marathon Petroleum", "Energy", "large"),
    StockInfo("PSX", "Phillips 66", "Energy", "large"),
    StockInfo("VLO", "Valero Energy", "Energy", "mid"),
    StockInfo("OXY", "Occidental", "Energy", "mid"),
    StockInfo("HAL", "Halliburton", "Energy", "mid"),

    # Consumer
    StockInfo("WMT", "Walmart", "Consumer", "mega"),
    StockInfo("PG", "Procter & Gamble", "Consumer", "mega"),
    StockInfo("KO", "Coca-Cola", "Consumer", "mega"),
    StockInfo("PEP", "PepsiCo", "Consumer", "mega"),
    StockInfo("COST", "Costco", "Consumer", "mega"),
    StockInfo("MCD", "McDonald's", "Consumer", "large"),
    StockInfo("NKE", "Nike", "Consumer", "large"),
    StockInfo("SBUX", "Starbucks", "Consumer", "large"),
    StockInfo("TGT", "Target", "Consumer", "large"),
    StockInfo("HD", "Home Depot", "Consumer", "mega"),
    StockInfo("LOW", "Lowe's", "Consumer", "large"),
    StockInfo("CL", "Colgate-Palmolive", "Consumer", "large"),
    StockInfo("EL", "Estée Lauder", "Consumer", "mid"),

    # Industrial
    StockInfo("CAT", "Caterpillar", "Industrial", "large"),
    StockInfo("HON", "Honeywell", "Industrial", "large"),
    StockInfo("UPS", "UPS", "Industrial", "large"),
    StockInfo("BA", "Boeing", "Industrial", "large"),
    StockInfo("GE", "GE Aerospace", "Industrial", "large"),
    StockInfo("RTX", "RTX Corp", "Industrial", "large"),
    StockInfo("LMT", "Lockheed Martin", "Industrial", "large"),
    StockInfo("DE", "Deere & Co", "Industrial", "large"),
    StockInfo("MMM", "3M", "Industrial", "mid"),
    StockInfo("FDX", "FedEx", "Industrial", "mid"),

    # Communication
    StockInfo("NFLX", "Netflix", "Communication", "mega"),
    StockInfo("DIS", "Walt Disney", "Communication", "large"),
    StockInfo("CMCSA", "Comcast", "Communication", "large"),
    StockInfo("T", "AT&T", "Communication", "large"),
    StockInfo("VZ", "Verizon", "Communication", "large"),
    StockInfo("TMUS", "T-Mobile", "Communication", "large"),
    StockInfo("SPOT", "Spotify", "Communication", "mid"),
    StockInfo("ROKU", "Roku", "Communication", "mid"),

    # Crypto
    StockInfo("BTC-USD", "Bitcoin", "Crypto", "mega"),
    StockInfo("ETH-USD", "Ethereum", "Crypto", "mega"),
    StockInfo("BNB-USD", "BNB", "Crypto", "mega"),
    StockInfo("SOL-USD", "Solana", "Crypto", "mega"),
    StockInfo("XRP-USD", "XRP", "Crypto", "mega"),
    StockInfo("ADA-USD", "Cardano", "Crypto", "large"),
    StockInfo("DOGE-USD", "Dogecoin", "Crypto", "large"),
    StockInfo("AVAX-USD", "Avalanche", "Crypto", "large"),
    StockInfo("DOT-USD", "Polkadot", "Crypto", "large"),
    StockInfo("LINK-USD", "Chainlink", "Crypto", "large"),
    StockInfo("MATIC-USD", "Polygon", "Crypto", "large"),
    StockInfo("UNI-USD", "Uniswap", "Crypto", "large"),
    StockInfo("ATOM-USD", "Cosmos", "Crypto", "mid"),
    StockInfo("LTC-USD", "Litecoin", "Crypto", "mid"),
    StockInfo("FIL-USD", "Filecoin", "Crypto", "mid"),
    StockInfo("APT-USD", "Aptos", "Crypto", "mid"),
    StockInfo("NEAR-USD", "NEAR Protocol", "Crypto", "mid"),
    StockInfo("ARB-USD", "Arbitrum", "Crypto", "mid"),
    StockInfo("OP-USD", "Optimism", "Crypto", "mid"),
    StockInfo("AAVE-USD", "Aave", "Crypto", "mid"),
]

SECTORS = ["Technology", "Finance", "Healthcare", "Energy", "Consumer", "Industrial", "Communication", "Crypto"]


def get_stocks_by_sector(sector: str) -> list[StockInfo]:
    return [s for s in STOCK_UNIVERSE if s.sector == sector]


class StockDataFetcher:
    """Fetches stock quotes via yfinance (in a thread)."""

    async def get_quotes(self, symbols: list[str]) -> list[StockQuote]:
        return await asyncio.to_thread(self._fetch_sync, symbols)

    async def get_history(self, symbol: str, period: str = "1mo") -> PriceHistory:
        return await asyncio.to_thread(self._fetch_history_sync, symbol, period)

    def _fetch_sync(self, symbols: list[str]) -> list[StockQuote]:
        try:
            import yfinance as yf
        except ImportError:
            log.error("yfinance not installed. Install with: pip install yfinance")
            return [
                StockQuote(symbol=s, name=s, price=0, change=0, change_pct=0)
                for s in symbols
            ]

        quotes: list[StockQuote] = []
        try:
            tickers = yf.Tickers(" ".join(symbols))
            for symbol in symbols:
                try:
                    ticker = tickers.tickers.get(symbol)
                    if ticker is None:
                        quotes.append(StockQuote(
                            symbol=symbol, name=symbol,
                            price=0, change=0, change_pct=0,
                        ))
                        continue

                    info = ticker.fast_info
                    price = info.last_price
                    prev = info.previous_close

                    if price is None or prev is None:
                        quotes.append(StockQuote(
                            symbol=symbol, name=symbol,
                            price=0, change=0, change_pct=0,
                        ))
                        continue

                    change = price - prev
                    change_pct = (change / prev) * 100 if prev else 0

                    w52_high = 0.0
                    w52_low = 0.0
                    try:
                        w52_high = float(info.year_high) if info.year_high else 0.0
                        w52_low = float(info.year_low) if info.year_low else 0.0
                    except Exception:
                        pass

                    quotes.append(StockQuote(
                        symbol=symbol,
                        name=symbol,
                        price=round(price, 2),
                        change=round(change, 2),
                        change_pct=round(change_pct, 2),
                        high=round(info.day_high or 0, 2),
                        low=round(info.day_low or 0, 2),
                        week52_high=round(w52_high, 2),
                        week52_low=round(w52_low, 2),
                        last_updated=time.time(),
                    ))
                except Exception as e:
                    log.warning("Failed to fetch %s: %s", symbol, e)
                    quotes.append(StockQuote(
                        symbol=symbol, name=symbol,
                        price=0, change=0, change_pct=0,
                    ))
        except Exception as e:
            log.error("yfinance batch fetch failed: %s", e)
            quotes = [
                StockQuote(symbol=s, name=s, price=0, change=0, change_pct=0)
                for s in symbols
            ]

        return quotes

    def _fetch_history_sync(self, symbol: str, period: str) -> PriceHistory:
        try:
            import yfinance as yf
        except ImportError:
            return PriceHistory()

        try:
            ticker = yf.Ticker(symbol)
            hist = ticker.history(period=period)
            if hist.empty:
                return PriceHistory()

            dates = [d.strftime("%m/%d") for d in hist.index]
            prices = [round(float(p), 2) for p in hist["Close"]]
            return PriceHistory(dates=dates, prices=prices)
        except Exception as e:
            log.warning("Failed to fetch history for %s: %s", symbol, e)
            return PriceHistory()
