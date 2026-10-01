# Connected Cartridge apps

The requested deliverable is three finished handheld apps: **Frequency**,
**Mission Control**, and **Outside**. All run through the native 720×720 Lua/SDL
runtime, have controller navigation, and ship with the Cartridge application
bundle. They are not browser mockups. Work stays on `codex/cartridge-primary`.

## Acceptance requirements

| App | Required end-to-end behavior |
| --- | --- |
| Frequency | Discover real radio stations geographically and by genre/search; show trustworthy station attributes; tune actual streams without freezing the UI; pause/stop/volume; save favourites and restore settings; handle mirror failure, missing coordinates, offline directory cache and unavailable/unsupported stations. |
| Mission Control | Configure and persist a real VibeBoy server; poll real session/agent state; navigate projects, session details and recent output; execute supported contextual actions through the existing API with visible acknowledgments/errors; remain usable while disconnected; avoid overlapping or stale requests. |
| Outside | Search and save locations; fetch real current/hourly/daily forecasts; illustrate selected weather and local day/night; scrub hours and days; read detailed conditions; change units; restore settings and cached weather with accurate stale/error indicators. |

Shared completion requires meaningful lifecycle/state tests, review of rendered
720×720 screens, native simulator navigation and error scenarios, real API
checks, Linux ARM compatibility, and explicit identification of any remaining
hardware-only acceptance. Tests must exercise user behavior and failure recovery,
not only confirm that each manifest loads. A scripted screenshot does not prove
real audio, a real server command, or handheld performance.

## Development and verification

Use `./sim.sh app lua_cartridges/<app>` for live operation. The native simulator
uses the same application and rendering code as the handheld; it does not emulate
RK3326 performance. `--fixture sim/fixtures/<app>.json` selects deterministic
HTTP data and disables real internet audio. See each app's README and the Lua API
reference for controls, endpoints, permissions and streaming-format limits.

A streaming probe is available independently of the UI:

```sh
cargo run -p cartridge-lua --example stream_probe -- https://ice1.somafm.com/groovesalad-128-mp3 5 0
```

The last argument is volume; zero verifies the real audio pipeline silently.
Network, decoding and audio output passed this MP3 probe on the Mac during
implementation. Automated tests also cover a stalled HTTP stream cancelled by
stop/drop, offline audio isolation, and non-seekable MP3/AAC-LC/Vorbis decoding
using short generated sine-wave fixtures. These observations do not establish
handheld audio compatibility or battery use.

Implementation and integration are in progress. Do not treat this document as a
release sign-off; final checks must be recorded against the completed code.
