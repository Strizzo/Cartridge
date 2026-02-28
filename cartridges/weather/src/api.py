"""Open-Meteo API client for the Weather app."""

from __future__ import annotations

import datetime
from typing import TYPE_CHECKING

from .models import City, CurrentWeather, DayForecast, ForecastData

if TYPE_CHECKING:
    from cartridge_sdk.net import HttpClient


_WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]


class WeatherApi:
    """Thin wrapper around the Open-Meteo free JSON API."""

    BASE = "https://api.open-meteo.com/v1/forecast"
    CACHE_TTL = 300  # 5 minutes

    def __init__(self, http: HttpClient) -> None:
        self._http = http

    # ── Current weather + 24h hourly temps ───────────────────────────────

    async def fetch_current(self, city: City) -> CurrentWeather:
        url = (
            f"{self.BASE}"
            f"?latitude={city.latitude}&longitude={city.longitude}"
            "&current=temperature_2m,relative_humidity_2m,"
            "apparent_temperature,weather_code,wind_speed_10m,surface_pressure"
            "&hourly=temperature_2m"
            "&daily=sunrise,sunset"
            "&timezone=auto&forecast_days=1"
        )
        resp = await self._http.get_cached(url, ttl_seconds=self.CACHE_TTL)
        if not resp.ok:
            raise RuntimeError(f"API error: {resp.status_code}")

        data = resp.json()
        cur = data["current"]
        hourly = data.get("hourly", {}).get("temperature_2m", [])
        daily = data.get("daily", {})

        sunrise_raw = ""
        sunset_raw = ""
        if daily.get("sunrise"):
            sunrise_raw = daily["sunrise"][0]
        if daily.get("sunset"):
            sunset_raw = daily["sunset"][0]

        sunrise_str = _format_time(sunrise_raw)
        sunset_str = _format_time(sunset_raw)

        return CurrentWeather(
            temperature=cur["temperature_2m"],
            feels_like=cur["apparent_temperature"],
            humidity=cur["relative_humidity_2m"],
            wind_speed=cur["wind_speed_10m"],
            pressure=cur["surface_pressure"],
            weather_code=cur["weather_code"],
            hourly_temps=hourly,
            sunrise=sunrise_str,
            sunset=sunset_str,
        )

    # ── 5-day forecast ───────────────────────────────────────────────────

    async def fetch_forecast(self, city: City) -> ForecastData:
        url = (
            f"{self.BASE}"
            f"?latitude={city.latitude}&longitude={city.longitude}"
            "&daily=weather_code,temperature_2m_max,temperature_2m_min,"
            "apparent_temperature_max,apparent_temperature_min,"
            "sunrise,sunset,precipitation_sum,wind_speed_10m_max"
            "&timezone=auto"
        )
        resp = await self._http.get_cached(url, ttl_seconds=self.CACHE_TTL)
        if not resp.ok:
            raise RuntimeError(f"API error: {resp.status_code}")

        data = resp.json()
        daily = data["daily"]

        days: list[DayForecast] = []
        for i in range(min(5, len(daily["time"]))):
            date_str = daily["time"][i]
            try:
                dt = datetime.date.fromisoformat(date_str)
                weekday = _WEEKDAYS[dt.weekday()]
            except (ValueError, IndexError):
                weekday = "???"

            sunrise_str = _format_time(daily.get("sunrise", [""])[i] if daily.get("sunrise") and i < len(daily["sunrise"]) else "")
            sunset_str = _format_time(daily.get("sunset", [""])[i] if daily.get("sunset") and i < len(daily["sunset"]) else "")

            days.append(
                DayForecast(
                    date=date_str,
                    weekday=weekday,
                    temp_max=daily["temperature_2m_max"][i],
                    temp_min=daily["temperature_2m_min"][i],
                    weather_code=daily["weather_code"][i],
                    precipitation=daily.get("precipitation_sum", [0.0] * 7)[i],
                    wind_max=daily.get("wind_speed_10m_max", [0.0] * 7)[i],
                    sunrise=sunrise_str,
                    sunset=sunset_str,
                )
            )

        return ForecastData(days=days)


def _format_time(iso: str) -> str:
    """Extract HH:MM from an ISO datetime string like '2024-01-15T07:30'."""
    if not iso:
        return "--:--"
    try:
        if "T" in iso:
            return iso.split("T")[1][:5]
        return iso[:5]
    except Exception:
        return "--:--"
