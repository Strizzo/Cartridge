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

The card-writing Mac application still needs to be built. Its Linux helper can
mount a cloned ext4 system partition and the corresponding exFAT ROMS partition,
then use [`installer/offline_prepare.py`](../installer/offline_prepare.py) to
copy the CI app bundle, install the first-boot service override, preserve ES as
recovery and verify games/saves. The helper refuses a live `/` root, unmounted
folders, unsupported stock services and backups placed inside either card
partition. On failure it restores the previous Cartridge-owned files. The Mac
application must retain a durable backup, check the modified filesystems,
write back only the intended partition data and verify the physical card before
ejecting it.

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
