#!/bin/bash
# Setup Cartridge Boot Selector
#
# Run this from the EmulationStation Tools menu to enable the boot selector.
# On next reboot, you'll be able to choose between Cartridge and EmulationStation.
#
# This script:
#   1. Installs SDL2 runtime libraries (if needed)
#   2. Installs the boot selector as a systemd service
#   3. Reboots the device

set -euo pipefail

CARTRIDGE_DIR="/roms/Cartridge"

echo ""
echo "==================================="
echo "  Cartridge Boot Selector Setup"
echo "==================================="
echo ""

# ── Verify Cartridge is installed ────────────────────────────────────────────

if [[ ! -x "${CARTRIDGE_DIR}/cartridge" ]]; then
    echo "ERROR: Cartridge not found at ${CARTRIDGE_DIR}"
    echo ""
    echo "Extract the Cartridge zip to your SD card's roms/ folder first."
    sleep 5
    exit 1
fi

if [[ ! -x "${CARTRIDGE_DIR}/cartridge-boot" ]]; then
    echo "ERROR: Boot selector binary not found."
    echo ""
    echo "Make sure you extracted the full Cartridge zip."
    sleep 5
    exit 1
fi

# ── Install SDL2 runtime libraries ───────────────────────────────────────────

echo "[1/3] Checking SDL2 libraries..."

sdl2_missing=false
for lib in libSDL2-2.0 libSDL2_ttf libSDL2_gfx libSDL2_image; do
    if ! ldconfig -p 2>/dev/null | grep -qi "$lib"; then
        sdl2_missing=true
        break
    fi
done

if $sdl2_missing; then
    echo "  Installing SDL2 runtime libraries..."
    if command -v apt-get &>/dev/null; then
        apt-get update -qq 2>/dev/null
        apt-get install -y \
            libsdl2-2.0-0 \
            libsdl2-ttf-2.0-0 \
            libsdl2-gfx-1.0-0 \
            libsdl2-image-2.0-0 \
            2>/dev/null || echo "  Warning: Some SDL2 packages may not be available."
    elif command -v pacman &>/dev/null; then
        pacman -S --noconfirm sdl2 sdl2_ttf sdl2_gfx sdl2_image 2>/dev/null || true
    else
        echo "  Warning: Cannot install SDL2 automatically."
        echo "  If Cartridge fails to start, install SDL2 manually."
    fi
    echo "  Done."
else
    echo "  SDL2 already installed."
fi

# ── Install boot selector service ────────────────────────────────────────────

echo "[2/3] Installing boot selector service..."

cp "${CARTRIDGE_DIR}/cartridge-boot.service" /etc/systemd/system/
systemctl daemon-reload
systemctl enable cartridge-boot.service
echo "  Boot selector enabled."

# ── Configure boot order ─────────────────────────────────────────────────────

echo "[3/3] Configuring boot order..."

systemctl disable emulationstation.service 2>/dev/null || true
echo "  EmulationStation auto-start disabled."
echo "  (You can still choose it from the boot selector.)"

# ── Done ─────────────────────────────────────────────────────────────────────

echo ""
echo "==================================="
echo "  Setup complete!"
echo "==================================="
echo ""
echo "On next boot, you'll see the boot selector screen"
echo "where you can choose Cartridge or EmulationStation."
echo ""
echo "To undo this, run 'Undo Cartridge Boot' from Tools."
echo ""
echo "Rebooting in 5 seconds..."
sleep 5
reboot
