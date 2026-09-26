# CartridgeOS updates over Wi-Fi

The spare-card trial currently updates Cartridge by copying a verified ARM
bundle from a Mac. Installed store apps already have a network installer, but
the launcher executable and bundled assets have no on-device update path.
The first online updater should change only Cartridge-owned files on EASYROMS.
It must not rewrite BOOT, the Linux root, the partition table, ROMs or saves.
Boot graphics and kernel/driver updates remain offline until they have an
independent recovery design.

## Release format

CI should publish a versioned `aarch64` bundle to GitHub Releases, alongside a
small signed manifest containing: release channel and version, build revision,
supported board/panel variants, minimum installed version, archive size and
SHA-256, per-file hashes, and update schema version. The device embeds a public
verification key. HTTPS delivers the files; the manifest signature and hashes
decide whether the download is trusted. A GitHub Actions artifact URL is a
development handoff, not a stable update endpoint. Keep personal settings,
Wi-Fi credentials, saves and SSH keys out of the release bundle.

## Device transaction

1. Check for a release on a background thread and show its size, version and
   changes in Settings. Do not block input or redraw continuously while waiting
   for network I/O. Check battery level and free space before downloading.
2. Download into a new version directory under `/roms/Cartridge/releases/`.
   Resume if supported; never write over the active executable. Verify the
   signed manifest, archive hash and extracted file hashes, then sync the files.
3. Keep the current release available. Store the active and previous release
   IDs in one atomically replaced state file on the Linux filesystem; do not
   rely on symlinks on exFAT. The session supervisor currently starts the fixed
   `/roms/Cartridge/cartridge` path, so it must learn to select a verified
   release directory before this transaction can ship.
4. On the next Cartridge restart, start the candidate release with the existing
   first-frame deadline. If it fails, restore the previous release and retry
   once. If both fail, keep the existing EmulationStation recovery path. Record
   the outcome for the update screen. A successful first frame promotes the
   candidate; keep the previous release until a later successful boot.
5. Never auto-apply a BOOT logo, DTB, kernel, systemd or Linux-root change via
   this first updater. Those files have different failure and rollback modes.

## Validation before enabling downloads

The Mac simulator can serve a local test manifest and inject interrupted
downloads. The ARM VM should exercise signature rejection, changed card data,
out-of-space, power loss at every state transition, first-frame timeout and
rollback to the previous release. Then test the complete update over the
handheld's actual Wi-Fi connection, including a failed release that returns
to the known-good version. Until the current Wi-Fi hardware is detected and a
real scan succeeds, the device update UI should remain a later milestone.

The initial user-facing policy is to check for updates when connected and let
the owner choose when to download/install. This avoids a surprise restart or
large transfer on a battery-powered handheld.
