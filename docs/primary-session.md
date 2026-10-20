# Cartridge as the primary launcher

The primary session starts Cartridge directly inside the **existing working
EmulationStation service**, under its existing `ark` user. The original service,
its enable link, bootloader, kernel, DTB, recovery drop-ins, ROMs and saves stay
intact. This replaces the older setup that disabled/masked ES services and
installed a competing boot selector.

After deploying a tested build, run **Options → Tools → Setup Cartridge Boot**
once, or over SSH:

```sh
sudo python3 /roms/Cartridge/setup-primary.py enable --cartridge-dir /roms/Cartridge
```

Use `/roms2/Cartridge` if that is the active ROM partition. Setup checks the
stock service, fallback script, binary loader and fonts. It installs the session
supervisor on the Linux root filesystem and adds only
`emulationstation.service.d/99-cartridge-primary.conf`. It does not stop the
current session or reboot. The next normal boot starts Cartridge.

The runtime acknowledges startup **after presenting its first frame**. Missing
files, failure before the first frame, a 20-second startup timeout or a later
crash hand control to the stock ES script. Crashes/timeouts also latch fallback
for future boots, avoiding a repeating broken startup. Run Setup again after
fixing the installation to clear that latch. An intentional ES selection exits
with status 20; a power action exits with status 30 so the supervisor does not
start ES during shutdown. SDL/audio resources close before the handoff.

The launcher loads its bundled registry immediately; network availability no
longer blocks the first screen. Refresh the store explicitly for registry updates.

The Games screen is directly available with L2; see [game library](game-library.md)
for shared emulator/save paths and launch/return validation.

## Recovery and undo

- Cartridge's Select menu can switch to EmulationStation.
- A file named `boot-emulationstation` in the Cartridge directory forces stock
  startup. It can be created from a Mac without opening the Linux partition.
- **Tools → Undo Cartridge Boot** removes only the managed override. It does not
  remove Cartridge or any games, and takes effect at the next boot.
- Session output and the last transition are in
  `/home/ark/.cartridges/session/{session.log,last-session.json,fallback.json}`.
  The previous log is retained when startup rotates a log exceeding 1 MiB.
- A legacy `cartridge-boot.service` must not still be enabled. Setup refuses that
  conflicting layout instead of guessing which services to disable.

The SD installer now copies the application payload only. It does not modify
ext4 through `debugfs` or replace BOOT artwork. Primary startup is configured
from the running device using ordinary filesystem operations.

## Test without touching a card

```sh
python3 -m unittest discover -s tests -p 'test_*.py'
./sim.sh check
./sim.sh session --hidden --software -- --check
./sim.sh session                    # interactive native launcher + supervisor
```

The tests cover first-frame success, ES handoff, crash latching, startup timeout,
missing installation, recovery flag, shutdown signals, power requests, state
storage failure, reversible installation and preservation of stock unit/recovery
configuration and game saves. The session simulator runs the actual Rust UI and
actual Python supervisor with an ES stand-in; it does not boot a Linux kernel.

No automatic setup has been applied to the working replacement card yet. The
physical boot, GPU/display handoff, game launching and return must be validated
before this becomes the default on the device.
