"""Data models for the Weather app."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Optional


# ── Preset cities ────────────────────────────────────────────────────────────

@dataclass
class City:
    key: str
    name: str
    country: str
    latitude: float
    longitude: float


CITIES: list[City] = [
    City("new_york", "New York", "US", 40.7128, -74.0060),
    City("london", "London", "UK", 51.5074, -0.1278),
    City("tokyo", "Tokyo", "JP", 35.6762, 139.6503),
    City("sydney", "Sydney", "AU", -33.8688, 151.2093),
    City("paris", "Paris", "FR", 48.8566, 2.3522),
    City("berlin", "Berlin", "DE", 52.5200, 13.4050),
    City("sao_paulo", "Sao Paulo", "BR", -23.5505, -46.6333),
    City("mumbai", "Mumbai", "IN", 19.0760, 72.8777),
]

CITY_MAP: dict[str, City] = {c.key: c for c in CITIES}

DEFAULT_CITY_KEY = "new_york"


# ── Weather condition mapping ────────────────────────────────────────────────

@dataclass
class WeatherCondition:
    label: str
    icon_lines: list[str]
    color: tuple[int, int, int]


def condition_from_code(code: int) -> WeatherCondition:
    """Map WMO weather code to a display condition."""
    if code == 0:
        return WeatherCondition(
            "Clear Sky",
            [
                "    \\   |   /",
                "      .---.",
                "  ---( O  )---",
                "      '---'",
                "    /   |   \\",
            ],
            (255, 220, 80),
        )
    if code in (1, 2, 3):
        return WeatherCondition(
            "Partly Cloudy" if code <= 2 else "Overcast",
            [
                r"   \  /",
                r" _ /''.-.",
                r"   \_(   ).",
                r"   /(___(__)",
                r"",
            ],
            (180, 200, 220),
        )
    if code in (45, 48):
        return WeatherCondition(
            "Fog",
            [
                r" _ - _ - _ -",
                r"  _ - _ - _",
                r" _ - _ - _ -",
                r"  _ - _ - _",
                r" _ - _ - _ -",
            ],
            (160, 160, 180),
        )
    if code in (51, 53, 55):
        return WeatherCondition(
            "Drizzle",
            [
                r"    .---.",
                r"   (     ).",
                r"  (______(_)",
                r"   ' ' ' '",
                r"  ' ' ' '",
            ],
            (120, 180, 240),
        )
    if code in (61, 63, 65):
        return WeatherCondition(
            "Rain",
            [
                r"    .---.",
                r"   (     ).",
                r"  (______(_)",
                r"  | | | | |",
                r"  | | | | |",
            ],
            (80, 150, 255),
        )
    if code in (71, 73, 75, 77):
        return WeatherCondition(
            "Snow",
            [
                r"    .---.",
                r"   (     ).",
                r"  (______(_)",
                r"  * * * * *",
                r"   * * * *",
            ],
            (220, 230, 255),
        )
    if code in (80, 81, 82):
        return WeatherCondition(
            "Showers",
            [
                r"    .---.",
                r"   (     ).",
                r"  (______(_)",
                r"  /|/|/|/|",
                r"  |/|/|/|/",
            ],
            (80, 140, 240),
        )
    if code in (95, 96, 99):
        return WeatherCondition(
            "Thunderstorm",
            [
                r"    .---.",
                r"   (     ).",
                r"  (______(_)",
                r"    / / /",
                r"   / / /",
            ],
            (200, 180, 60),
        )
    # Fallback
    return WeatherCondition(
        "Unknown",
        [
            r"    ?????",
            r"   ?     ?",
            r"       ??",
            r"      ?",
            r"      .",
        ],
        (180, 180, 180),
    )


# ── Data containers ──────────────────────────────────────────────────────────

@dataclass
class CurrentWeather:
    temperature: float
    feels_like: float
    humidity: float
    wind_speed: float
    pressure: float
    weather_code: int
    hourly_temps: list[float] = field(default_factory=list)
    sunrise: str = ""
    sunset: str = ""


@dataclass
class DayForecast:
    date: str
    weekday: str
    temp_max: float
    temp_min: float
    weather_code: int
    precipitation: float
    wind_max: float
    sunrise: str = ""
    sunset: str = ""


@dataclass
class ForecastData:
    days: list[DayForecast] = field(default_factory=list)


def temp_color(temp_c: float) -> tuple[int, int, int]:
    """Return a colour reflecting the temperature."""
    if temp_c <= -10:
        return (100, 160, 255)
    if temp_c <= 0:
        return (120, 190, 255)
    if temp_c <= 10:
        return (140, 210, 230)
    if temp_c <= 20:
        return (200, 220, 140)
    if temp_c <= 30:
        return (255, 200, 80)
    if temp_c <= 35:
        return (255, 150, 60)
    return (255, 90, 60)
