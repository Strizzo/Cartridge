# Developing apps for the handheld

Use Lua for small applications that share Cartridge's drawing, input, storage,
network and audio APIs. Use Rust for CPU-heavy parsing, image processing or new
platform services; expose a small API to Lua instead of running that work once
per draw call. Existing Linux programs/emulators can run as separate processes:
release Cartridge's SDL window first and restore the selected app/game on exit.
The game library implements that lifecycle already.

For expensive remote tasks (for example AI inference or building software), keep
the computation on a server and make the handheld a responsive client. Cache the
last useful result, display connection state and allow Back while requests run.
The existing SSH tunnel API can support this, but connection setup is synchronous
and still needs an asynchronous lifecycle before it can be treated as stall-free.

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
| Idle | After 3 seconds, updates drop to 5 Hz by default. Draw only when dirty, with a 1-second compatibility refresh. |
| Redraw | Input, completed HTTP, hot reload, explicit requests and a true update return value invalidate the frame. |
| Frame work | Provisional device target: p95 below 33 ms during navigation; aim below 20 ms to leave headroom. Validate release binaries on the handheld. |
| Network | Four workers, at most 64 outstanding requests, at most 8 completions per poll; text bodies limited to 4 MiB each. Use much smaller, paginated responses where possible. |
| Memory | Interactive frame timing retains 1024 samples per stream. Bound app lists, decoded images and history; measure process RSS across repeated open/close cycles. No total per-app RAM limit is enforced. |
| Animation | Request redraw only while something changes; static screens should not redraw at 30 Hz. |

`app.request_redraw()` or `return true` from `on_update` marks changed content.
`app.set_idle_fps(n)` adjusts the idle update cadence (1–60); raising it increases
background work. Timers should use `dt` and request a redraw only when their
visible value changes. Leave input polling to the runtime.

All bundled network apps now use `get_async` and `poll`. Start requests once,
keep request IDs, ignore responses belonging to a cancelled view, and retry only
when a previous attempt finished. Catch queue-full errors using `pcall` and retry
later. Parsing and Lua callbacks still run on the UI thread: asynchronous transfer
does not make a large JSON decode or HTML layout free. The synchronous compatibility
HTTP methods and store/download/SSH paths remain separate work to audit.

## Iterate without an SD-card swap

```bash
./sim.sh                                  # real launcher, isolated settings
./sim.sh app lua_cartridges/ai_papers --fixture sim/fixtures/http.json --no-fps
./sim.sh check
./sim.sh check --profile sim/profiles/low-battery-offline.json
cargo test -p cartridge-lua --test network_apps --test http_runtime
CARTRIDGE_SIM=1 CARTRIDGE_HOME="$PWD/.sim/home" cargo run --release --bin perf-bench -- app
```

Lua edits hot reload within a second. Fixture mode opens no HTTP sockets for app
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
The current branch has not yet been measured or installed on the replacement
card. Keep the reversible EmulationStation fallback throughout validation.
