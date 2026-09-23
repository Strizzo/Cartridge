# One-step SD installer for CartridgeOS

The product installation flow is: insert an SD card into a computer, select the
correct handheld and card in a graphical installer, preserve an existing game
card by default or explicitly choose a full image for a blank/erased card, verify
the result, eject, then power on directly into Cartridge. Both paths configure
the next boot offline. EmulationStation's Options/Tools menu is a development
path, not the shipping first-boot experience.

This is a **planned deliverable**. The primary-session branch and CI ARM bundle
are not a bootable card image. Copying the current app bundle to the visible ROMS
partition alone cannot change a stock card's Linux startup service. The first
image must include the board boot files, Linux system and Cartridge startup
configuration before it is written to the card.

## Fresh card

1. Build a clean, versioned R36S Plus image from a pinned and licensed system
   base. Include the board-matched bootloader, kernel, DTB, firmware, userspace,
   Cartridge, bundled apps, emulator support and recovery entry. Configure
   Cartridge as the first interactive session inside the image. Leave ROMS/save
   storage empty; no personal data ships with the image.
2. Publish an image manifest with release version, supported board/panel
   variants, exact image size, partition map, hashes for the image and critical
   boot files, bundled application revision and known recovery path.
3. The desktop installer enumerates removable disks and displays their size,
   model and partition names. The user explicitly selects the target. The app
   checks capacity and hardware choice, downloads a verified release, displays
   the data that will be replaced, writes only the selected device, reads it
   back, checks partitions and filesystems, then safely ejects it.
4. On first boot, the system expands its data partition to the card's capacity,
   creates device-local settings, and starts Cartridge. Network credentials,
   SSH keys and user state are generated or configured on the device, never
   baked into the published image.

The first desktop target is macOS, where we can test the full workflow with the
existing Mac simulator and a disposable/spare SD. Keep image validation and
device selection in a portable backend so Windows and Linux installers can use
the same format and checks. The graphical shell should report each write and
verification stage and make the selected physical disk unmistakable.

## Existing card with games and saves

**Keep everything is the default.** If the card has a compatible working Linux
system, the installer updates Cartridge-owned files in ROMS and configures that
system's startup service offline. It does not reformat ROMS, replace the
partition table or require a Tools-menu step. ROMs, BIOS, saves, gamelists,
emulator configuration and user keys stay at their current paths. The installer
backs up every file it will replace and verifies the result before presenting
success. If the stock boot layout is unrecognized, it stops before changing the
card and offers a clear explanation.

If the card is blank, the installer offers a fresh image. A populated card may
also be fully erased **only when the user selects an explicit erase/reformat
option**. That path first inventories personal data and offers a verified backup
and restore; it clearly identifies partitions and data that will be replaced.
A spare card is the preferred first migration target because the original can
be booted until game launch and saves are checked.

The read-only macOS inventory is [`installer/card_inventory.py`](../installer/card_inventory.py).
It lists external whole disks, blocks internal, virtual and non-removable media,
and identifies a three-partition BOOT/Linux/EASYROMS layout as a *preserve
candidate*. It does not select a target automatically, unmount a disk, or write
anything. For example, `python3 installer/card_inventory.py --disk disk6` reports
the chosen disk and an inventory fingerprint. Disk identifiers can change when
media is reinserted, so the eventual writer must recheck the whole-disk identity
and layout immediately before any write. A matching partition layout alone does
not prove that the Linux system can boot Cartridge; the cloned-root inspection
remains mandatory. An unpartitioned removable disk is only a fresh-image
candidate, not proof that it contains no recoverable data.

[`installer/card_clone.py`](../installer/card_clone.py) is the read-only backup
backend. It requires the selected whole-disk ID and the fingerprint from
inventory, rejects a backup destination on that card, and copies only the
unmounted Linux partition into a new backup directory. It reads the source
twice, verifies the saved bytes, runs a no-write ext4 check, and records an
incomplete manifest if a pass fails. This passed on a disposable 64 MiB ext4
image with a real `e2fsck`; it has **not** read or written the user's physical
card.

[`installer/card_writeback.py`](../installer/card_writeback.py) now provides the
guarded Linux-partition write stage. It accepts only a clone with a verified
manifest for the selected card, checks the original and prepared image sizes and
SHA-256 hashes, requires both images and a new recovery journal outside the
card, and checks the prepared ext4 filesystem. It opens only the raw `s2`
partition, compares its current bytes with the original clone before writing,
rechecks the inventory fingerprint, records `write_started` durably, then writes,
flushes, reads back, and checks ext4 again. A handled error attempts a verified
rollback from the original clone; an interrupted process leaves a journal for
an explicit `restore`. BOOT, the partition table and EASYROMS are outside this
stage. Disposable-file tests cover success, changed target, bad inputs,
partial-write rollback, interrupted-write restoration and a real ext4 image.
**This writer has not run on a physical SD card.** The inventory fingerprint
identifies layout and volume metadata, not a guaranteed unique card serial;
the full-partition pre-write hash is the stronger guard against a swapped or
changed card.

[`installer/offline_prepare.py`](../installer/offline_prepare.py) copies the CI
bundle onto the selected offline exFAT partition and configures the cloned ext4
root to start Cartridge. It backs up every Cartridge-owned file before changing
it; its `restore_roms` backend can resume a later rollback after the Linux
root has been confirmed unchanged or fully restored.
Rollback accepts only managed Cartridge/Tools paths, verifies original backups
and refuses to overwrite files changed by another process. Games, saves and
keys are outside its allowed path set.

The [Mac preparation handoff](../installer/host_prepare.py) now separates these
steps. Its `root` phase checks a verified clone and selected card identity,
copies that clone into a **new workspace outside the card**, and runs only the
copy in a dedicated ARM VM. The VM shares only that workspace; it mounts the
ext4 file through a loop device, installs the startup override, unmounts it,
checks ext4 and records the image hash. macOS independently checks that hash,
ext4, the exact startup override and the unchanged original clone. Its `roms`
phase then checks the selected mounted games partition and stages only
Cartridge-owned files. A mismatch in the selected card, bundle or root image
stops before staging. This backend has passed on a disposable 128 MiB clone
through the actual host-to-VM share, and the separate ARM VM suite rehearses
split root/ROMS preparation and rollback on mounted ext4/exFAT images. The
repeatable [`host-prepare-check.py`](../sim/vm/host-prepare-check.py) smoke check
also completes the Mac/VM handoff, host ROMS staging and guarded s2 writeback
against disposable files, then checks the original root and game/save/key
hashes. It takes a CI bundle from the same code revision and never inventories
or opens a physical disk.

The backend remains a developer CLI, not a graphical one-step installer. The
intended sequence is read-only `card_inventory.py`, read-only `card_clone.py`,
`host_prepare.py root`, `host_prepare.py roms`, then
`install_transaction.py`. The root workspace needs free space for one more
Linux partition image plus 1 GiB. A failed `roms` phase restores only managed
Cartridge files; the original s2 backup stays immutable. **Do not run the
staging or writeback phases on the working card yet.** The spare-card boot and
recovery gate remains open.

[`installer/install_transaction.py`](../installer/install_transaction.py)
coordinates the two partitions after preparation and unmounting. It checks that
the mounted exFAT partition belongs to the selected card and still contains the
prepared files, verifies every ROMS rollback backup, and reads the unmounted
ext4 image with `debugfs` to confirm the exact Cartridge boot override, session
supervisor, stock ES service and recovery enable link. It then calls the guarded
s2 writer. If s2 is unchanged or its rollback is verified, a failed write also
restores the Cartridge-owned exFAT files. If s2 recovery is incomplete, it
leaves those files available and reports the card as needing recovery. Neither
path formats the card or edits ROMs/saves.

The [full-size stock-root rehearsal](../sim/vm/full-root-check.py) has now
prepared and independently verified a 10.35 GB copy of the recovered Linux
partition through the actual Mac-to-ARM-VM handoff. It preserved the original
image checksum and stock ES recovery service, and removed its temporary copy.
The [VM instructions](arm-vm.md#offline-root-preparation-vm) include the command
and hashes. This strengthens the Linux-root preparation gate; it does not test
ROMS staging or raw writeback on a physical card.

This backend passed a complete ARM VM transaction using separate disposable
ext4 and exFAT images plus a CI device bundle. The test performed preparation,
an exFAT rollback and re-prepare, wrote the prepared root to a disposable s2
file, read it back, verified the boot files inside it, and checked both
filesystems and game/save/key hashes. The normal desktop simulator and ARM VM
still cannot emulate the handheld's exact board and display. **No physical SD
card was written or booted by this transaction.** The Mac graphical installer,
safe eject, fresh bootable image and spare-card
first-boot validation remain to be built before this can be offered to users.

The converter has passed unit checks and an ARM VM check on separate mounted
ext4 and exFAT images using the actual CI bundle. A second VM rehearsal used a
SHA-256-verified **copy** of the recovered stock Linux partition
(`58a57477031efeb3fdc8803882fefc1534bcf71a66d3af57e47faadfaa501c3f`
before conversion) and a small synthetic exFAT games partition. It confirmed
that the real stock service and its recovery logging override accept the offline
Cartridge setup, ES remains available, fixture ROM/save/gamelist/key bytes stay
unchanged, and both filesystems check clean. The CI ARM executable also returns
`cartridge 0.5.3` when launched with `--version` inside the cloned stock
userspace, confirming that this binary starts with its dynamic loader and
libraries. This does not test its display/audio startup. The rehearsal script is
[`sim/vm/stock-root-check.py`](../sim/vm/stock-root-check.py); it changes its
input root image, so only a disposable copy may be supplied. These checks did
not write to the user's card or boot a physical handheld.

## Release gates

- A clean, redistributable image builds from documented inputs and carries no
  ROMs, saves, account data, Wi-Fi credentials, keys or recovery captures.
- The installer identifies blank versus populated cards. A compatible
  populated card defaults to preservation and becomes Cartridge-first without
  an on-device setup step. Formatting requires an empty card or an explicit
  user-selected erase option.
- The installer refuses ambiguous/internal targets, records the selected disk
  identity, shows the data-loss scope and verifies all written data. Interrupted
  writes report an incomplete card rather than claiming success.
- A spare R36S Plus boots directly into Cartridge on the first insertion, with
  working controls, graphics, audio, apps, games, shutdown and ES recovery.
- Games and saves restored from a test library remain byte-for-byte identical
  and launch through the existing emulator settings.
- A failed Cartridge startup enters recovery, and a tested procedure restores
  the previous working system.

Development continues through the [simulator and ARM VM](arm-vm.md) and then
wireless on-device tests. Those checks help build the image; they do not replace
the spare-card first-boot gate.
