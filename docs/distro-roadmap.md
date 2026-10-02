# Cartridge device distribution

Cartridge is intended to be the R36S Plus's primary operating environment: the
first interactive screen after power-on, with applications, games, device settings
and power controls. A reproducible bootable image and a desktop SD installer are the distribution
milestone. The current branch supplies a primary session and an initial native macOS
installer for verified local images; a reproducible public system image remains open. The intended user flow is described in
[one-step SD installation](offline-installer.md).

## Boot and hardware architecture

```text
Bootloader -> Linux kernel + board drivers -> systemd -> Cartridge session
                                                        |-- Cartridge home
                                                        |     |-- Lua apps
                                                        |     `-- native apps / emulators
                                                        `-- EmulationStation recovery
```

On the device, the session selects SDL's KMSDRM display backend and ALSA audio
backend. Cartridge is designed to own the display without a desktop environment.
The launcher releases its SDL resources before starting an emulator and resumes
when it exits. The physical display/audio transition remains a device gate.

The current migration reuses the verified stock `emulationstation.service`, with
one override replacing its executable with the Cartridge session supervisor. Its
name is a compatibility detail: normal startup runs Cartridge first, without
starting the ES interface. The original service supplies the working account,
permissions, ordering and recovery configuration. A future image can provide a
named `cartridge-session.service` after those dependencies are captured explicitly.
[Primary-session setup and undo](primary-session.md) describe the implemented path.

A distribution can use the existing Linux kernel and still own the entire user
experience, default services, update policy and application platform. Kernel and
bootloader work should address a demonstrated driver, startup or power-management
need. Replacing them alone does not remove blocking application work or expensive
rendering. The working board-specific kernel, DTB and firmware are the hardware
baseline; preserve their verified versions while measuring improvements.

## Delivery stages

| Stage | Deliverable | Current state |
| --- | --- | --- |
| 1. Primary session | Direct boot into Cartridge; apps and games launch from home; explicit ES recovery and undo. | Implemented and tested in the native simulator and ARM VM. Owner confirmed direct boot on the spare card; complete device lifecycle validation remains open. |
| 2. Device platform | Measured startup, display/input/audio lifecycle, brightness, volume, WiFi, power actions, bounded app work and wireless updates. | APIs and tooling exist; remaining blocking paths and physical validation are open. |
| 3. Cartridge system image and installer | Versioned image recipe with pinned base, board files, packages, kernel/DTB checksums, emulator compatibility, recovery and a desktop writer that verifies the card. | Read-only card inventory and clone, Mac-to-VM root preparation, guarded s2 writeback, ROMS rollback and two-partition coordination pass disposable image checks. Spare-card direct boot confirmed. Native macOS image-writer UI exists; reproducible public fresh image and full preserve-card UI remain open. |
| 4. Hardware specialization | Measured service reduction, clock/power policy and justified driver/kernel changes for the exact board/panel. | Requires a recorded device baseline and benchmarks first. |

The owner confirmed direct boot into Cartridge on 2026-09-24, and later confirmed
that the verified WPA package repair restored Wi-Fi and connected apps. Preserve
that working boot and network baseline during subsequent app-bundle updates. Routine UI/app
iterations stay in the simulator, followed by wireless deployment for device-only
checks. Keep ROMs, saves and emulator configuration at their existing paths.

## Platform baseline and performance work

Before choosing a distribution base or tuning boot, record the actual board and
panel identity, kernel/DTB/firmware, available graphics drivers, SDL renderer,
shared-library versions, input mappings, audio device, RAM and storage. A model
name or an ARM CPU architecture alone is insufficient to validate those pieces.
Capture the enabled services and boot timing to understand which startup work is
necessary; preserve power-button, battery, filesystem and hardware services.

Measure power-on to first usable home, app/game start and return, input-to-display
latency, rendered-frame p95/max, process memory across repeated launches, and idle
power/temperature under fixed brightness and WiFi conditions. Separate cold and
warm starts. The VM can test ordering and failures, but its timing is not a device
benchmark. Define device budgets from that baseline and compare the same build
and workload before changing governors or disabling services.

The runtime already skips clean frames, wakes idle waits on input, bounds timing
history and network queues, and keeps bundled app HTTP off the UI thread. Store
installation and catalogue refresh now run on workers with verified staged packages
(see [App Store](app-store.md)). Wi-Fi scan/status/connect/disconnect and Settings hardware controls now run on
workers, with cached UI values and bounded pending work. SSH setup and repeated
SDL/font startup remain candidates for measured work. [App development](app-development.md) covers the
API, original graphics, asynchronous workflows and provisional frame budgets.

## Reproducible image and updates

The initial image recipe should reproduce the verified userspace/board stack
with Cartridge selected by default. Record the base-image and toolchain digests,
lock dependencies, inventory packages and runtime libraries, and retain sources
and license notices. The current Docker build pins a distribution release but
still resolves package updates and a moving Rust toolchain; it is not a fully
reproducible image build.

Keep system installation separate from ROM/save migration. Both installer modes
must boot directly into Cartridge without an EmulationStation menu step. A blank
card receives a fresh image. A compatible populated card keeps its ROMS, saves
and partition map by default while Cartridge files and the Linux startup service
are prepared offline. A full reformat is available only for a blank card or an
explicit erase choice, with backup and verified restoration for populated cards.
The offline conversion, rollback and guarded Linux-partition writeback core is
implemented and tested on disposable image files; macOS has a read-only
whole-disk/layout inventory and a tested CLI handoff into a dedicated ARM
preparation VM. The native macOS writer supports verified local images on an explicitly selected
empty spare; the full preserve-card graphical flow remains open. An image builder must
produce an artifact file, with explicit target-media selection for any later
flashing step. Test installation on a disposable image or spare card before
considering the user's working card. Never distribute personal ROMs, saves, SSH
keys, WiFi credentials or recovery captures inside the base image.

Image releases need a tested restore path and interrupted-update behavior.
Application updates can progress first toward staging a complete version,
verifying its manifest and libraries, switching it into use only when ready, and
retaining the previous version. The current wireless deploy copies files and
restarts the session; it is a development tool, not yet a transactional OS updater.
A/B system partitions are an option to evaluate for fresh images, not a reason
to repartition the existing game card during development.

## Validation without repeated card removal

- **Mac simulator:** the actual Rust/Lua UI at 720x720, device-state fixtures,
  interactive hot reload, screenshots and deterministic app scenarios.
- **ARM Linux VM:** the CI-built ARM executables, shared-library loading, real
  systemd installation, first-frame readiness, ES handoff, crash recovery and undo.
- **Handheld over WiFi:** actual panel/GPU, controls/audio, emulators/saves,
  suspend/power behavior, frame times, thermals and battery. This closes release
  gates that the generic ARM VM cannot establish.

The VM currently uses a generic Ubuntu kernel/userspace and software rendering.
It does not boot the handheld's board image or emulate its Rockchip/Mali devices.
A device-matched userspace compatibility test could improve ABI coverage later;
it would still not establish physical GPU, power or timing behavior.

The living acceptance checklist is [device experience](cartridge-device-goal.md).
The goal remains open until the primary session and app/game lifecycle work on
the replacement card and can be iterated wirelessly with a verified recovery path.
