#!/bin/bash
# Launch Cartridge from EmulationStation Tools menu
# This script appears as "Cartridge" in the Tools section of ES.

# Resolve Cartridge directory relative to this script's location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROMS_DIR="$(dirname "$SCRIPT_DIR")"
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
