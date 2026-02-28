# Cartridge

An open app ecosystem for Linux handheld devices. Build, share, and run apps on your R36S, Anbernic, PowKiddy, and other handhelds.

## What is Cartridge?

Cheap Linux handhelds are open computers with WiFi, screens, d-pads, and buttons — but the entire ecosystem treats them as single-purpose retro gaming devices. Cartridge changes that.

Cartridge is a Python SDK, an app store, and a launcher that turns any Linux handheld into a general-purpose pocket device. Apps are "cartridges" — small, focused programs built with a simple framework and distributed through a curated registry.

## Bundled Apps

| App | Category | Description |
|-----|----------|-------------|
| Hacker News | News | Browse top stories, comments, and articles |
| Stock Market | Finance | Track stocks, crypto, indices with live charts |
| Weather | Tools | Current conditions and 5-day forecast |
| Calculator | Tools | Calculator with expression history |
| Pomodoro | Productivity | Focus timer with work/break cycles and stats |
| System Monitor | Tools | Real-time CPU, memory, disk, and network monitoring |

## Quick Start (Desktop Development)

Cartridge apps run in a desktop simulation window for development. You need Python 3.11+.

### macOS

```bash
brew install python@3.12
git clone https://github.com/Strizzo/Cartridge.git
cd Cartridge
pip3 install -e ".[stock]"
cartridge run --path cartridges/hacker_news
```

### Linux (Debian/Ubuntu)

```bash
sudo apt install python3 python3-pip python3-venv
git clone https://github.com/Strizzo/Cartridge.git
cd Cartridge
pip3 install -e ".[stock]"
cartridge run --path cartridges/hacker_news
```

### Linux (Fedora)

```bash
sudo dnf install python3 python3-pip
git clone https://github.com/Strizzo/Cartridge.git
cd Cartridge
pip3 install -e ".[stock]"
cartridge run --path cartridges/hacker_news
```

### Linux (Arch)

```bash
sudo pacman -S python python-pip
git clone https://github.com/Strizzo/Cartridge.git
cd Cartridge
pip3 install -e ".[stock]"
cartridge run --path cartridges/hacker_news
```

### Windows

1. Install Python 3.12+ from [python.org](https://www.python.org/downloads/)
2. Open a terminal:

```powershell
git clone https://github.com/Strizzo/Cartridge.git
cd Cartridge
pip install -e ".[stock]"
cartridge run --path cartridges/hacker_news
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

## Install on Device

### Supported Devices

Cartridge runs on Linux handhelds with WiFi and a supported custom firmware:

| Device | SoC | Firmware |
|--------|-----|----------|
| R36S / R36S Plus | RK3326 / A133P | ArkOS, ROCKNIX, Knulli |
| Anbernic RG351P/M/V/MP | RK3326 | ArkOS, ROCKNIX, Knulli |
| Anbernic RG353P/M/V/VS | RK3566 | ArkOS, ROCKNIX, Knulli |
| Anbernic RG503 | RK3566 | ArkOS, ROCKNIX |
| PowKiddy RGB30 | RK3566 | ArkOS, ROCKNIX, Knulli |
| PowKiddy X55 | RK3566 | ArkOS, ROCKNIX |

Any device running a Debian-based Linux handheld CFW should work.

### Method 1: USB Install (Recommended)

No SSH, no terminal, no Python required on your computer — just a USB cable.

1. Download the installer for your computer from the [latest release](https://github.com/Strizzo/Cartridge/releases/latest):
   - **macOS**: `Cartridge-Installer-macOS`
   - **Linux**: `Cartridge-Installer-Linux`
   - **Windows**: `Cartridge-Installer-Windows.exe`
2. Connect your device to your computer via USB (enable USB Mass Storage in your device's settings if needed)
3. Run the installer — it detects your device and copies everything automatically
4. Disconnect the USB cable
5. On your device, connect to WiFi (needed to install Python dependencies)
6. In EmulationStation, go to **Tools** and run **Install Cartridge**
7. Wait for setup to complete (~2-5 minutes)
8. Restart EmulationStation — **Cartridge** will appear in the main carousel alongside your game systems

### Method 2: Network Install (via SSH)

If you have SSH access and prefer a one-liner:

```bash
curl -sSL https://raw.githubusercontent.com/Strizzo/Cartridge/main/install.sh | bash
```

This clones the repo, installs dependencies, and sets up the launcher automatically.

### Method 3: Manual Setup

```bash
# On the device via SSH
sudo apt update
sudo apt install -y python3 python3-pip python3-venv python3-dev libsdl2-dev libsdl2-image-dev libsdl2-mixer-dev libsdl2-ttf-dev

# Create install directory
sudo mkdir -p /opt/cartridge
sudo chown $(whoami) /opt/cartridge

# Clone and install
cd /opt/cartridge
git clone https://github.com/Strizzo/Cartridge.git .
python3 -m venv venv
source venv/bin/activate
pip install -e ".[stock]"

# Launch
cartridge home cartridges/cartridge_client --fullscreen
```

### Installing Additional Apps

From any terminal on the device:

```bash
source /opt/cartridge/venv/bin/activate
cartridge install https://github.com/user/their-cartridge-app
```

Or use the on-device store to browse and install from the registry.

## Developing a Cartridge

### Create a New App

```bash
cartridge new "My App"
cd my_app
cartridge run
```

This creates a project with:

```
my_app/
  cartridge.toml      # App manifest
  src/
    main.py           # Entry point (your CartridgeApp subclass)
    screens/
      __init__.py
  assets/
```

### App Lifecycle

```python
from cartridge_sdk import CartridgeApp, AppContext, Screen, InputEvent

class MyApp(CartridgeApp):

    async def on_init(self, ctx: AppContext) -> None:
        await super().on_init(ctx)
        # Load data, set up screens

    def on_input(self, event: InputEvent) -> None:
        # Handle button presses

    async def on_update(self, dt: float) -> None:
        # Update state each frame (dt = seconds since last frame)

    def on_render(self, screen: Screen) -> None:
        # Draw everything
        screen.clear()
        screen.draw_text("Hello", 20, 20, bold=True, font_size=24)
```

### Available SDK Features

- **UI Widgets**: ListView, DetailView, TabBar, Table, StatusBar, LoadingIndicator, Toast, SparkLine, LineChart
- **Networking**: Async HTTP client with disk + memory caching
- **Storage**: Scoped key-value persistence per app
- **Input**: D-pad, face buttons (A/B/X/Y), shoulders (L1/R1/L2/R2), with key repeat
- **Screen**: 640x480 drawing surface with text, shapes, cards, pills, progress bars, gradients
- **Theming**: Consistent dark theme with accent colors

See [design_catridge.md](design_catridge.md) for the full architecture and API reference.

## Project Structure

```
Cartridge/
  src/cartridge_sdk/          # The SDK framework
    app.py                    # CartridgeApp base class, AppContext
    runner.py                 # Game loop (pygame + asyncio)
    screen.py                 # Drawing surface
    input.py                  # Button enum, InputEvent, InputManager
    net.py                    # Async HTTP client with caching
    storage.py                # Scoped app storage
    manifest.py               # cartridge.toml parser
    theme.py                  # Color/style constants
    management.py             # Install/remove/launch management
    cli/main.py               # CLI entry point
    ui/                       # Widget library
    reader/                   # Article extraction
  cartridges/                 # Bundled apps
    hacker_news/
    stock_market/
    weather/
    calculator/
    pomodoro/
    system_monitor/
    cartridge_client/         # The on-device store and launcher
  registry.json               # App catalog
  install.sh                  # Device install script
  pyproject.toml              # Python package config
  design_catridge.md          # Full design document
```

## CLI Reference

| Command | Description |
|---------|-------------|
| `cartridge run --path <dir>` | Run a cartridge in desktop simulation mode |
| `cartridge run --resolution 800x600` | Run with custom resolution |
| `cartridge run --fullscreen` | Run in fullscreen mode |
| `cartridge new <name>` | Scaffold a new cartridge project |
| `cartridge home <client-path>` | Run the launcher loop (client + app switching) |
| `cartridge install <github-url>` | Install a cartridge from a GitHub repo |
| `cartridge install <url> --branch dev` | Install from a specific branch |

## License

MIT
