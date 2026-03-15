# CartridgeOS

An open app platform for Linux handhelds (R36S Plus, Anbernic, PowKiddy). Turn a $40 retro gaming device into a general-purpose pocket computer with WiFi, apps, and a d-pad.

Built in Rust with SDL2. Apps are scripted in Lua.

## Install on R36S Plus

No build tools needed. Works from Windows, macOS, or Linux.

1. Download `cartridge-r36s-plus.zip` from the [latest release](https://github.com/Strizzo/Cartridge/releases/latest)
2. Turn off the device and remove the SD card
3. Insert SD card into your computer, open it, find the `roms/` folder
4. Extract the zip into `roms/` so you get `roms/Cartridge/` and new files in `roms/tools/`
5. Eject, put the SD card back, boot the device
6. In EmulationStation, go to **Tools > Cartridge** to launch

To make Cartridge the default at boot, run **Tools > Setup Cartridge Boot** from EmulationStation. You'll get a boot selector to choose between Cartridge and EmulationStation on every startup.

See [INSTALL.md](INSTALL.md) for troubleshooting and build-from-source instructions.

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

Available permissions: `network`, `storage`, `ssh`.

### Testing locally

```bash
cargo run -- run --path /path/to/my-cartridge
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

MIT
