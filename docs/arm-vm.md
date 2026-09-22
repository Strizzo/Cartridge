# ARM Linux compatibility VM

The Mac native simulator is the interactive UI workbench. The separate ARM Linux
VM executes CI-built ARM binaries and exercises Linux systemd setup/fallback/undo.
Both work without inserting an SD card.

## Run

Requires Apple Silicon, macOS 13.5+, Lima 2.x, Python 3, curl and (for `--run`) the
GitHub CLI authenticated for this repository. Lima was already installed on the
development Mac. The VM uses [Apple Virtualization through Lima](https://lima-vm.io/docs/config/vmtype/vz/),
2 vCPUs, 1 GiB RAM and an 8 GiB sparse disk. No host directories, raw disks,
recovery images or real games are mounted in it. Containers are disabled.

```sh
./sim/vm.sh start
./sim/vm.sh check --run 35783335255     # verified ARM CI artifact
# Or: ./sim/vm.sh check /path/to/extracted/device-bundle
./sim/vm.sh stop
```

For future runs, use the successful Build workflow ID for the commit under test.
The device bundle must include `Cartridge/dev/sim-check`. The check downloads
only that selected artifact, copies a small fixture into the VM and saves the
results back under `.sim/vm/results/`. Binary SHA-256, CI revision, OS/kernel,
linked libraries, systemd transition reports and 720×720 screenshots are retained.
No source compilation or mounted host workspace is needed inside the guest.

The first boot downloads a checksum-pinned 217 MiB Ubuntu 24.04 ARM minimal
image and installs SDL runtime libraries. `curl` uses the Mac resolver because
Lima's Go resolver timed out on this network. Apple NAT is configured because
the initial usernet interface could accept SSH but could not reach package
servers. Host DNS settings are unchanged. Stop the VM after testing to release
its 1 GiB RAM. Its disk lives in `~/.lima/cartridge-arm64`; images/artifacts live
in `.sim/vm`. Together these occupied about 1.6 GiB after initial verification.

## What passes

Verified on 2026-09-22 using the successful CI ARM artifact from commit
`12e8697` (including the game-library implementation from `1e0d11e`):

- ARM ELF loading and runtime library resolution.
- 27 Python tests for library import, quoting, simulator isolation, launch
  failure, supervisor cleanup, recovery and reversible setup.
- The actual ARM renderer/input code: home/settings/store, system/game browsing,
  simulated game handoff and restoration of a non-first selection, Lua Todo,
  first-frame readiness and ES request.
- Real Linux systemd service installation, rendered first-frame/ES handoff,
  forced startup failure, fallback latch and undo. The stock ES service and ES
  executable are explicit test stand-ins; no emulator or ES build is included.

The guest verifier requires `/etc/cartridge-compat-vm`, created only by VM
provisioning. Do not run it on the handheld: it creates a disposable fake stock
service and `ark` user for integration checks. The fake service is disabled after
the check, and the Cartridge override is removed.

## What it cannot establish

The guest uses a generic Linux 6.8 kernel, Ubuntu 24.04 libraries and the Mac's
ARM CPU through virtualization. The handheld has a different kernel/library
stack, Rockchip display/Mali GPU, audio hardware, controls, clocks and battery.
Rendering here uses SDL's software/dummy driver. VM frame times do not predict
RK3326 frame times. Successful dynamic linking here does not prove compatibility
with every installed handheld library version.

Game fixtures are non-playable and launch is simulated. Actual RetroArch and
standalone gameplay, existing saves, GPU/audio ownership, battery and thermal
behavior remain device tests. Use the [wireless workflow](wireless-deploy.md)
for those final checks; keep the working ES fallback until they pass.
