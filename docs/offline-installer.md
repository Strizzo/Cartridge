# One-step SD installer for CartridgeOS

The product installation flow is: insert an SD card into a computer, select the
correct handheld and card in a graphical installer, write and verify the image,
eject, then power on the handheld directly into Cartridge. EmulationStation's
Options/Tools menu is a development and migration path, not the shipping first
boot experience.

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

An image write replaces the card's partition map. The installer must detect an
existing game library before offering that operation. For migration, inventory
ROMs, BIOS, saves, gamelists and emulator configuration; back them up to another
volume; verify the backup; write the new image; restore the data; then verify it
again. Present the destination capacity and any unsupported file/path mappings
before the write. A spare card is the recommended first migration target so the
original remains bootable until game launch and saves are checked.

An in-place conversion is a separate installer mode with different recovery
requirements. It cannot be implemented by copying into the exFAT ROMS partition
alone because normal startup is configured on Linux. The current on-device
`setup-primary.py` is the reversible conversion mechanism for the development
build. The public desktop installer should not write ext4 through macOS
`debugfs`, patch a live partition table ad hoc, or depend on an unverified
first-boot hook. Design and validate an offline conversion mechanism on
throwaway images before exposing it to users.

## Release gates

- A clean, redistributable image builds from documented inputs and carries no
  ROMs, saves, account data, Wi-Fi credentials, keys or recovery captures.
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
