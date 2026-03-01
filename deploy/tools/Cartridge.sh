#!/bin/bash
# Launch CartridgeOS from EmulationStation Tools menu
#
# On first run, automatically sets up the boot selector service
# so future boots show the Cartridge/EmulationStation choice screen.

# Detect active roms directory using ArkOS convention
if [ -f "/opt/system/Advanced/Switch to main SD for Roms.sh" ]; then
    ROMS_DIR="/roms2"
else
    ROMS_DIR="/roms"
fi
CARTRIDGE_DIR="${ROMS_DIR}/Cartridge"

if [[ ! -x "${CARTRIDGE_DIR}/cartridge" ]]; then
    echo "CartridgeOS not found at ${CARTRIDGE_DIR}"
    echo "Extract the CartridgeOS zip to your roms/ folder first."
    sleep 5
    exit 1
fi

# ── Auto-setup boot selector on first run ────────────────────────────────────

if ! systemctl is-enabled cartridge-boot.service &>/dev/null; then
    echo ""
    echo "First run detected - setting up CartridgeOS boot selector..."
    echo ""

    # Install the autosetup script if present
    if [[ -x "${CARTRIDGE_DIR}/autosetup.sh" ]]; then
        bash "${CARTRIDGE_DIR}/autosetup.sh" --no-reboot
    fi
fi

# ── Launch CartridgeOS ────────────────────────────────────────────────────────

cd "${CARTRIDGE_DIR}"
export SDL_VIDEODRIVER="${SDL_VIDEODRIVER:-kmsdrm}"
export SDL_AUDIODRIVER="${SDL_AUDIODRIVER:-alsa}"
export HOME="${HOME:-/root}"
exec ./cartridge
