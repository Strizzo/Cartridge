#!/bin/bash
# Setup Cartridge Boot Selector
#
# Run this from the EmulationStation Tools menu to enable the boot selector.
# On next reboot, you'll be able to choose between Cartridge and EmulationStation.

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

# Fix execute permissions (exFAT doesn't preserve Unix bits)
chmod +x "${CARTRIDGE_DIR}/cartridge" 2>/dev/null
chmod +x "${CARTRIDGE_DIR}/cartridge-boot" 2>/dev/null
chmod +x "${CARTRIDGE_DIR}/cartridge-boot.sh" 2>/dev/null
chmod +x "${CARTRIDGE_DIR}/autosetup.sh" 2>/dev/null

if [[ ! -f "${CARTRIDGE_DIR}/cartridge-boot" ]]; then
    echo "ERROR: Boot selector binary not found at ${CARTRIDGE_DIR}"
    echo "Extract the full CartridgeOS zip to your roms/ folder first."
    sleep 5
    exit 1
fi

# Delegate to autosetup.sh which handles everything
if [[ -f "${CARTRIDGE_DIR}/autosetup.sh" ]]; then
    bash "${CARTRIDGE_DIR}/autosetup.sh"
else
    echo "ERROR: autosetup.sh not found at ${CARTRIDGE_DIR}"
    sleep 5
    exit 1
fi
