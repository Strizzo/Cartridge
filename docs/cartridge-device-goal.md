# Device experience implementation and validation

Objective: Cartridge becomes the R36S Plus primary launcher and mini-computer
experience, with apps and the existing game library, consistent bold visual
styling, responsive input, efficient idle rendering, Mac development and a
verified/reversible device boot. Keep the original interface isolated in git.

The working replacement card now boots EmulationStation and runs games (confirmed
by its owner after all restored files, bootloader, root partition and filesystems
passed checks). This is the recovery baseline. Do not replace its kernel/DTB or
reformat it as part of launcher development.

## Current evidence

- `redesign/neo-tokyo` contains the UI/runtime work. Primary-session development is
  isolated on `codex/cartridge-primary`.
- The actual recovered ES service runs as `ark` and invokes
  `/usr/bin/emulationstation/emulationstation.sh`. Its recovery output override
  is `90-cartridge-recovery.conf`. The new later override preserves both files.
- Native 720×720 home/settings/store/Todo renders and the requested ES handoff
  pass in the simulator. Python startup failure/recovery/undo tests pass.
- Lua already has dirty rendering, idle rate controls and asynchronous HTTP APIs.
  All bundled network apps use async polling, including AI Papers and Network
  Tool. Request/completion queues and text responses are bounded; cancellation,
  retries and incremental updates have behavioral tests. Both timing histories
  are bounded during interactive use, and benchmarks separate rendered work from
  skipped idle iterations. Actual device budgets remain to measure.
- Shared app styling now supplies condensed display headers, flat cards and
  control hints. Twenty-two app scenarios render with synthetic delayed/offline
  responses and fail on Lua errors. Full app-content and physical readability
  review remains open. Weather now has custom condition icons and larger current/forecast
  readings; input-aware pacing wakes on SDL events while preserving low idle update
  rates. App scenarios use separate temporary storage. Development/porting guidance is in `app-development.md`.
- Root-system verification uses a known-good image on the backup SSD. No changes
  in this branch have been installed on the physical card.

## Remaining acceptance gates

- [ ] Run the new primary session on the handheld, verify normal and recovery
  startup, ES handoff and return, shutdown/reboot, controls/audio and logs.
- [x] Add a direct game library in Cartridge using installed emulator launch
  configuration, ROM locations and per-game settings. Native simulator launch
  and selection restoration pass; real recovered library import is read-only.
- [ ] Validate physical RetroArch and standalone emulator launch, saves and return
  to Cartridge. See `game-library.md` for the evidence and remaining limits.
- [ ] Apply the design language consistently to apps, including data/loading/error
  states, and review actual device-resolution captures rather than mockups only.
- [ ] Complete app performance work, remove blocking I/O from interaction paths,
  measure actual device latency/frame times and verify efficient idle behavior.
- [ ] Keep simulator checks reproducible (offline/low battery/control scenarios,
  app interactions and launch/return), with a small virtual device fixture.
- [x] Establish a working ARM/Linux compatibility VM: ARM binaries, real UI
  scenarios and systemd setup/fallback/undo pass in Lima/VZ. See `arm-vm.md`;
  no RK3326 GPU, thermal, battery or exact device-library emulation is claimed.
- [ ] Validate wireless deploy/restart/log/screenshot workflow with the device.
- [ ] Document app-development budgets and platform APIs using measured hardware
  behavior and a practical simulator-first release workflow.

Hardware/VM gates stay open until observed. A passing desktop smoke test cannot
close them. All games, saves, old-card captures and recovery backups must remain
preserved. EmulationStation remains an accessible fallback.
