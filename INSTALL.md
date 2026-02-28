# Installing Cartridge on R36S Plus

## Quick Install (Windows, macOS, Linux)

No build tools required. Works from any computer.

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


## Enabling the Boot Selector

The boot selector lets you choose between Cartridge and EmulationStation
every time your device starts up. No SSH or terminal needed.

1. Boot the device into EmulationStation

2. Go to **Tools** and select **Setup Cartridge Boot**

3. The device will install the boot selector and reboot automatically

4. On next boot, you'll see the Cartridge boot screen where you can
   pick Cartridge or EmulationStation

To undo this and go back to booting directly into EmulationStation:
- Go to **Tools > Undo Cartridge Boot** from EmulationStation


## What's Included

| File/Folder | Purpose |
|---|---|
| `Cartridge/cartridge` | Main Cartridge OS binary |
| `Cartridge/cartridge-boot` | Boot selector binary |
| `Cartridge/cartridge-boot.sh` | Boot wrapper script |
| `Cartridge/cartridge-boot.service` | Systemd service for boot selector |
| `Cartridge/assets/` | Fonts, overlay textures |
| `Cartridge/lua_cartridges/` | Bundled apps (Calculator, Hacker News, etc.) |
| `tools/Cartridge.sh` | Launch Cartridge from ES Tools menu |
| `tools/Setup Cartridge Boot.sh` | Enable boot selector (run once) |
| `tools/Undo Cartridge Boot.sh` | Disable boot selector |


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
- Run "Setup Cartridge Boot" from Tools first — it installs SDL2 libraries
- If still black, the device may need SDL2 libs installed manually

**Boot selector doesn't appear after setup**
- Make sure you ran "Setup Cartridge Boot" from the Tools menu
- The device needs to reboot for the boot selector to take effect

**Want to go back to EmulationStation only**
- Run "Undo Cartridge Boot" from the Tools menu
- Or if you can't reach the Tools menu, remove the SD card and delete
  `Cartridge/cartridge-boot.service` from the `roms/` folder

**Apps show text instead of icons**
- This is normal if the app icons haven't been downloaded yet
- Icons are included with the bundled apps in `lua_cartridges/`
