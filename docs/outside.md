# Outside integration and verification

`lua_cartridges/outside` is a finished standalone Lua cartridge. Manifest ID:
`dev.cartridge.outside`; category: `tools`; entry: `main.lua`; permissions:
`network`, `storage`. The launcher icon is `lua_cartridges/outside/icon.png`.
Registry and shared simulator registration are owned by the parent integration.
No changes to the old weather cartridge are required.

## Implementation

- `main.lua`: lifecycle, controller routing, async request ownership, location
  search, persistent settings, cache freshness, retries, and deletion/reorder.
- `model.lua`: bounded Open-Meteo decoding, local ISO timestamps, calendar
  weekdays, SI storage, display conversions, and URL encoding.
- `view.lua`: 720px poster layout, original terrain and condition artwork,
  current/hourly/daily displays, details, places/search/error/empty screens.
  Drawing is read-only; terrain contours and circle spans are prepared at load.
- `assets/icon.svg`, `assets/icon.png`, `icon.png`: original launcher artwork.
- `README.md`: complete controls, data attribution, and normal usage.

All requests use `http.get_async` and are drained with `http.poll` in
`on_update`. A forecast completion is tied to its request ID and coordinate
key; it can update only that place's cache. Selection is independent. New
searches/cancellation advance a generation so stale searches cannot replace
new results. The selected hour is preserved when an active forecast refreshes.
Callbacks time out after 35 seconds, and late timed-out replies are discarded.
Submission errors are caught. There are at most 12 tracked pending requests,
eight saved places/caches/search results, seven daily rows, and 180 hourly rows
per cache (including allowance for clock changes). Repeated refresh during a
pending request is deduplicated. A deleted place's data cannot be resurrected
by an old response.

Units are converted locally from Celsius, km/h, and mm, so changing units does
not issue new HTTP. `timezone=auto` and ISO timestamps drive all labels;
day membership comes from each returned timestamp, never `day * 24` or the
handheld's time zone. `is_day` drives the scene's night palette. Sunrise/sunset
and the selected local hour affect the daytime palette and sun position.
Missing values remain missing, rather than being fabricated as zero.

## Persistent settings schema

Runtime namespace: `dev.cartridge.outside`. Normal files are under
`<CARTRIDGE_HOME>/.cartridges/dev.cartridge.outside/data/`.
The only storage keys are `preferences` and `forecasts`.

`preferences.json` / `--seed preferences=/tmp/outside-preferences.json`:

```json
{
  "version": 1,
  "units": "metric",
  "selected": "49.6116,6.1319",
  "places": [
    {
      "name": "Luxembourg",
      "lat": 49.6116,
      "lon": 6.1319,
      "country": "LU",
      "region": "",
      "key": "49.6116,6.1319"
    },
    {
      "name": "Tokyo",
      "lat": 35.6762,
      "lon": 139.6503,
      "country": "JP",
      "region": "Tokyo",
      "key": "35.6762,139.6503"
    }
  ]
}
```

`units` accepts `metric` or `imperial`. Array order is favorite order. `key`
is recomputed from validated coordinates to four decimals, so seeds may omit
it. `selected` is a coordinate key, not an array index. Duplicate coordinates
are merged on restore. An explicitly empty saved `places` array stays empty
after restart; missing/invalid preferences get the initial Luxembourg place.

`forecasts.json` contains:

```json
{
  "version": 1,
  "entries": {
    "49.6116,6.1319": {
      "fetched": 1790800000,
      "raw": {
        "timezone": "Europe/Luxembourg",
        "timezone_abbreviation": "CEST",
        "current": {},
        "hourly": {},
        "daily": {}
      }
    }
  }
}
```

The `raw` placeholder above must be a real forecast object to be usable. Saved
payloads contain only supported fields and bounded rows. Every load passes
through the same validator as network data. Only saved-place keys are loaded
or written; removing a place purges its cache. Preferences save on changes;
forecasts save on a successful response. Read-back verifies writes because
some runtime backends silently discard filesystem errors; a failed read-back
shows “Storage unavailable” while retaining usable in-memory data.

## Deterministic HTTP fixture

`sim/fixtures/outside.json` is synthetic, not a live weather recording.
The default entry serves 168 hours and seven days for Luxembourg starting
2026-10-01, current time 14:15. Its daily conditions are, in order: clear,
rain, snow, fog, storm, partly cloudy, overcast. `is_day` changes at 07:00
and 19:00; sunrise/sunset fields are separately populated. The longest-prefix
Tokyo entry uses `Asia/Tokyo` / JST and current time 21:15. Geocoding any
two-character query returns Tokyo and Paris. Responses complete after three
polls. These fixed dates are deliberately independent of the test machine's
date. Unmatched HTTP URLs fail offline.

From a fresh fixture launch:

| Scenario | Buttons |
| --- | --- |
| Current | None |
| Night | L2 twice (02:00) |
| Rain / snow / fog / storm | Right one / two / three / four times |
| Details | A |
| Imperial units | Y, R2, B |
| Places | Y |
| Built-in keyboard | Y, Y |
| Search results | Y, Y, A, A, Start (types `qq`) |
| Add Tokyo | Previous sequence, then A |
| Reorder Tokyo | Y, L1 after adding |
| Remove focused place | Y, X, A from landscape |
| Forecast/network error | Use `sim/fixtures/http.json` |
| Empty favorites | Seed preferences with `{"version":1,"places":[]}` |

The in-app [README](../lua_cartridges/outside/README.md) documents every control.
Select cancels an open keyboard; otherwise it exits to the launcher.

## Reproduce native captures

The `app-check` runner uses isolated temporary storage. No edits to
shared `sim_check.rs` or `app_check.rs` are necessary. Run from the repo root:

```sh
cargo test -p cartridge-lua --test outside
cargo run --bin app-check -- outside --fixture sim/fixtures/outside.json \
  --press 10:a --capture 8,45 --frames 50 --out /tmp/outside-final/details

# Clear, night, rain, snow, fog, storm, partly cloudy, overcast, details.
cargo run --bin app-check -- outside --fixture sim/fixtures/outside.json \
  --press 10:l2 --press 14:l2 --press 24:b --press 28:right \
  --press 36:right --press 44:right --press 52:right --press 60:right \
  --press 68:right --press 76:a --capture 8,20,32,40,48,56,64,72,80 \
  --frames 90 --out /tmp/outside-final/conditions

# The real keyboard, search, save Tokyo, reorder, confirm remove, return.
cargo run --bin app-check -- outside --fixture sim/fixtures/outside.json \
  --press 8:y --press 12:y --press 16:a --press 20:a --press 24:start \
  --press 36:a --press 46:y --press 54:l1 --press 62:x --press 70:a \
  --press 78:b --capture 14,32,42,50,58,66,74,82 --frames 90 \
  --out /tmp/outside-final/search

cargo run --bin app-check -- outside --fixture sim/fixtures/outside.json \
  --press 8:y --press 12:r2 --press 16:b --press 24:a --capture 20,40 \
  --frames 45 --out /tmp/outside-final/units

cargo run --bin app-check -- outside --fixture sim/fixtures/http.json \
  --capture 1,40 --frames 45 --out /tmp/outside-final/error
```

Create a valid stale-cache seed directly from the fixture:

```sh
python3 - <<'PY'
import json
from pathlib import Path
raw = json.loads(Path('sim/fixtures/outside.json').read_text())[0]['body']
seed = {'version': 1, 'entries': {'49.6116,6.1319':
        {'fetched': 1790800000, 'raw': raw}}}
Path('/tmp/outside-forecasts.json').write_text(json.dumps(seed))
PY
cargo run --bin app-check -- outside --fixture sim/fixtures/http.json \
  --seed forecasts=/tmp/outside-forecasts.json --capture 40 --frames 45 \
  --out /tmp/outside-final/stale
```

For a live request omit `--fixture`, e.g. `--frames 100 --capture 90`.
The runner then paces frames so the async network response has time to arrive.

## Checks completed

Eleven mlua integration tests exercise the real cartridge modules and JSON API:
request construction/deduplication; all day/hour boundaries; stale location and
search replies; UTF-8 query encoding; keyboard submit/cancel; saved-place
selection, duplicates, reorder, removal and empty restart; cache restart and
unit persistence; malformed/null/partial replies; storage read-back failure;
request submission failure, timeout and retry; bounded data; refresh preserving
selection; and local calendar/DST behavior. Render stubs fail if networking,
storage, or text measurements happen inside render.

Actual 720 × 720 SDL captures were inspected for clear/night/rain/snow/fog/storm,
detail and imperial screens, eight saved places, keyboard, search results,
added Tokyo, reorder/remove, no data/loading, empty favorites, failed search,
and stale cached data. The native runner completed these controller sequences
without Lua errors. Live forecast and geocoding URLs were checked against the
official documentation and returned valid data. A real asynchronous runtime
forecast was also captured on 2026-10-01 (Luxembourg 21:15 local current data).

See [connected-app verification](connected-apps.md) for retained native screenshots and shared-runtime checks.

### Limits

- These are desktop native renderer checks, not RK3326 frame-time measurements
  or handheld hardware validation.
- The coast and crescent are illustrative. They do not claim to show local
  terrain, actual lunar phase, or solar azimuth.
- No geolocation permission or implicit location lookup is used. The initial
  Luxembourg location is replaceable through the built-in search flow.
