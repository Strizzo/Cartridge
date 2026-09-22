# Desktop Simulator

`sim.sh` runs the real launcher and real cartridges on your desktop, with the
device's screen geometry, a simulated battery/WiFi/hostname, and a sandboxed
home directory — so you can build the whole UI without touching the handheld.

```bash
./sim.sh                                  # launcher
./sim.sh app lua_cartridges/hacker_news   # one cartridge, hot reload on
./sim.sh boot                             # the 5 s boot selector
./sim.sh demo                             # drawing-primitives demo
```

## Flags

| Flag | What it does |
| --- | --- |
| `--scale N` | Window is 720·N points. Default 1. |
| `--true-size` | Scales the window so it is 71.8 mm wide on your main display — the physical size of the device panel. Use this before deciding a font is big enough. |
| `--fullscreen` | Desktop fullscreen, 720×720 letterboxed. |
| `--release` | Release build. Closer to device behaviour, still not device numbers. |
| `--profile <json>` | Simulated device profile. Default `sim/profiles/r36s-plus.json`. |
| `--battery N` | Override battery percent (e.g. `--battery 8` to check the low-battery header). |
| `--wifi off\|<ssid>` | Override WiFi state. |
| `--home <dir>` | `CARTRIDGE_HOME`. Default `.sim/home`, which is gitignored. |
| `--no-fps` | Turn off the FPS overlay (on by default). |
| `--software` | Software renderer. Use it when you want exact F12 screenshots. |
| `--hidden` | Hidden window, for headless smoke tests. |

Everything after `--` is passed through to the binary.

## Controls

```
arrows = D-pad   Z = A   X = B   C = X   V = Y
A = L1   S = R1   Q = L2   W = R2
Enter = Start    Space = Select    Esc = quit
F12 = screenshot -> screenshots/
```

The same line is printed to stderr on startup when `CARTRIDGE_SIM=1`. A USB
gamepad works too; `assets/gamecontrollerdb.txt` is loaded when present, so you
can drop the SDL community database there for an unusual pad.

## The device profile

`sim/profiles/r36s-plus.json` is the single source of truth for everything the
desktop can't measure: hostname, battery, WiFi (including the list the WiFi
screen scans), RAM and disk totals, baseline CPU load, brightness, volume.
Before this existed, the macOS code paths each invented their own values — a
hard-coded 72 % battery, your Mac's hostname, and two different fake SSIDs that
disagreed with each other.

`sim/profiles/low-battery-offline.json` is a second profile for testing the
unhappy path. Brightness and volume changes are kept in memory for the life of
the process, so the Settings sliders actually move.

## Environment variables

`sim.sh` sets these for you; they also work standalone.

| Variable | Effect |
| --- | --- |
| `CARTRIDGE_SIM=1` | Simulator mode: prints the keyboard cheat sheet. |
| `CARTRIDGE_SIM_PROFILE` | Path to the device profile JSON. |
| `CARTRIDGE_SIM_BATTERY`, `CARTRIDGE_SIM_WIFI`, `CARTRIDGE_SIM_HOSTNAME` | Quick overrides that beat the profile. |
| `CARTRIDGE_HOME` | Replaces `$HOME` for `~/.cartridges/…` lookups — settings, installed apps, caches. |
| `CARTRIDGE_ASSETS` | Assets directory (fonts, overlays, controller DB). |
| `CARTRIDGE_SCALE`, `CARTRIDGE_FULLSCREEN` | Window scaling. Drawing code always works in 720×720 logical pixels. |
| `CARTRIDGE_SOFTWARE`, `CARTRIDGE_HIDDEN` | Renderer and window visibility. |
| `CARTRIDGE_FPS=1` | On-screen FPS / frame-time / text-cache overlay. Works in the launcher **and** in cartridges. |
| `CARTRIDGE_HOT_RELOAD=1` | Re-run a cartridge when any of its `.lua` files changes. |

## Screenshots

F12 (or `kill -USR1 <pid>`) writes `screenshots/<timestamp>.png`. Frame
read-back is exact on the software renderer; on the accelerated renderer it is
usually right but can come back empty depending on the driver, so pass
`--software` when a screenshot has to be pixel-accurate.

## Hot reload

`./sim.sh app <dir>` sets `CARTRIDGE_HOT_RELOAD=1`. Edits to any `.lua` file in
the cartridge directory rebuild the Lua VM and call `on_init()` again within a
second. Lua state is discarded; edits to `cartridge.json` or to assets are not
watched, so restart for those.

## Limits — read this before trusting a number

- **Desktop frame times are not device frame times.** A MacBook is roughly an
  order of magnitude faster than the RK3326. Use the simulator to catch
  *regressions* (`cargo run --bin perf-bench --release -- app lua_cartridges/bench`),
  and the real device to judge whether something is fast enough.
- Text rendering goes through the same SDL2_ttf path, but the device
  rasterises at a different subpixel offset, so snapshots differ slightly.
- Audio, brightness and volume are simulated. Power actions (reboot, shutdown,
  switch to EmulationStation) are logged and ignored off-Linux.
- `snapshot` and `perf-bench` still expect to run from the repo root.

## Repeatable checks and boot-session rehearsal

`./sim.sh check` runs the real launcher through home, settings, store, the game
library with simulated launch/selection restoration, and the ES handoff, renders the Todo cartridge, exercises simulated brightness/volume/Wi-Fi,
and checks first-frame readiness and 720×720 PNG dimensions. Captures are saved
under `.sim/home/checks/`. This command uses a hidden software-rendered window;
on macOS it still needs access to the WindowServer. On Linux CI it can run with
`SDL_VIDEODRIVER=dummy`. Simulator mode bypasses real hardware/network-management
controls on Linux as well as macOS.

`./sim.sh session` wraps the real native UI in the same supervisor used at device
startup. Selecting ES prints a simulated handoff instead of launching a foreign
binary. `./sim.sh session --hidden --software -- --check` drives that handoff
automatically. No host services are modified. The old `boot` subcommand previews
the optional graphical selector; it is not the new primary-session startup path.

## Game-library fixture

The default simulator generates a small original library and cover art in its
isolated home. Press Q (L2) for Games, then Z (A) to open a system or simulate a
launch. Game fixtures are non-playable; simulator mode never executes device
emulator commands. See [game library](game-library.md) for configuration import,
launch reports, automated return-to-selection checks and device validation limits.

## ARM virtual machine boundary

The native simulator is the fastest edit/run/screenshot loop. It uses the same
Rust/Lua drawing and input code but does not emulate RK3326 hardware or predict
its frame times. The next compatibility layer is an ARM Linux VM for the device
binaries, dynamic libraries and systemd startup integration, with a disposable
root and a small ROM fixture rather than the 128 GB recovery image.

QEMU's [`virt` platform](https://www.qemu.org/docs/master/system/arm/virt.html)
supports Cortex-A35 emulation but is explicitly a generic virtual board. Its
virtual GPU is not the handheld's Mali/display stack. The stock Rockchip kernel
and DTB cannot simply be assumed to boot that board. Such a VM needs a compatible
generic kernel; it is an ABI/service test, not a hardware-performance oracle.
An ARM Linux compatibility VM is now implemented and verified using Lima/Apple
Virtualization. It runs the CI ARM binaries, captures the actual UI and tests
setup/fallback/undo through Linux systemd. Use `./sim/vm.sh start`, then
`./sim/vm.sh check --run <successful-build-id>`. See [ARM VM](arm-vm.md) for
requirements, exact checks, saved evidence and the hardware limits. QEMU board
emulation is not installed or claimed.

Use wireless deployment for final real-device checks. A single enabled SSH
connection lets the Mac push builds, restart the primary session, collect logs
and capture frames without repeated SD-card swaps. See
[wireless deployment](wireless-deploy.md) and [primary startup](primary-session.md).
