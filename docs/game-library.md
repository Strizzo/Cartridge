# Games in Cartridge

Press **L2 → Games** from Home, choose a system, and press A to play. B returns,
L1/R1 page through long lists, and Y refreshes the library. On the Mac, Q is L2
and Z is A. After the emulator exits, Cartridge restores the selected system
and game. Launch failures return to that screen with an error.

The library reads the installed EmulationStation configuration and gamelists.
It leaves ROMs, saves, BIOS files, emulator configuration and gamelists in place.
Metadata supplies titles, favorites, hidden entries, player counts and cover art.
Only configured `/roms` and `/roms2` systems are indexed; the Options/tools
category is excluded. Symlink directories are not traversed.

## Launch compatibility

`deploy/game-library.py` uses the stock system/emulator command template and
resolves emulator, core and governor from per-game metadata, saved system
settings and configured defaults. Selection follows the installed ArkOS
[FCAMOD launcher](https://github.com/christianhaitian/EmulationStation-fcamod/blob/master/es-app/src/FileData.cpp)
and [system defaults](https://github.com/christianhaitian/EmulationStation-fcamod/blob/master/es-app/src/SystemData.h).
ROM filenames are quoted as literal shell arguments. Stock `perfmax`/`perfnorm`
commands remain in the template; Cartridge does not choose higher clocks.

The Rust UI reads library data through a background worker. It loads one game
system at a time and retains only the current cover texture. The SDL window,
input devices and audio close before the emulator starts. The installed stock
service's `ark` user and HOME retain the same emulator settings/save locations.
Use a single running frontend: launching a competing copy alongside ES is not
a substitute for primary-session validation.

This is command/configuration compatibility, not every ES feature. Collections,
editing favorites/metadata, scraper, video previews, play-count updates and ES
launch hooks are not implemented. Existing favorites are displayed. Malformed
configuration or unknown command placeholders produce an error. Covers outside
the configured system directory are omitted.

Diagnostics are in `~/.cartridges/games/launch.log` and `last-launch.json`.
The previous log is retained when startup rotates a file exceeding 1 MiB.

## Test without a card

`./sim.sh` creates five original, non-playable game fixtures and cover images in
`.sim/home/device/`. A simulated launch writes a report and returns immediately;
it never runs an installed emulator or opens a real ROM. `./sim.sh check` captures
system/game screens, selects a non-first game, runs that simulated handoff, then
checks that returning to the library restores exactly the same selection.

For a read-only catalog inspection, override `CARTRIDGE_ES_SYSTEMS`,
`CARTRIDGE_ES_SETTINGS`, `CARTRIDGE_ES_HOME` and `CARTRIDGE_ROMS`. Setting
`CARTRIDGE_ES_SYSTEMS` disables generated fixtures. `CARTRIDGE_SIM=1` still
prevents emulator execution. Do not use a recovery image as a writable VM disk.

## Evidence and remaining device gate

On 2026-09-22 the actual recovered config and SSD game backup resolved 122 game
systems, 29 populated. PlayStation returned 55 games with `retroarch32` /
`pcsx_rearmed_rumble`; PSP returned 36 with `standalone`; both respected the
saved `powersave` governor. This was a read-only import/command-plan check.

Native simulator launch/return, offline Linux-style SDL dummy rendering and
Python command/quoting/failure tests pass. Physical RetroArch and standalone
launch, display/audio/input ownership, save loading and return remain unverified.
Test those on the working replacement card over SSH before making the new
primary session the default. Keep EmulationStation available throughout.
