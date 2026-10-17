# Installing Cartridge on R36S Plus

## Quick Install (Windows, macOS, Linux)

No build tools required. Works from any computer.

The direct-start instructions below describe this primary-session branch. A
previously published release may still contain the legacy boot selector. Use a
tested bundle that includes `setup-primary.py` and `cartridge-session.py` for
primary startup; do not assume a release contains unmerged branch changes.

1. Download `cartridge-r36s-plus.zip` from the
   [latest release](https://github.com/Strizzo/Cartridge/releases/latest)

2. Turn off your R36S Plus and remove the SD card

3. Insert the SD card into your computer

4. Open the SD card and find the `roms/` folder

5. Extract the zip contents into the `roms/` folder.
   After extraction you should see:
   ```
   roms/
     Cartridge/          <-- new
     tools/
       Cartridge.sh      <-- new
       Setup Cartridge Boot.sh   <-- new
       Undo Cartridge Boot.sh    <-- new
       ... (existing tools)
   ```

6. Eject the SD card and put it back in the device

7. Boot the device — it starts EmulationStation as usual

8. Go to **Tools** in EmulationStation and select **Cartridge** to launch it


## Make Cartridge the primary launcher

1. Verify the tested build launches and works from **Options → Tools → Cartridge**.
2. Run **Options → Tools → Setup Cartridge Boot** once.
3. Setup validates the executable and working stock service, then installs one
   removable startup override. It leaves the current session running.
4. Restart normally from the power menu when ready. The next boot opens Cartridge
   directly; no selector or trip through the ES game categories is required.

The primary-session branch has passed native and ARM VM checks. Its first boot,
display/audio handoff and real game launch still need validation on the working
replacement card. See [primary session](docs/primary-session.md).

Cartridge's Select menu opens EmulationStation when wanted. Startup failure or a
crash also falls back to ES, and latches recovery until Setup is run again after
fixing the problem. To restore stock boot, run **Tools → Undo Cartridge Boot**
and restart normally. This removes only the managed override.

If Cartridge cannot be reached, create an empty file named `boot-emulationstation`
inside its installation directory over SSH or on the ROMS volume. The supervisor
will use ES at the next boot. Remove the file after resolving the issue. No ext4
editing or game deletion is required. These recovery instructions apply to the
new primary session; inspect legacy installations before changing their services.

For further iterations use [wireless deploy, logs and screenshots](docs/wireless-deploy.md)
instead of moving the SD card for every build.


## What's Included

| File/Folder | Purpose |
|---|---|
| `Cartridge/cartridge` | Main Cartridge OS binary |
| `Cartridge/cartridge-session.py` | Primary session supervisor and ES recovery |
| `Cartridge/setup-primary.py` | Reversible primary-session setup |
| `Cartridge/game-library.py` | Read existing games and emulator launch configuration |
| `Cartridge/cartridge-boot` | Legacy boot selector; unused by the new primary session |
| `Cartridge/cartridge-boot.sh` | Legacy boot wrapper |
| `Cartridge/cartridge-boot.service` | Legacy service; not enabled by primary setup |
| `Cartridge/assets/` | Fonts, overlay textures |
| `Cartridge/lua_cartridges/` | Bundled apps (Calculator, Hacker News, etc.) |
| `tools/Cartridge.sh` | Launch Cartridge from ES Tools menu |
| `tools/Setup Cartridge Boot.sh` | Enable direct Cartridge startup (run once) |
| `tools/Undo Cartridge Boot.sh` | Restore stock ES startup |


## Building from Source

For developers who want to build from source (macOS or Linux):

### Prerequisites

- Rust toolchain: https://rustup.rs
- cross-rs: `cargo install cross`
- Docker (required by cross-rs for cross-compilation)
- Python 3 (for generating overlay assets)

### Build and install

```bash
git clone https://github.com/Strizzo/Cartridge.git
cd Cartridge

# Connect your SD card or device, then:
./install_to_device.sh

# Or build only:
cross build --release --target aarch64-unknown-linux-gnu

# Generate overlay textures:
python3 scripts/generate_overlays.py
```


## Troubleshooting

**Cartridge doesn't appear in Tools menu**
- Make sure you extracted the zip into `roms/`, not into a subfolder
- The `tools/Cartridge.sh` file must be at `roms/tools/Cartridge.sh`

**Screen is black when launching Cartridge**
- Test from **Tools > Cartridge** while keeping EmulationStation as the boot launcher.
- Check `Cartridge/launch.log` for the actual startup error. The manual launcher
  saves output there even on systems with journald disabled.
- If the log identifies a missing library or an SDL renderer error, resolve that
  error before enabling boot integration. "Setup Cartridge Boot" configures
  startup services; it does not install SDL2 libraries.

**Cartridge does not start directly after setup**
- Setup takes effect at the next normal boot and never forces a reboot.
- Read `/home/ark/.cartridges/session/session.log` and `last-session.json`.
- A `fallback.json` latch means a prior startup failed; resolve the logged error
  before rerunning Setup. A `Cartridge/boot-emulationstation` file requests ES.
- An unknown stock service or enabled legacy Cartridge boot service causes Setup
  to refuse the change. Preserve the working boot configuration and inspect the
  reported mismatch instead of enabling a second competing startup service.

**Want to go back to EmulationStation only**
- Select EmulationStation from Cartridge, then run **Tools → Undo Cartridge Boot**.
- If Cartridge is unavailable, use the recovery file described above.
- Undo takes effect at the next boot; Cartridge remains available from Tools.

**Apps show text instead of icons**
- This is normal if the app icons haven't been downloaded yet
- Icons are included with the bundled apps in `lua_cartridges/`
