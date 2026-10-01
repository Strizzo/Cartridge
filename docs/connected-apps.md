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

## Verification record — 2026-10-01

Runtime/app implementation: `5a667eb66ebf97598974ae0785e6509347326840`.

- 52 Rust unit/integration tests passed on macOS, plus the opt-in test using
  the real VibeBoy HTTP handlers. The legacy snapshot test is skipped in CI
  mode; the separate native simulator suite below supplies render coverage.
- All native simulator scenarios passed, including disconnected apps, posted
  command acknowledgments and keyboard cancellation without exiting an app.
- 86 Python installer/session regressions passed.
- Frequency fetched a live directory and played Sports Radio Brila FM through
  its native controller UI, including pause/resume/stop. The integrated player
  also played SomaFM Groove Salad at zero volume for a silent output check.
- Outside loaded a live Open-Meteo forecast and rendered the source location's
  local night scene. Native fixture checks covered all illustrated conditions,
  locations, unit changes, stale/offline data and search failures.
- Mission Control sent an approved response through the real VibeBoy HTTP API,
  ActionExecutor and libtmux to a disposable terminal in the ARM Linux VM. The
  terminal received the exact text and the app displayed its returned output.
  No real user session was controlled. The disposable service/pane was stopped.
- The shared keyboard now cancels on Select, handles Unicode defaults, scrolls
  long input, and offers a symbols page. Storage writes publish complete JSON
  and report write failures instead of silently claiming to save.

Native 720×720 captures (live data; terminal is an isolated test):

| Frequency | Mission Control | Outside |
| --- | --- | --- |
| [Radio playback](screenshots/connected-apps/frequency.png) | [Terminal delivery](screenshots/connected-apps/mission-control.png) | [Live forecast](screenshots/connected-apps/outside.png) |

Linux and ARM build evidence is available in [the implementation CI run](https://github.com/Strizzo/Cartridge/actions/runs/36916936408).
The actual ARM bundle passed the isolated Linux VM checks: native simulator
scenarios, all 86 Python tests, library loading, offline-image preparation and
systemd startup/handoff/failure/undo. The test package now includes its splash
fixture, and developer binaries regain executable permissions after ZIP extraction.
The tested artifact was built from GitHub's PR merge `2faa4b6e3b7bc5d9daa0238f68a129b19a172cd4`;
its Cartridge executable SHA-256 is `3e8a92e12d922b35b3a5d6c329d007c55dbc1ec2e2f4017c5dfd62becf6907b5`.
Reproduce with `./sim/vm.sh check --run 36916936408`. This establishes Linux/ARM
compatibility, not RK3326 timing, Wi-Fi hardware, physical audio or battery life.

## Hardware acceptance

Before calling these apps device-validated, test the resulting bundle on the
handheld: launch/exit each app with the actual controller; sustain radio playback
and station switching; open weather locations; send a disposable terminal response;
measure input/render time, memory and idle power. No SD card was written during
this app implementation. HLS/HE-AAC/Opus and local speech/LLM inference are outside
these apps' supported features. Mission Control's server acknowledgment still
requires checking terminal output to confirm the requested operation happened.
