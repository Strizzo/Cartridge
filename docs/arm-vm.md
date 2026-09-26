# ARM Linux compatibility VM

The Mac native simulator is the interactive UI workbench. The separate ARM Linux
VM executes CI-built ARM binaries and exercises Linux systemd setup/fallback/undo.
Both work without inserting an SD card.

## Run

Requires Apple Silicon, macOS 13.5+, Lima 2.x, Python 3, curl and (for `--run`) the
GitHub CLI authenticated for this repository. Lima was already installed on the
development Mac. The VM uses [Apple Virtualization through Lima](https://lima-vm.io/docs/config/vmtype/vz/),
2 vCPUs, 1 GiB RAM and an 8 GiB sparse disk. The normal check mounts no host
directories, raw disks, recovery images or real games. Containers are disabled.

```sh
./sim/vm.sh start
./sim/vm.sh check --run 35789361899     # verified ARM CI artifact
# Or: ./sim/vm.sh check /path/to/extracted/device-bundle
./sim/vm.sh stop
```

For future runs, use the successful Build workflow ID for the commit under test.
The device bundle must include `Cartridge/dev/sim-check`. The check downloads
only that selected artifact, copies a small fixture into the VM and saves the
results back under `.sim/vm/results/`. Binary SHA-256, CI revision, OS/kernel,
linked libraries, systemd transition reports and 720×720 screenshots are retained.
No source compilation or mounted host workspace is needed inside the guest.

## Offline root preparation VM

[`installer/host_prepare.py`](../installer/host_prepare.py) uses a second VM,
`cartridge-prep-arm64`, for the Linux-root phase of the SD installer. It is
separate from the compatibility VM above. The Mac first verifies the selected
card's immutable s2 backup, then creates a new `prepared-root.ext4` inside a
workspace outside the card. The preparation VM receives **only that workspace**
as a writable virtiofs mount and a copy of the verified CI device bundle. It
never receives a raw card device or the original backup directory. Its 4 GiB
guest disk does not need to contain the full Linux image; the loop-mounted
image stays on the host's workspace volume. After unmount, both Linux and macOS
run no-write ext4 checks, and macOS reads the exact boot override and supervisor
from the prepared image before any games-partition staging can begin.

The 128 MiB disposable end-to-end host/VM rehearsal passed, including image
hash and boot-file verification; the immutable original retained its SHA-256.
The main ARM suite also passed split preparation and rollback using a real
mounted ext4 image plus synthetic exFAT game, save, gamelist and key fixtures.
To repeat the complete Mac-to-VM transaction on temporary files, run
`python3 sim/vm/host-prepare-check.py /path/to/device-bundle/Cartridge` from
the repository. It saves `.sim/vm/results/host-prepare-check.json` and deletes
its 128 MiB test images afterward. Use a CI bundle built from the same source
revision; the root preparation script in that bundle must support the offline
image handoff.
This is a development backend. No physical SD write or handheld boot has been
validated through it; keep the working card untouched until spare-media tests.

On 2026-09-24, the same dedicated preparation VM also passed against a
**full-size 10,351,525,376-byte copy** of the recovered stock Linux root on an
external SSD. Run [`full-root-check.py`](../sim/vm/full-root-check.py) with a
read-only source image, its recorded SHA-256, an extracted CI `Cartridge` bundle,
and a separate temporary workspace directory on the same volume:

```sh
python3 sim/vm/full-root-check.py \
  --source /path/to/verified/stock-root.ext4 \
  --sha256 RECORDED_SOURCE_SHA256 \
  --bundle /path/to/extracted/Cartridge \
  --workspace-parent /path/to/external-ssd/temporary-workspaces
```

The script checks the original hash and ext4, copies the image, checks the
copy independently, prepares it in the ARM VM, and verifies the resulting
filesystem, startup override, supervisor, stock ES service and recovery link.
It rechecks that the original is unchanged, deletes the temporary copy, and
writes `.sim/vm/results/full-root-check.json`. This run used source hash
`58a57477031efeb3fdc8803882fefc1534bcf71a66d3af57e47faadfaa501c3f`
and produced prepared hash
`914bb4d6ab1f8e5ce9ab7ccf885b38d207249c08d9bce64115bb39f3b6586d56`
from CI bundle revision `56d6b48892f15919a909c80f630d484a06ae9890`.
The workspace was emptied afterward and the VM stopped with no host mount.
The physical card was not accessed. This verifies full-size root preparation,
not the handheld's boot, display or game launch.

For a spare-card trial, `--keep-work-dir /path/to/new-directory` retains the
verified prepared root and its VM manifest on the workspace volume. The
[offline installer notes](offline-installer.md) describe how the test-only
virtual card image uses that root. A retained image uses another 10.35 GB until
removed; the source recovery image stays unchanged.

The first boot downloads a checksum-pinned 217 MiB Ubuntu 24.04 ARM minimal
image and installs SDL runtime libraries. `curl` uses the Mac resolver because
Lima's Go resolver timed out on this network. Apple NAT is configured because
the initial usernet interface could accept SSH but could not reach package
servers. Host DNS settings are unchanged. Stop the VM after testing to release
its 1 GiB RAM. Its disk lives in `~/.lima/cartridge-arm64`; images/artifacts live
in `.sim/vm`. Together these occupied about 1.6 GiB after initial verification.

## What passes

Verified on 2026-09-23 using [Build 35789361899](https://github.com/Strizzo/Cartridge/actions/runs/35789361899)
for commit `af32b8a` (CI merge revision
`3a5743ef463186669408197822b848dadcc78a2c`). The Cartridge binary SHA-256 is
`3774fa0fe0229178ee5b9fbec474462e755776de610c767d5b74b5bb116d69f4`.

- ARM ELF loading and runtime library resolution.
- 27 Python tests for library import, quoting, simulator isolation, launch
  failure, supervisor cleanup, recovery and reversible setup.
- The actual ARM renderer/input code: home/settings/store, system/game browsing,
  simulated game handoff and restoration of a non-first selection, input-aware
  idle wakeup without dropped/duplicated button events, first-frame readiness
  and ES request.
- Twenty-two Lua app scenarios covering loading, offline and populated data,
  including Weather current/forecast/city, news detail and stock period changes.
  Each scenario uses isolated temporary storage and synthetic HTTP fixtures.
- Offline conversion of separate mounted ext4 system and exFAT ROMS images
  using the exact CI app bundle: Cartridge becomes the configured next boot,
  the original ES unit stays intact, game/save/gamelist/key hashes remain
  unchanged, and both filesystems check clean after unmounting. The same fixture
  now rehearses exFAT rollback, re-preparation and a full two-partition install
  transaction with a disposable s2 target. `debugfs` verifies the exact startup
  override and supervisor inside the prepared ext4 image before writeback.
- Real Linux systemd service installation, rendered first-frame/ES handoff,
  forced startup failure, fallback latch and undo. The stock ES service and ES
  executable are explicit test stand-ins; no emulator or ES build is included.

For the offline installer gate, `sim/vm/stock-root-check.py` was also run on a
SHA-256-verified **throwaway copy** of the recovered stock Linux partition,
mounted through a temporary VM-only directory. It paired that root with a
synthetic exFAT ROMS image, applied the exact CI bundle, checked the original ES
service and fixture data, then ran no-write ext4/exFAT checks. A separate
`chroot` check launched the CI ARM executable with `--version` under the stock
system libraries and returned `cartridge 0.5.3`. This passed on
2026-09-23; the temporary host mount was removed and the VM stopped afterward.
The normal `sim/vm.sh check` workflow still uses no host mounts. Do not pass the
original recovery image or a physical card to the stock-root script: its input
image is modified during the rehearsal.

The offline converter and guarded writeback are backend components. A graphical
desktop app that chooses a physical card, runs preparation and safely ejects it
does not exist yet; no physical-card write has been validated. The guest verifier requires
`/etc/cartridge-compat-vm`, created only by VM
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
