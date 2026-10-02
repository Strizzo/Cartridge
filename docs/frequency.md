# Frequency

Frequency is a 720×720 controller radio atlas in `lua_cartridges/frequency`.
It uses Radio Browser's live directory and Cartridge's native asynchronous
streaming player. Opening the cartridge never starts audio. Its launcher category
is `media`; `icon.png` is an original illustrated tuning dial.

## Controls

| Input | Action |
| --- | --- |
| D-pad up/down | Select stations; move through menus and countries. |
| D-pad left | Open Countries from a station list. |
| D-pad right | Enter map navigation. Only reported station coordinates become pins. |
| D-pad in map mode | Select the nearest pin in that direction, preferring alignment. B returns to the list. |
| A | Start the selected station; pause/resume that same station; retry it after error/end. With an empty failed directory, retry the directory. |
| B | Close the menu, return from map or another view, or stop audio while on Explore's station list. |
| X | Add/remove the selected favorite. |
| Y | On-screen station-name search; in Countries, country-name search. Submit blank text to clear the filter. |
| L1/R1 | Previous/next genre; applies to the directory and returns to Explore. |
| L2/R2 | Volume down/up by 5%, clamped to 0–100%, on every app screen. |
| Start | Open/close the menu. Opening always selects its first row. |
| Select | Runtime-owned exit; `on_destroy` stops streaming. |

The menu order is stable: **1 Explore, 2 Countries, 3 Favorites, 4 Recent,
5 Settings, 6 Refresh directory, 7 Next page, 8 Previous page, 9 Station details**.
D-pad left/right pages through the country list. Settings rows are **1 Volume,
2 Play a custom stream, 3 About Frequency, 4 Clear recent history**. Left/right
adjusts the selected Volume row. A on custom stream opens a URL keyboard;
submitting a valid direct HTTP(S) URL explicitly starts it and opens Recent. Custom streams can be
saved from Recent. Details shows the selected record's location, supplied
coordinates, codec, bitrate, tags, languages and homepage; unknowns stay unknown.

## Directory and playback behavior

Discovery, station searches, country searches, mirrors and play-click requests
all use `http.get_async` plus `http.poll`. There are no synchronous HTTP calls.
Frequency follows the [Radio Browser discovery guidance](https://api.radio-browser.info/)
with an asynchronous DNS-over-HTTPS SRV lookup for
`_api._tcp.radio-browser.info` via `dns.google/resolve`. Only port-443 targets
under `.radio-browser.info` with valid hostname labels are accepted. Discovered
hosts are rotated, and fallback seeds (`de1`, `de2`, `nl1`) remain available.
`/json/servers` refreshes the mirror pool as Lua has no native DNS facility.
A query tries at most four distinct mirrors, using the last successful one
first. Malformed JSON or an unexpected response shape is a failed attempt.
Requests have a 14-second application deadline; the shared HTTP transport has
its own timeout. Late replies to timed-out requests are discarded.

Station and country pages are limited to **100 records**, fetched on demand.
A full page enables manual next-page navigation; no background crawl or full
world station download occurs. The app keeps one directory page in memory and
one persistent page cache. A response larger than requested is still capped at
100 records. At most 16 application requests are tracked at once. Request-queue
errors are caught. Filter changes are debounced by 250 ms, use generation IDs,
and discard old responses and old retry chains. Country identity, cache keys and station queries use the
API's `iso_3166_1`/`countrycode`; country names are display labels only. Country
records lacking a valid two-letter code are omitted. Country counts describe the upstream directory, including unsupported
formats.

Only HTTP(S) `url_resolved` streams are used. The original `url` is never used
as a playlist fallback. HLS and playlist URLs, AAC+/HE-AAC, Opus, unknown codecs,
broken stations and missing resolved URLs are omitted. MP3 stations appear first
within each page, preserving popularity order within the MP3 and other-codec
groups. AAC is a **candidate**, not a guarantee: directory labels often do not
distinguish AAC-LC from HE-AAC. Empty compatible results explain the limitation.
The UI describes surviving entries as stream candidates, not verified playback.

Audio uses `audio.stream`, `stream_status`, `pause_stream`, `stop_stream`, and
`set_stream_volume`. Native states drive connecting, buffering, playing, paused,
ended and error displays; errors are not presented as successful playback.
Errors advise retrying or choosing MP3. No fabricated track metadata, waveform,
play count or audible elapsed timer is displayed. `status.seconds` is deliberately
unused because it measures decoded/queued audio rather than exact audible time.
Favorites and Recent work while the directory is unavailable, but listening still
requires a reachable broadcaster.

A best-effort `/json/url/{stationuuid}` request is sent only after an explicit
user play/retry request is accepted by `audio.stream`. It is not sent for browsing,
favoriting, resuming a paused stream, directory retries or custom URLs. No retries
are made for click counting. Stream error/end never causes automatic reconnection.
Recent history records explicit play attempts, including broadcasters that fail.
Exit always stops the native stream.

## Storage schema and automation

App ID: `dev.cartridge.frequency`. All data uses the existing per-app storage API.

`frequency.v1`:

```json
{
  "version": 1,
  "volume": 0.7,
  "favorites": [],
  "recent": [],
  "last_station": null
}
```

Favorites are capped at 100 and Recent at 30, deduplicated by `stationuuid`.
`last_station` is the latest explicit play request and restores selection when
present in the loaded page. It **never autoplays**. Missing or malformed saved
fields are ignored. Empty lists can be serialized as `{}` by the shared Lua JSON
bridge; both forms load safely. Optional nil fields may be absent.

Each stored station is a sanitized object with `stationuuid`, `name`,
`url_resolved`, `codec`, `hls=0`, `country`, `countrycode`, `state`, `language`,
`tags`, `bitrate`, `homepage`, `favicon`, optional finite `geo_lat`/`geo_long`, and
optional `custom=true`. Custom IDs are `custom:` followed by the URL. Coordinates
are retained only as a complete, valid pair within latitude ±90/longitude ±180.
No country centroid is substituted. `favicon` is retained as source metadata;
remote logo images are not fetched or invented.

`frequency.cache.v1`:

```json
{
  "version": 1,
  "key": "\n\nAll sounds\n0",
  "stations": [],
  "more": false,
  "raw_count": 0,
  "skipped": 0,
  "saved_at": 0
}
```

The cache key joins country code, search text, genre label and offset with newline
characters. Only a matching page is shown as cached/stale; changing filters clears
old results. Startup uses the world/All sounds/offset-zero key above. Cached
results are labeled while refreshing and after connection failure. Filters are
not persisted. `saved_at` is Unix time, not a freshness promise. Preferences,
favorites and recents live independently of this cache. Storage writes are protected with `pcall`. The runtime publishes complete JSON
files atomically and surfaces write errors; settings failures show a warning that
changes will last only for the current session.

`app-check --seed` accepts the literal dotted keys shown above. On disk they
are `frequency.v1.json` and `frequency.cache.v1.json` inside the app's `data`
directory; a key spelling change is not needed.

## Sources and artwork

The map is baked from official **Natural Earth v5.1.2**, 1:110m administrative
country GeoJSON, with a full-world equirectangular projection. The exact transform
is `x = 24 + (lon + 180) / 360 * 672`,
`y = 150 + (90 - lat) / 180 * 252`. Source, SHA-256, projection, license and asset
notes are recorded in `assets/attribution.json`. The map is illustrative; the
source's generalized borders imply no position on territorial disputes.

[Natural Earth data is public domain](https://www.naturalearthdata.com/about/terms-of-use/).
The [Radio Browser directory](https://www.radio-browser.info/) is community
metadata; stream content and station marks remain with their respective owners.
The app names both sources in About. The radio dial and markers are original
geometry. They do not impersonate station logos. No browser tile service, map key,
external font icon, or dynamic map rendering is required.

Regenerate assets with Pillow (development dependency only):

```sh
curl -L --fail https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_110m_admin_0_countries.geojson -o /tmp/frequency-map.geojson
python3 lua_cartridges/frequency/tools/render_assets.py /tmp/frequency-map.geojson
```

The generator writes both `assets/icon.png` and the launcher's `icon.png`, plus
`world.png`, two pin PNGs and attribution metadata. The runtime draws only visible
rows and cached images. It uses opaque rectangular primitives, avoiding the
software renderer's SDL_gfx rounded-shape color issue. Error/notice space reduces
the station list to two rows so text does not overlap. There is no continuous
animation and no unconditional redraw from `on_update`; idle is 5 Hz.

## Checks and native screenshots

The independent mlua suite stubs HTTP, storage, keyboard and native audio while
running the actual cartridge modules and lifecycle callbacks:

```sh
cargo test -p cartridge-lua --test frequency
cargo run --bin app-check -- frequency --fixture sim/fixtures/frequency.json --capture 1,12,45 --frames 60
cargo run --bin app-check -- frequency --fixture sim/fixtures/frequency.json --press 10:a --capture 35 --frames 40
```

The fixture is explicitly synthetic: every station is labeled `[fixture]`, stream
URLs use `example.invalid`, and three unsupported-format entries exercise the
filter. It includes mirror discovery, country pages, country-filtered station
pages, successful click replies and a `no-match` empty search. The fixture is a
small visual scenario, not a general search server; other arbitrary names/genres
may match the generic station page. The shared runtime disables broadcaster
network playback in fixture mode and surfaces an explicit offline audio error.
No fixture claims to play actual radio.

Behavior tests cover no autoplay, resolved URLs, MP3 priority, pause/resume/stop,
click timing, playback errors, history reorder, favorites across restart, volume
bounds, persistence shape, corrupt storage, queue saturation, real geographic
positions, unsupported streams, cancellation, URL encoding, stale replies,
malformed JSON, mirror validation/failover, timeouts, ISO country queries,
pagination, oversized pages, custom URLs, keyboard completion after close, and
static idle behavior.

Native `app-check` runs exercised atlas/loading/map, countries, favorites, menu,
settings, About, station details, no-results, directory offline, and fixture audio
error at 720×720. A seeded native offline run also checked persistent favorites,
volume and stale cached records without autoplay. Screenshots were visually inspected after the opaque palette and
error-area fixes. A bounded live directory check returned 100 records, with
86 stream candidates after filtering, 21 provided coordinate pairs and 65
without coordinates at the time of the check. Station availability and counts
will change. Live requests continued to use the shared runtime's descriptive
`Cartridge/0.1.0` User-Agent; Lua currently cannot override request headers.

## Practical limits

The native backend currently supports MP3, AAC-LC, Ogg Vorbis, FLAC and WAV/PCM,
stereo up to 96 kHz. It does not support HLS, HE-AAC or Opus. Broadcaster access,
TLS, redirects, incorrect directory codec labels and region restrictions can still
prevent playback; choose another station when a retry fails. No promise is made
that every candidate is decodable or continuously online. Search is by station
name, with separate country and curated genre filters. Map navigation covers pins
on the current page, without zoom or clustering. Refresh and paging are explicit.

A live end-to-end run explicitly played Sports Radio Brila FM (an MP3 candidate),
then paused, resumed and stopped it. Native screenshots captured `CONNECTING`,
`PAUSED`, `PLAYING`/`ON AIR` and `STOPPED`; the initial frame was stopped. This
verifies the app-to-native state path, not a subjective listening assessment or
a long-duration stream soak. The decoded MP3 transport emitted two initial
reservoir-underflow warnings, recovered, and reached playback. The local 200-frame
run presented 18 frames with 7.18 ms average rendered work and a cold-frame maximum
of 34.75 ms. A later cold screenshot run reached 101.52 ms maximum rendered work under the
concurrent desktop workload. These are desktop debug-build numbers, not device
guarantees; the provisional cold-frame performance target is not established.

The native engine's transport and device audio tests belong to the shared runtime.
Desktop screenshots and timing checks do not establish handheld performance,
battery consumption, physical-panel readability or continuous-stream stability.

## Changed files

Only these Frequency-owned files were added:

```text
lua_cartridges/frequency/cartridge.json
lua_cartridges/frequency/main.lua
lua_cartridges/frequency/directory.lua
lua_cartridges/frequency/station.lua
lua_cartridges/frequency/view.lua
lua_cartridges/frequency/icon.png
lua_cartridges/frequency/assets/icon.png
lua_cartridges/frequency/assets/world.png
lua_cartridges/frequency/assets/pin.png
lua_cartridges/frequency/assets/pin-selected.png
lua_cartridges/frequency/assets/attribution.json
lua_cartridges/frequency/tools/render_assets.py
crates/cartridge-lua/tests/frequency.rs
sim/fixtures/frequency.json
docs/frequency.md
```
