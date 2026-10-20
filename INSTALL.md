# Installing CartridgeOS on R36S Plus

## Experimental software: read before installing

CartridgeOS is a prototype, provided as-is and without warranty. Installing
software or changing startup configuration can cause data loss or leave an SD
card unbootable. Compatibility and data preservation are not guaranteed. See
[LICENSE](LICENSE) for the license terms.

Keep an independent backup of your working card, **including games and saves**,
before making changes. Prefer a spare card for experiments. The application
bundle is intended to leave games in place; that is not a substitute for a backup.

The bundled card in the project's original handheld returned inconsistent data
across repeated reads and had corrupted system files. Replacing it was part of
getting the device working again. Back up the supplied card while it works;
do not keep your only recovery copy on that same card. This experience does not
establish a failure rate for all bundled cards.

## What this download supports

These instructions describe the **v0.7.0 application bundle**, developed and tested
on an R36S Plus with an existing working Linux/EmulationStation setup. Other
models, similarly named hardware and firmware variants have not been validated.
Primary startup setup checks the expected stock service and refuses unrecognized
layouts. Do not bypass that check.

- Start with a card that already boots EmulationStation and runs games. Confirm
  that its network works too if you plan to use online apps.
- The ZIP adds Cartridge to that system. It is **not a complete operating-system
  image**, cannot initialize an empty card, and does not repair a broken base
  system or its drivers.
- The macOS image installer is a prototype for verified local images and empty
  cards. A general-purpose public image installer is not included in this release.
- The default `main` branch now contains the released v0.7.0 runtime and the
  current installation guidance. The immutable
  [v0.7.0 tag](https://github.com/Strizzo/Cartridge/tree/v0.7.0) identifies the
  release source. Older boot-selector scripts have different behavior and must
  not be mixed with this procedure.

## Add Cartridge while retaining normal startup

1. Make the backup described above. Download `cartridge-r36s-plus.zip` from the
   [v0.7.0 release](https://github.com/Strizzo/Cartridge/releases/tag/v0.7.0).
   Use the ZIP for this initial install; the `cartridgeos-…tar.gz` file is an
   update payload for an already bootstrapped installation.

   The v0.7.0 ZIP's SHA-256 is:

   ```text
   7f01a61d924ef4017579a403508d1fb878d968c1d6b9928356d03af78a2014c7
   ```

   On macOS, run `shasum -a 256 /path/to/cartridge-r36s-plus.zip` and compare
   the result. This checksum applies only to the v0.7.0 ZIP.

2. Power off the handheld normally, remove its card, and insert it into your
   computer. Open the existing ROMS volume, often labelled `EASYROMS`. It may
   appear directly as a volume of game folders rather than a folder named `roms`.
   If you cannot identify the correct volume, stop before copying anything.

3. Extract the ZIP on your computer. Copy its **`Cartridge/` folder** to the root
   of the ROMS volume. If you already have a Cartridge installation, keep a
   separate backup of that folder before replacing its files.

4. Copy the three `.sh` files from **`Cartridge/tools/` into the existing `tools/`
   folder at the root of the ROMS volume**. Keep the other tools. The ZIP does
   not put these scripts there automatically. The resulting layout is:

   ```text
   EASYROMS/
     Cartridge/
       cartridge
       setup-primary.py
       cartridge-session.py
       assets/
       lua_cartridges/
       tools/                         # source copies of the menu scripts
     tools/
       Cartridge.sh
       Setup Cartridge Boot.sh
       Undo Cartridge Boot.sh
       ...                            # existing tools
     ...                              # existing games and saves
   ```

5. Safely eject the card, put it back in the powered-off handheld, then boot
   normally into EmulationStation. Open **Options → Tools → Cartridge** (some
   themes show Tools directly). With the v0.7.0 scripts this launches Cartridge
   without changing the startup service.

6. Confirm that the launcher and the apps you need work, and that you can return
   to EmulationStation from Cartridge's Select menu. Resolve any errors before
   making it the default launcher.

Installing this application bundle requires no changes to the BOOT partition,
kernel, DTB or partition layout.

## Optional: start directly in Cartridge

After the manual launch check succeeds:

1. Run **Options → Tools → Setup Cartridge Boot**.
2. Setup validates the installation and existing stock EmulationStation service.
   On a recognized system it installs the session supervisor and one removable
   service override. If it stops with an error, save the message and investigate;
   do not disable services or force the installation.
3. Restart normally from the power menu. Setup does not reboot automatically.
   The next boot starts Cartridge directly, with EmulationStation as fallback.

The supervisor falls back to EmulationStation if Cartridge fails to start or
crashes. A failure latch keeps subsequent boots in recovery until the problem is
resolved and Setup is run again. This relies on the existing stock system being
healthy; it cannot recover a broken kernel or unreadable card.

For implementation details, see the release's
[primary-session guide](https://github.com/Strizzo/Cartridge/blob/v0.7.0/docs/primary-session.md).

## Return to EmulationStation and undo

- From Cartridge's Select menu, choose EmulationStation. Then run
  **Tools → Undo Cartridge Boot** to restore stock startup at the next normal
  boot. Cartridge stays installed and available from Tools.
- If Cartridge cannot be reached, power off and connect the card to your
  computer. Create an empty file named exactly **`boot-emulationstation`** inside
  the `Cartridge/` folder on the ROMS volume. Safely eject and boot again; the
  primary supervisor will choose EmulationStation. You can then run Undo.
  Remove the recovery file when you want Cartridge to start again.
- Deleting the payload copy of `Cartridge/cartridge-boot.service` does **not**
  undo an installed Linux service override. The recovery file and Undo tool
  above apply to the v0.7.0 primary-session setup. Legacy installations need
  their own configuration inspected first.

## Updates, development and troubleshooting

- **Routine updates:** after the trusted initial install and updated supervisor
  are in place, use Settings → System Update for signed Cartridge runtime
  updates, and the Store for app updates. Linux, drivers and the kernel are
  outside that updater. See
  [the update guide](https://github.com/Strizzo/Cartridge/blob/v0.7.0/docs/online-updates.md)
  for power, space and bootstrap requirements.
- **No Tools entry:** check the three scripts really are in the volume's root
  `tools/` folder, not only in `Cartridge/tools/`.
- **Black screen or failed launch:** keep stock startup while investigating.
  Read `Cartridge/launch.log` on the ROMS volume. Boot Setup configures startup;
  it does not install missing SDL libraries or fix display drivers.
- **Failed primary startup:** session details are under
  `/home/ark/.cartridges/session/` on Linux, including `session.log`,
  `last-session.json` and, after a failure, `fallback.json`.
- **Report an issue:** include your exact model, base firmware/image, Cartridge
  version, the action that failed and relevant logs. Remove passwords, network
  credentials and other private information before sharing logs.
- **Try it on a computer first:** use the release's
  [simulator guide](https://github.com/Strizzo/Cartridge/blob/v0.7.0/docs/simulator.md)
  and [app development guide](https://github.com/Strizzo/Cartridge/blob/v0.7.0/docs/app-development.md).
  The simulator runs the real app code; device drivers and physical performance
  still need testing on the handheld.
