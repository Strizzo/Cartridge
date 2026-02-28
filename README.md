# Cartridge — App SDK for Linux Handheld Devices

An open app ecosystem for devices like R36S Plus, Anbernic, and PowKiddy handhelds. Built in Rust with SDL2. Apps are scripted in Lua.

Cheap Linux handhelds are open computers with WiFi, screens, d-pads, and buttons — but the entire ecosystem treats them as single-purpose retro gaming devices. Cartridge changes that by turning any Linux handheld into a general-purpose pocket device.

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

## Bundled Apps

| App | Category | Description |
|-----|----------|-------------|
| Calculator | Tools | Calculator with expression history |
| Hacker News | News | Browse top stories, comments, and articles |
| Pomodoro | Productivity | Focus timer with work/break cycles and stats |
| Stock Market | Finance | Track stocks, crypto, indices with live charts |
| System Monitor | Tools | Real-time CPU, memory, disk, and network monitoring |
| Weather | Tools | Current conditions and 5-day forecast |

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
