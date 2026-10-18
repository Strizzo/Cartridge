# CartridgeOS

An experimental app platform developed and tested on an R36S Plus. Turn a retro gaming handheld into a general-purpose pocket computer with WiFi, apps, and a d-pad. Other devices and firmware variants have not been validated.

Built in Rust with SDL2. Apps are scripted in Lua.

> **Experimental software — back up your card before installing.** CartridgeOS is
> provided as-is, without warranty. Installation and boot changes can cause data
> loss or leave the card unbootable. Keep an independent backup of your working
> system, games and saves, and preferably experiment on a spare card. A similar
> product name does not guarantee compatible hardware or firmware. See
> [installation, SD-card guidance and recovery](INSTALL.md) before proceeding.

## See it running

[![CartridgeOS running on a green R36S Plus](docs/media/overview-poster.jpg)](docs/media/overview.mp4)

**[Watch the new 36-second hardware overview](docs/media/overview.mp4)** · [Photos, focused demos and simulator gallery](docs/media/README.md)

New R36S Plus footage: launcher navigation, radio-map zooming, illustrated weather, news, focus timer and system stats. The overview is edited and muted; the gallery clearly labels simulator screenshots that use sample data.

## Install on R36S Plus

The [v0.7.0 release](https://github.com/Strizzo/Cartridge/releases/tag/v0.7.0)
contains an application bundle for an **existing, working R36S Plus system**.
It is not a complete SD-card image. The macOS image installer remains a
prototype; it is not a general-purpose public installer.

1. Back up your working card, games and saves. Download
   `cartridge-r36s-plus.zip` from that release.
2. Power off the handheld, insert its card into your computer, and open the
   existing ROMS volume (often labelled `EASYROMS`).
3. Extract the `Cartridge/` folder into the root of that volume. Copy the three
   scripts from `Cartridge/tools/` into its existing `tools/` folder, keeping
   other tools and games in place. See [INSTALL.md](INSTALL.md) for the layout
   and archive checksum.
4. Safely eject, boot normally into EmulationStation, then open
   **Options → Tools → Cartridge** and confirm the launcher works.
5. Only after that check, optionally run **Tools → Setup Cartridge Boot**.
   In v0.7.0 this enables direct Cartridge startup at the next normal boot,
   with EmulationStation retained as fallback. Setup does not reboot for you.

[INSTALL.md](INSTALL.md) explains returning to EmulationStation, the recovery
file, and the limits of this tested setup. Do not follow legacy boot-selector
instructions for this release.

**Source and documentation:** the default `main` branch still contains the older
runtime. To work with the version shown in the current demos, check out
[`v0.7.0`](https://github.com/Strizzo/Cartridge/tree/v0.7.0) and use its
[simulator guide](https://github.com/Strizzo/Cartridge/blob/v0.7.0/docs/simulator.md)
and [app development guide](https://github.com/Strizzo/Cartridge/blob/v0.7.0/docs/app-development.md).

## Architecture

Cargo workspace with the following crates:

| Crate | Description |
|-------|-------------|
| `cartridge-core` | Core types: screen, input, theme, storage, fonts |
| `cartridge-runner` | SDL2 game loop, window management, rendering |
| `cartridge-lua` | Lua scripting engine (app lifecycle, API bindings) |
| `cartridge-net` | HTTP client and networking for Lua apps |
| `cartridge-launcher` | On-device launcher UI and app switching |
| `cartridge-boot` | Device bootstrap and EmulationStation integration |

The top-level `cartridge` binary ties everything together as the single entry point.

## Building

Requires SDL2, SDL2_ttf, and SDL2_gfx development libraries:

```bash
# macOS
brew install sdl2 sdl2_ttf sdl2_gfx

# Debian/Ubuntu
sudo apt-get install libsdl2-dev libsdl2-ttf-dev libsdl2-gfx-dev
```

Then build:

```bash
cargo build --release
```

## Running

```bash
# Launch the launcher (default)
cargo run

# Run a specific Lua app
cargo run -- run --path cartridges/calculator

# Show the demo screen
cargo run -- demo
```

### Controls (Desktop)

| Key | Button | Action |
|-----|--------|--------|
| Arrow keys | D-pad | Navigate |
| Z | A | Confirm / Select |
| X | B | Back / Cancel |
| C | X | Action 1 |
| V | Y | Action 2 |
| A / S | L1 / R1 | Tab left / right |
| Q / W | L2 / R2 | Page up / down |
| Return | Start | Menu |
| Space | Select | Toggle |
| Esc | — | Quit |

## Cross-Compilation (aarch64)

Build for ARM64 Linux handhelds with [cross-rs](https://github.com/cross-rs/cross):

```bash
cross build --release --target aarch64-unknown-linux-gnu
```

## Lua App SDK

Apps are "cartridges" — small Lua programs with a simple lifecycle:

```lua
function on_init(ctx)
    -- Load data, set up state
end

function on_input(event)
    -- Handle button presses
end

function on_update(dt)
    -- Update state each frame (dt = seconds since last frame)
end

function on_render(screen)
    -- Draw everything
    screen:clear()
    screen:draw_text("Hello", 20, 20, { bold = true, size = 24 })
end
```

### Available APIs

| Module | Description |
|--------|-------------|
| `screen` | 640x480 drawing surface — text, shapes, colors |
| `theme` | Consistent dark theme with accent colors |
| `storage` | Scoped key-value persistence per app |
| `http` | HTTP requests for fetching data |
| `json` | JSON encode/decode |
| `ssh` | SSH tunnel management |

## Bundled Apps

| App | Category | Description |
|-----|----------|-------------|
| Calculator | Tools | Calculator with expression history |
| Hacker News | News | Browse top stories, comments, and articles |
| Pomodoro | Productivity | Focus timer with work/break cycles and stats |
| Stock Market | Finance | Track stocks, crypto, indices with live charts |
| System Monitor | Tools | Real-time CPU, memory, disk, and network monitoring |
| Weather | Tools | Current conditions and 5-day forecast |

## Community Cartridges

| App | Category | Description |
|-----|----------|-------------|
| [VibeBoy](https://github.com/Strizzo/vibeboy-cartridge) | Tools | Remote control for VibeBoy daemon, manage tmux sessions and Claude Code |
| [World Pulse](https://github.com/Strizzo/worldpulse-cartridge) | Tools | Real-time global earthquake monitor and intelligence dashboard |

## Creating Cartridges

A cartridge is a directory with a `cartridge.json` manifest and a Lua entry point:

```
my-cartridge/
  cartridge.json
  main.lua
  icon.png          # optional, 64x64
```

The manifest describes your app:

```json
{
    "id": "dev.cartridge.my-app",
    "name": "My App",
    "version": "0.1.0",
    "author": "Your Name",
    "description": "What it does",
    "category": "tools",
    "tags": ["example"],
    "permissions": ["network", "storage"],
    "entry": "main.lua"
}
```

Available permissions: `network`, `storage`, `audio`, `system`, `input`, `ssh`.

### Lua API reference

Full API docs live in **[`docs/lua-api.md`](docs/lua-api.md)** — drawing
primitives, lifecycle callbacks, input events, networking, storage,
audio, system info, on-screen keyboard, and SSH tunneling.

For a runnable starter, see **`lua_cartridges/hello_world/`** — a 90-line
example covering input, drawing, and audio.

### Testing locally

```bash
cargo run -- run --path /path/to/my-cartridge
```

### Performance & visual regression tools

CartridgeOS ships a headless test harness for iterating without a device
(see [`docs/testing.md`](docs/testing.md)):

```bash
cargo run --bin perf-bench --release    # frame-time stats with thresholds
cargo run --bin snapshot                # PNG captures of UI screens
cargo test --test snapshot_test         # visual regression test
CARTRIDGE_FPS=1 ./dev-run.sh            # on-screen FPS overlay
```

### Publishing to the registry

The app registry is the `registry.json` file in this repo. To list your cartridge:

1. Host your cartridge in a public GitHub repo
2. Add a release workflow that creates a `.zip` artifact (see any existing cartridge repo for reference)
3. Open a PR adding your app entry to `registry.json`

## Project Structure

```
Cartridge/
  Cargo.toml                  # Workspace root
  Cross.toml                  # cross-rs config
  src/main.rs                 # Binary entry point
  crates/
    cartridge-core/           # Core types and abstractions
    cartridge-runner/         # SDL2 game loop and rendering
    cartridge-lua/            # Lua scripting engine
    cartridge-net/            # HTTP and networking
    cartridge-launcher/       # Launcher UI
    cartridge-boot/           # Device bootstrap
  assets/fonts/               # Bundled fonts (Cascadia Mono)
  cartridges/                 # Bundled Lua apps
  install.sh                  # Build script
  install_to_device.sh        # Cross-compile and deploy to device
```

## Device Deployment

Cross-compile for aarch64, then copy the binary, `assets/`, and `cartridges/` to the device. See `install_to_device.sh` for automated deployment.

## License

See [LICENSE](LICENSE) for the project's license terms.
