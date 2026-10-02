# Outside

A controller-only weather cartridge for the 720 × 720 CartridgeOS handheld.
An original illustrated coast changes with the selected hour, daylight, and
weather: cream and vermilion sunshine, a moonlit blue coast, rain, snow, fog,
and storms. The illustration is decorative, not a map or an astronomical
moon-phase calculation.

Outside opens with Luxembourg. Add your own cities or postal codes with the
built-in keyboard. Up to eight saved places retain their order, selected
location, units, and last successful forecast across restarts.

## Controls

| Screen | Button | Action |
| --- | --- | --- |
| Landscape / details | D-pad left / right | Previous / next day, preserving the local hour |
| Landscape / details | D-pad up / down | Previous / next saved location |
| Landscape / details | L1 / R1 | Previous / next hour, across day boundaries |
| Landscape / details | L2 / R2 | Jump six hours |
| Landscape | A | Detailed current/hourly and daily values |
| Details | A or B | Back to landscape |
| Landscape | B | Return to current conditions |
| Landscape / details | X | Refresh or retry |
| Landscape / details | Y or Start | Saved places and units |
| Saved places | D-pad up / down; A | Focus a place; open it |
| Saved places | Y | Search/add a city or postal code |
| Saved places | X; A / B | Remove focused place; confirm / cancel |
| Saved places | L1 / R1 | Move focused place earlier / later in saved order |
| Saved places | R2 | Toggle °C / km/h / mm and °F / mph / in |
| Saved places | B or Start | Return to landscape |
| Search | D-pad up / down; A | Choose a result; save and open |
| Search | Y / X / B | Edit search / retry / return to saved places |
| Keyboard | D-pad; A | Choose a key; type it |
| Keyboard | Start | Submit search |
| Keyboard | B | Backspace; cancel when input is empty |
| Anywhere | Select | Runtime quit |

Select is reserved by the runtime. The shared keyboard may label it “Cancel”;
use B on empty input to cancel while staying in Outside.

## Weather and persistence

The landscape shows current conditions until an hour/day is selected. The
hour strip shows eight nearby hours. The seven-day strip shows daily high/low,
the day's weather symbol, and maximum precipitation probability. Details add
feels-like temperature, wind/direction/gusts, humidity, precipitation amount,
hourly precipitation probability, pressure, daily UV, sunrise, sunset, and
daylight duration. Missing fields are shown as `--`.

Every displayed date and time belongs to the forecast location. Storage and
freshness use epoch timestamps; forecast labels never use the device's time
zone. Cached forecasts remain visible offline and are explicitly marked
`STALE CACHE`. X retries. Successful data stays fresh for 30 minutes; a
30-second maintenance check refreshes expired data and retries failed requests.
An in-flight request is never duplicated. Saved forecasts are revalidated on
launch. No HTTP requests or text measurements occur in render callbacks.

## Data attribution

Weather: [Open-Meteo](https://open-meteo.com/),
[Forecast API documentation](https://open-meteo.com/en/docs).
Location search: [Open-Meteo Geocoding API](https://open-meteo.com/en/docs/geocoding-api),
using [GeoNames](https://www.geonames.org/) location data.
Open-Meteo weather data is provided under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).
The public API is used without credentials; its
[service terms](https://open-meteo.com/en/terms) apply.
Weather values are model forecasts, including the “current” fields.

The landscape, glyphs, and icon are original repo-native artwork. The icon
source is `assets/icon.svg`; `assets/icon.png` and the launcher's required
root `icon.png` are 192 × 192 rasterizations. Regenerate with:

```sh
magick -background none assets/icon.svg assets/icon.png
cp assets/icon.png icon.png
```

See [integration and verification](../../docs/outside.md) for storage seeds,
deterministic simulator scenarios, and test coverage.
