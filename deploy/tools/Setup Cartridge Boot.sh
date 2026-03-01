#!/bin/bash
# Setup Cartridge Boot Selector
#
# Run this from the EmulationStation Tools menu to enable the boot selector.
# On next reboot, you'll be able to choose between Cartridge and EmulationStation.
#
# This script:
#   1. Installs the boot selector as a systemd service
#   2. Reboots the device

# Detect active roms directory using ArkOS convention
if [ -f "/opt/system/Advanced/Switch to main SD for Roms.sh" ]; then
    ROMS_DIR="/roms2"
else
    ROMS_DIR="/roms"
fi
CARTRIDGE_DIR="${ROMS_DIR}/Cartridge"

echo ""
echo "==================================="
echo "  Cartridge Boot Selector Setup"
echo "==================================="
echo ""
echo "  Cartridge dir: ${CARTRIDGE_DIR}"
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

# ── Install boot selector service ────────────────────────────────────────────

echo "[1/2] Installing boot selector service..."

# Generate service file with the correct path for this device
cat > /etc/systemd/system/cartridge-boot.service << SVCEOF
[Unit]
Description=Cartridge Boot Selector
After=multi-user.target
Before=emulationstation.service cartridge.service
DefaultDependencies=no

[Service]
Type=oneshot
RemainAfterExit=no
ExecStart=${CARTRIDGE_DIR}/cartridge-boot.sh
StandardInput=tty
TTYPath=/dev/tty1
Environment=HOME=/root
Environment=SDL_VIDEODRIVER=kmsdrm
Environment=SDL_AUDIODRIVER=alsa

[Install]
WantedBy=multi-user.target
SVCEOF

systemctl daemon-reload
systemctl enable cartridge-boot.service
echo "  Boot selector enabled."

# ── Configure boot order ─────────────────────────────────────────────────────

echo "[2/2] Configuring boot order..."

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
