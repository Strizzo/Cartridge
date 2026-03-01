#!/bin/bash
# Launch Cartridge from EmulationStation Tools menu
# This script appears as "Cartridge" in the Tools section of ES.

# Detect active roms directory using ArkOS convention
if [ -f "/opt/system/Advanced/Switch to main SD for Roms.sh" ]; then
    ROMS_DIR="/roms2"
else
    ROMS_DIR="/roms"
fi
CARTRIDGE_DIR="${ROMS_DIR}/Cartridge"

if [[ ! -x "${CARTRIDGE_DIR}/cartridge" ]]; then
    echo "Cartridge not found at ${CARTRIDGE_DIR}"
    echo "Extract the Cartridge zip to your roms/ folder first."
    sleep 5
    exit 1
fi

cd "${CARTRIDGE_DIR}"
export SDL_VIDEODRIVER="${SDL_VIDEODRIVER:-kmsdrm}"
export SDL_AUDIODRIVER="${SDL_AUDIODRIVER:-alsa}"
export HOME="${HOME:-/root}"
exec ./cartridge
