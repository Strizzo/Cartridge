#!/bin/bash
# Launch Cartridge from EmulationStation Tools menu
# This script appears as "Cartridge" in the Tools section of ES.

# Search known ArkOS mount points for Cartridge
CARTRIDGE_DIR=""
for dir in /roms2/Cartridge /roms/Cartridge /opt/system/Cartridge; do
    if [[ -x "${dir}/cartridge" ]]; then
        CARTRIDGE_DIR="$dir"
        break
    fi
done

if [[ -z "$CARTRIDGE_DIR" ]]; then
    echo "Cartridge not found."
    echo "Extract the Cartridge zip to your roms/ folder first."
    sleep 5
    exit 1
fi

cd "${CARTRIDGE_DIR}"
export SDL_VIDEODRIVER="${SDL_VIDEODRIVER:-kmsdrm}"
export SDL_AUDIODRIVER="${SDL_AUDIODRIVER:-alsa}"
export HOME="${HOME:-/root}"
exec ./cartridge
