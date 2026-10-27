# Hello, Cartridge

A complete first app for **CartridgeOS v0.7.0**: A increments a counter, B
resets it, and local storage keeps the value across exits and Lua hot reloads.
It has no animation or network requests. The manifest grants only `storage`.

For Mac setup, the pinned runtime checkout and a walkthrough, follow
[Build your first Cartridge app](../../docs/app-development.md).

From the runtime repository root, with Rust and the README's SDL libraries:

```bash
./sim.sh app examples/hello-cartridge
```

On the desktop, **Z = A**, **X = B**, **Space = Select (exit)**, and **Esc =
quit**. Press and release Z twice: the count should be 2. X resets it to 0.
Holding Z does not keep counting. A USB gamepad can use the corresponding
buttons. Counts stop at 999999 so the number stays readable.

After counting, exit and run again to see the saved value. With the default
simulator home, it lives at
`.sim/home/.cartridges/dev.example.hello-cartridge/data/counter.json`.
For a separate fresh count, use a new simulator home:

```bash
./sim.sh app examples/hello-cartridge --home .sim/hello-fresh
```

Edit the title in `main.lua` while the app is running. About a second later,
hot reload creates a fresh Lua VM and runs `on_init()`; the saved counter
survives. Restart the app after changing `cartridge.json` or assets.

`on_input` saves only changed values and requests a redraw. `on_update`
returns `false`; `app.set_idle_fps(1)` reduces updates after three seconds
without input. The runtime still performs its one-second compatibility
refresh. `on_render` only draws. Save errors show a warning while the app
continues with its count in memory.

The existing app harness can exercise the real Lua runtime and capture the
initial, incremented and reset screens, using temporary app storage:

```bash
cargo run --bin app-check -- examples/hello-cartridge \
  --frames 40 --press 5:a --press 10:a --press 20:b \
  --capture 0,12,25 --out .sim/hello-check
```

Expect counts **0, 2, 0** in `frame_0000.png`, `frame_0012.png` and
`frame_0025.png`. The harness fails on Lua errors; inspect the captures for
the displayed values and layout. On macOS even hidden SDL windows need
WindowServer access. See [testing](../../docs/testing.md) for the existing
broader checks.

Copy this directory to your own app repository when ready, then change the
manifest's ID, name, author and description. Keep `min_runtime` at `0.7.0`
for this tutorial's supported baseline and declare any additional
permissions your app needs. See the [API reference](../../docs/lua-api.md)
and [Store publishing guide](../../docs/app-store.md) for next steps.
