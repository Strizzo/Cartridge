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
  App adoption, blocking network calls, and device frame budgets still require
  inspection and verification; existence of an API is not evidence of adoption.
- Root-system verification uses a known-good image on the backup SSD. No changes
  in this branch have been installed on the physical card.

## Remaining acceptance gates

- [ ] Run the new primary session on the handheld, verify normal and recovery
  startup, ES handoff and return, shutdown/reboot, controls/audio and logs.
- [ ] Add a direct game library in Cartridge; reuse installed emulator launch
  configuration, ROM locations, per-game settings and saves. Validate at least
  representative RetroArch and standalone emulator paths and return to Cartridge.
- [ ] Apply the design language consistently to apps, including data/loading/error
  states, and review actual device-resolution captures rather than mockups only.
- [ ] Complete app performance work, remove blocking I/O from interaction paths,
  measure actual device latency/frame times and verify efficient idle behavior.
- [ ] Keep simulator checks reproducible (offline/low battery/control scenarios,
  app interactions and launch/return), with a small virtual device fixture.
- [ ] Establish a working ARM/Linux compatibility VM or equivalent runtime and
  document its limits; no claim of RK3326 GPU, thermal or battery emulation.
- [ ] Validate wireless deploy/restart/log/screenshot workflow with the device.
- [ ] Document app-development budgets and platform APIs using measured hardware
  behavior and a practical simulator-first release workflow.

Hardware/VM gates stay open until observed. A passing desktop smoke test cannot
close them. All games, saves, old-card captures and recovery backups must remain
preserved. EmulationStation remains an accessible fallback.
