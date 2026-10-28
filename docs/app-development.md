# Build your first Cartridge app

Build a small app on your Mac before using a handheld. This guide targets the
public stable **CartridgeOS v0.7.1** runtime. The finished app counts A presses,
resets with B and saves its count locally. You can run and edit it without
installing anything on a device or preparing an SD card.

The **runtime repository, desktop simulator and documented Lua APIs are the
SDK**. There is no separate proprietary devkit to obtain. The runtime is Rust
with SDL2; your app is a directory containing a JSON manifest, Lua code and any
assets. Lua 5.4 is embedded in the runtime, so a separate Lua installation is
not needed. See the [complete starter](../examples/hello-cartridge/),
[Lua API reference](lua-api.md) and [runtime source](../crates/cartridge-lua/src/).

## 1. Set up a Mac and pin the runtime

Have Git, Python 3, [Homebrew](https://brew.sh/) and stable Rust installed
([rustup](https://rustup.rs/) provides `rustc` and `cargo`). Rust builds need the
macOS Command Line Tools; if they are missing, run `xcode-select --install` and
finish that installation first. `sim.sh` uses Python 3 to generate its isolated
game fixture. Check that the tools are on your terminal's path:

```bash
git --version
python3 --version
rustc --version
cargo --version
```

Install the same SDL dependencies listed in the [project README](../README.md#building):

```bash
brew install sdl2 sdl2_ttf sdl2_image sdl2_gfx
```

Clone the release tag, which keeps the runtime at the API version used here:

```bash
git clone --depth 1 --branch v0.7.1 https://github.com/Strizzo/Cartridge.git Cartridge
cd Cartridge
```

Git's detached-HEAD notice is expected for a tag checkout. The commands below
run from this `Cartridge` directory. The first simulator run compiles the native
runtime and may take a while; later Lua edits reuse it. This is a desktop build;
device installation is a separate workflow in [INSTALL.md](../INSTALL.md).

## 2. Get and run the starter

The starter is included in the release checkout:

```bash
./sim.sh app examples/hello-cartridge
```

You should see a 720×720 window showing **Hello, Cartridge** with a count of 0.
Use the controller's face buttons or these keyboard equivalents:

| Controller | Desktop key | Starter action |
| --- | --- | --- |
| A | Z | Increment once per press |
| B | X | Reset to zero |
| Select | Space | Exit the app |
| D-pad | Arrow keys | Navigation in other apps |
| X / Y | C / V | Available for your own actions |
| Start | Return | Available for your own menu |
| — | Esc | Quit the simulator |

Press and release Z twice: the count becomes 2. Press X: it becomes 0. Hold Z:
it increments only once because the app ignores `repeat` and `release` events.
Select is reserved by the runtime and never reaches the app's `on_input`.
[Simulator controls and options](simulator.md) cover shoulder buttons, gamepads,
screenshots and window scaling.

## 3. Understand the two files

The [manifest](../examples/hello-cartridge/cartridge.json) identifies the app,
selects its Lua entry point and declares permissions:

```json
{
  "id": "dev.example.hello-cartridge",
  "name": "Hello, Cartridge",
  "version": "0.1.0",
  "author": "Your Name",
  "description": "A first Lua app: press A to count, B to reset, and keep your count locally.",
  "category": "tools",
  "tags": ["example", "tutorial"],
  "permissions": ["storage"],
  "min_runtime": "0.7.0",
  "entry": "main.lua"
}
```

`version` is your app's version; `min_runtime` is the minimum CartridgeOS
version, a plain semantic version such as `0.7.0`. The runtime checks it when
loading the manifest. Keep this starter's minimum at `0.7.0`, the API baseline
used by this starter. Use v0.7.1 or newer for the runtime security fixes. Use a unique ID for a new app: storage is scoped by
that ID. Request only permissions you use. Here, `storage` enables
`storage.load` and `storage.save`; without it the `storage` table is absent.
Drawing, input callbacks, `theme`, `ui`, `json` and `app` need no permission.

Read [`main.lua`](../examples/hello-cartridge/main.lua) from top to bottom:

| Callback | What this starter does |
| --- | --- |
| `on_init()` | Load and validate the saved counter; set idle updates to 1 Hz. |
| `on_input(button, action)` | Accept A/B presses, update state and save only when the count changes. |
| `on_update(dt)` | Return `false`: there is no animation or background work. `dt` is elapsed seconds. |
| `on_render()` | Draw the title, count, storage status and button hints. All `screen.*` calls belong here. |
| `on_destroy()` | Nothing to close in this app; release resources here in larger apps. |

The counter is bounded to 999999 to fit the display. A save uses
`pcall(storage.save, "counter", {count = count})`: a write failure shows a
warning and leaves the app usable with its count in memory. A successful save
is JSON scoped to this app. Saving on changes avoids disk writes during idle
updates or compatibility renders.

After a change, `app.request_redraw()` marks the screen dirty. The runtime
already invalidates on input, but explicit redraw is useful when you later
change state from deferred work. Returning `true` from `on_update` also requests
a redraw. Do that only when visible content changes. In v0.7.0, interaction
updates run at up to 30 Hz; after three seconds without input they drop to 5 Hz
by default, or 1 Hz as this starter requests. Input wakes the pacing wait.
Clean frames skip drawing, with a one-second compatibility refresh. A static app
therefore still renders occasionally; it should not request continuous animation.

## 4. Edit, reload and check persistence

Keep the simulator open and change `"Hello, Cartridge"` in `main.lua` to your
own title. Saving a `.lua` file triggers hot reload in about a second. The runtime
calls `on_destroy()`, creates a fresh Lua VM and calls `on_init()` again. Local
variables reset; this app restores its saved counter. Changes to the manifest
or assets require exiting and running the command again.

Count a few presses, exit with Space and relaunch. The count should survive.
By default `sim.sh` sets `CARTRIDGE_HOME` to the checkout's gitignored
`.sim/home`. The starter's saved value is at:

```text
.sim/home/.cartridges/dev.example.hello-cartridge/data/counter.json
```

This simulator data is separate from your normal `~/.cartridges` and from the
handheld. Runs using the same simulator home and app ID share the saved count.
For a fresh isolated session, choose a new home:

```bash
./sim.sh app examples/hello-cartridge --home .sim/hello-fresh
```

For a repeatable check, the existing `app-check` harness drives real lifecycle
callbacks and captures three screens using fresh temporary app storage:

```bash
cargo run --bin app-check -- examples/hello-cartridge \
  --frames 40 --press 5:a --press 10:a --press 20:b \
  --capture 0,12,25 --out .sim/hello-check
```

Inspect `frame_0000.png`, `frame_0012.png` and `frame_0025.png` under
`.sim/hello-check`: their counts should be **0, 2, 0**. The command fails on Lua
errors and missing captures; it does not assert the text in a screenshot.
It removes its temporary storage afterwards and leaves your interactive count
alone. On macOS hidden SDL windows still require WindowServer access. See
[testing tools](testing.md) for broader scenarios and visual regression checks.

If an API is nil, check the manifest permission and restart after editing it.
For Lua errors, read the error panel and terminal output. If `screen.*` reports
that it can only be called during `on_render`, move the drawing or text
measurement there. If Rust cannot link SDL, recheck the Homebrew dependencies
and that your Rust toolchain and Homebrew libraries use the same architecture.

## 5. Grow the app without blocking input

For a network app, add `network` to the manifest, restart, then start work with
`http.get_async` or `http.post_async`. Keep the request ID and consume results
with `http.poll()` in `on_update`. Show loading/error/cached content, keep Back
responsive, and ignore results for views the user has left. There is no HTTP
cancel method in v0.7.0; ignoring an old request's result is a view-level policy.
Use `pcall` when submitting because the queue is bounded. Avoid synchronous
`http.get`, `http.post` and `http.get_cached` in interactive callbacks: they block
even in `on_init` or `on_update`. Large JSON decoding also runs on the UI thread.

The [async HTTP reference](lua-api.md#http) gives the real signatures and result
fields; [`ai_papers/main.lua`](../lua_cartridges/ai_papers/main.lua) shows request
IDs, polling and incremental views. Use [HTTP fixtures](simulator.md#flags) to
check slow/offline states. Adding a network permission allows real requests;
simulated Wi-Fi being off alone does not prevent them.

Design around a controller and a 720×720 logical canvas: make focus visible,
label the face-button actions and use readable type with generous spacing.
Test `./sim.sh app examples/hello-cartridge --true-size`; its physical scaling
is approximate, so check the real panel before release. Use cached PNG/JPG
assets through `screen.draw_image` for complex artwork. Image and font/text
caches belong to the runtime; load/draw only needed assets, bound their sizes
and avoid generating artwork or recomputing layout on every draw.

Useful next examples are [calculator](../lua_cartridges/calculator/) for button
navigation, [Todo](../lua_cartridges/todo/) for storage and text entry, and
[Weather](../lua_cartridges/weather/) for cached illustrations and network views.
The earlier [Hello World](../lua_cartridges/hello_world/) example adds drawing
primitives and audio. The [Lua API reference](lua-api.md) covers lifecycle,
permissions, modules and the restricted `require` for splitting your app into
several `.lua` files.

## 6. Keep your app in its own repository and publish a release

Copy the starter to your own repository and change its ID, name, author and
description. Keep the runtime checkout for development; it accepts external app
directories directly:

```bash
cp -R examples/hello-cartridge ../my-cartridge
./sim.sh app ../my-cartridge
```

Commit your Lua, manifest and assets in that app repository. Apps can ship as
independent versioned release packages; they do not need a runtime rebuild.
Follow the [Store publishing guide](app-store.md#developing-an-app) and the
release workflows in an existing app such as
[Outside](https://github.com/Strizzo/outside-cartridge) for packaging and
validation. A Store release has an explicit HTTPS package URL, SHA-256, exact
byte size, permissions and minimum runtime version. Keep manifest and release
metadata consistent and test the package you intend to distribute.

For the official Store, propose your app/version in
[`Strizzo/cartridge-apps`](https://github.com/Strizzo/cartridge-apps) for
**maintainer review**. Maintainers approve the package and metadata, then sign
the official catalogue. Contributors do **not** receive the catalogue private
key; your app repository and release workflow do not need it. Changing this
runtime's bundled `registry.json` does not publish a downloadable Store app.
See [app-store.md](app-store.md) for the current catalogue and signing policy.

## Performance and device validation

The rest of this guide covers budgets, assets and validation as your app grows.

Use Lua for small applications that share Cartridge's drawing, input, storage,
network and audio APIs. Use Rust for CPU-heavy parsing, image processing or new
platform services; expose a small API to Lua instead of running that work once
per draw call. Existing Linux programs/emulators can run as separate processes:
release Cartridge's SDL window first and restore the selected app/game on exit.
The game library implements that lifecycle already.

For expensive remote tasks (for example AI inference or building software), keep
the computation on a server and make the handheld a responsive client. Cache the
last useful result, display connection state and allow Back while requests run.
The v0.7.0 SSH API includes asynchronous connection and command jobs; poll their
results from `on_update`. Legacy synchronous SSH methods still block. See the
[SSH reference](lua-api.md#ssh) before building a remote-control app.

## The graphics API does not dictate the layout

SDL2 provides a 720×720 canvas, fonts, image textures and drawing primitives.
Cartridges can place content anywhere, use original raster icons/sprite sheets,
change typography and construct their own controls. There is no mandatory widget
layout. `ui.header`, `ui.footer`, `ui.card`, `ui.rect` and `ui.pill` are optional
helpers that keep bundled apps consistent with the launcher. In the Neo theme
these use flat fills, clear outlines and a condensed display face. Raw `screen.*`
primitives remain available for custom graphics.

Prefer opaque fills and cached image/text textures. The accelerated renderer can
scale/blit textures, but numerous tiny Lua calls, repeated text wrapping and
scanline-based rounded shapes/gradients still cost CPU time. Avoid full-screen
alpha overlays, continuously changing text and animations with no user purpose.
Pre-render complex artwork as an image, draw only visible list rows, downsample
charts to their pixel width and recompute layouts when content/width changes.

Check font sizes with `./sim.sh --true-size` as well as 720×720 screenshots. The
physical-size option is approximate and depends on the Mac display information;
confirm readability once on the actual panel. A large desktop window is not a
substitute for that check.

## Runtime behavior and initial budgets

These are engineering targets, not measured RK3326 performance guarantees.
Neither the Mac nor the ARM compatibility VM reproduces its GPU, memory timings,
power draw or thermal limits.

| Area | Current behavior / development target |
| --- | --- |
| Input and updates | Up to 30 Hz during interaction; avoid blocking calls in every lifecycle callback. |
| Idle | After 3 seconds, updates drop to 5 Hz by default. SDL input interrupts the pacing wait immediately. Draw only when dirty, with a 1-second compatibility refresh. |
| Redraw | Input, completed HTTP, hot reload, explicit requests and a true update return value invalidate the frame. |
| Frame work | Provisional device target: p95 below 33 ms during navigation; aim below 20 ms to leave headroom. Validate release binaries on the handheld. |
| Network | Four workers, at most 64 outstanding requests, at most 8 completions per poll; text bodies limited to 4 MiB each. Use much smaller, paginated responses where possible. |
| Memory | Interactive frame timing retains 1024 samples per stream. Bound app lists, decoded images and history; measure process RSS across repeated open/close cycles. No total per-app RAM limit is enforced. |
| Animation | Request redraw only while something changes; static screens should not redraw at 30 Hz. |

`app.request_redraw()` or `return true` from `on_update` marks changed content.
`app.set_idle_fps(n)` adjusts the idle update cadence (1–60); raising it increases
background work. Timers should use `dt` and request a redraw only when their
visible value changes. Leave input polling to the runtime.

Bundled network apps use asynchronous requests and polling. Start requests once,
keep request IDs, ignore responses belonging to a cancelled view, and retry only
when a previous attempt finished. Catch queue-full errors using `pcall` and retry
later. Parsing and Lua callbacks still run on the UI thread: asynchronous transfer
does not make a large JSON decode or HTML layout free. Legacy synchronous HTTP
and SSH methods still block. Store downloads use
a background worker with verified staged installation; see [App Store](app-store.md).

## Iterate without an SD-card swap

```bash
./sim.sh                                  # real launcher, isolated settings
./sim.sh app lua_cartridges/ai_papers --fixture sim/fixtures/http.json --no-fps
./sim.sh check
./sim.sh check --profile sim/profiles/low-battery-offline.json
cargo test -p cartridge-lua --test network_apps
CARTRIDGE_SIM=1 CARTRIDGE_HOME="$PWD/.sim/home" cargo run --release --bin perf-bench -- app
```

Lua edits hot reload in about a second. Fixture mode opens no HTTP sockets for app
requests; the supplied JSON includes synthetic papers, delayed reader content,
DNS and probe results. Unmatched URLs fail offline. Use a separate fixture file
for API failures, longer delays, empty results and large lists. The simulated
Wi-Fi status alone does not disable ordinary app HTTP: choose fixtures explicitly
when you need a repeatable offline run.

`sim.sh check` drives real input and render callbacks and fails on Lua errors.
It checks loading/data/offline screens and verifies a static Todo app skips clean
redraws. `network_apps` tests cancellation, out-of-order results, incremental
updates, retries and refresh deduplication. These behavioral checks complement
screenshot review rather than replacing it.

The performance harness reports rendered-frame work separately from clean idle
iterations. It excludes screenshot encoding and frame pacing, but includes
updates/input on a rendered iteration and any wait inside SDL presentation.
Presentation rate means actual presents divided by elapsed wall time; it is not
`1000 / CPU-work-ms`. Full sample history is retained only in explicitly bounded
checks. Release thresholds compare rendered-frame p95, including cold draws.

After local checks, push the isolated branch and run its successful CI ARM bundle
through `./sim/vm.sh check --run ID`. This tests ARM/Linux execution, real rendering
and systemd lifecycle in a disposable VM. See [ARM VM](arm-vm.md).

Use [wireless deployment](wireless-deploy.md) for the final device gate. Record
build revision, renderer, p95/max frame work, RSS, input/launch behavior, idle
battery behavior and any temperature/governor changes under the same workload.
Do not force a permanent performance governor to hide inefficient idle work.
Keep the reversible EmulationStation fallback throughout device validation.

## Visual assets and repeatable state

Weather's current and forecast views use original warm-white/red illustrations,
a large display face and static cards. Ten 192px condition PNGs are cached on first
use; no runtime SVG parsing or animation is needed. `scripts/render_weather_icons.py`
regenerates the filled SVG sources and PNGs (ImageMagick is a developer dependency
only). Condition codes follow the [Open-Meteo table](https://open-meteo.com/en/docs).

`sim/fixtures/populated.json` supplies synthetic news, stocks and weather as well
as the basic paper/network fixtures. The app checks cover current/forecast/city
changes, news detail and stock detail/period changes. Every scenario gets its own
temporary storage directory, removed afterwards; it does not alter interactive
simulator settings. This includes successful storage writes such as a city change.

The SDL input check sends actual key events from a worker during a one-second
idle wait. It verifies early wake, ordered press/release delivery exactly once,
and waiting when no input is present. Host wake timings validate this code path;
they do not measure the handheld's complete button-to-display latency.
