# Testing & Performance Tooling

CartridgeOS ships with a headless test harness, perf bench, and snapshot
diff tool — so you can iterate on the launcher without flashing a device.

## Tools

### `cargo run --bin snapshot`

Runs the launcher headlessly through scripted scenarios and dumps PNG
captures of each UI screen to `snapshots/`. By default captures: home,
store, settings, app_detail.

```bash
cargo run --bin snapshot
ls snapshots/
# -> home.png, store.png, settings.png, app_detail.png

cargo run --bin snapshot home settings    # Only specific screens
```

Useful for:
- Reviewing UI changes without flashing the device
- Generating images for documentation
- Producing inputs for the snapshot diff test

### `cargo run --bin perf-bench [scenario]`

Runs the launcher headlessly through a bench scenario and prints
frame-time statistics. Exits with non-zero status on regression.

```bash
cargo run --bin perf-bench --release          # default: home
cargo run --bin perf-bench --release navigate # walk through dock + zones
cargo run --bin perf-bench --release store    # open store, scroll
```

Output:
```
=== Bench: home ===
  wall time  : 4.39s (600 frames)
  uncapped   : 137 fps
  frame ms   : min=6.98  avg=7.31  p95=7.47  max=41.56
  text cache : 30543 hits / 57 misses (99.8% hit, 57 entries)
```

Threshold: defaults to `p95 < 30ms` for release, `< 200ms` for debug.
Override with `CARTRIDGE_BENCH_P95_MS=50`.

### `cargo test --test snapshot_test`

Visual regression test: regenerates snapshots and pixel-diffs them
against committed baselines in `tests/baseline/`. Allows up to 1% pixel
drift to tolerate font anti-aliasing and dynamic content.

```bash
# Run normally
cargo test --test snapshot_test

# After an intentional UI change, update baselines
UPDATE_SNAPSHOTS=1 cargo test --test snapshot_test
```

The test invokes the snapshot binary as a subprocess (cargo test's panic
handler is incompatible with SDL2 on macOS).

## Environment Variables

| Variable                     | What it does                                           |
| ---------------------------- | ------------------------------------------------------ |
| `CARTRIDGE_FPS=1`            | Show on-screen FPS / frametime / cache stats overlay   |
| `CARTRIDGE_HIDDEN=1`         | Create the SDL window hidden (for headless tests)      |
| `CARTRIDGE_SOFTWARE=1`       | Use software renderer (read_pixels works reliably)     |
| `CARTRIDGE_BENCH_VISIBLE=1`  | Show the window during perf bench (visual debug)       |
| `CARTRIDGE_BENCH_P95_MS=50`  | Override the regression threshold for perf bench       |
| `UPDATE_SNAPSHOTS=1`         | Update baselines instead of failing on diffs           |
| `RUST_LOG=cartridge=info`    | Verbose logging for the launcher                       |
| `CARTRIDGE_HOT_RELOAD=1`     | Re-run a Lua cartridge when its `.lua` files change    |

## Iterating on a Lua Cartridge

```bash
# Run a cartridge with hot reload — edits to .lua files trigger a
# fresh VM with on_init() called again. Useful for tight UI iteration.
CARTRIDGE_HOT_RELOAD=1 cargo run -- run --path lua_cartridges/hello_world
```

The watcher polls every second for file mtime changes anywhere in the
cartridge directory. Lua state is discarded across reloads (as if the
cartridge had been quit and re-launched).

## Iterating on UI Changes

A typical workflow for a UI tweak:

```bash
# 1. Run snapshot, view current state
cargo run --bin snapshot
open snapshots/home.png

# 2. Make your code change

# 3. Re-run snapshot, compare visually
cargo run --bin snapshot
open snapshots/home.png

# 4. Run perf bench to confirm no regression
cargo run --bin perf-bench --release

# 5. Update baselines if change is intentional
UPDATE_SNAPSHOTS=1 cargo test --test snapshot_test
git diff tests/baseline/   # review what changed
```

## Known Limitations

- macOS only for now (Linux requires running tests with a display server).
- Software rendering is slower than the device's accelerated path, so
  perf-bench numbers aren't directly comparable to the device — but they
  are consistent across runs, which makes them useful for spotting
  regressions.
- The launcher must run with hidden window + software renderer for tests.
  Production runs unchanged (default to visible + accelerated).
