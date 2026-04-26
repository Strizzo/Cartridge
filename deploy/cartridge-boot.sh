#!/bin/bash
# cartridge-boot.sh -- Boot selector wrapper for ArkOS / handheld devices
#
# Runs the graphical boot selector, reads the choice, and launches
# the appropriate environment (Cartridge OS or EmulationStation).
#
# Exit codes from cartridge-boot:
#   0 = Cartridge OS
#   1 = EmulationStation

set -uo pipefail

# Resolve paths relative to this script's location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CARTRIDGE_DIR="${SCRIPT_DIR}"

# Fix execute permissions (exFAT/NTFS don't preserve Unix bits)
chmod +x "${CARTRIDGE_DIR}/cartridge" 2>/dev/null || true
chmod +x "${CARTRIDGE_DIR}/cartridge-boot" 2>/dev/null || true

BOOT_BIN="${CARTRIDGE_DIR}/cartridge-boot"
CARTRIDGE_BIN="${CARTRIDGE_DIR}/cartridge"
CHOICE_FILE="/tmp/.cartridge_boot_choice"

# ArkOS EmulationStation launch script (verified from device)
ES_SCRIPT="/usr/bin/emulationstation/emulationstation.sh"

# SDL environment for direct framebuffer rendering (no X11/Wayland)
export SDL_VIDEODRIVER="${SDL_VIDEODRIVER:-kmsdrm}"
export SDL_AUDIODRIVER="${SDL_AUDIODRIVER:-alsa}"
export HOME="${HOME:-/root}"

# Point to assets directory
if [[ -d "${CARTRIDGE_DIR}/assets" ]]; then
    export CARTRIDGE_ASSETS="${CARTRIDGE_DIR}/assets"
fi

# Set working directory so binaries can find assets/ and lua_cartridges/
cd "${CARTRIDGE_DIR}"

# Run the graphical boot selector (if available)
if [[ -f "$BOOT_BIN" ]]; then
    "$BOOT_BIN"
    EXIT_CODE=$?
else
    echo "[cartridge-boot] Boot selector binary not found, defaulting to Cartridge"
    EXIT_CODE=0
fi

# Read the choice file as fallback (exit code is primary signal)
CHOICE="cartridge"
if [ -f "${CHOICE_FILE}" ]; then
    CHOICE=$(cat "${CHOICE_FILE}")
fi

# Also check exit code in case file write failed
if [ "${EXIT_CODE}" -eq 1 ]; then
    CHOICE="emulationstation"
fi

case "${CHOICE}" in
    emulationstation)
        echo "[cartridge-boot] Launching EmulationStation"
        if [ -f "${ES_SCRIPT}" ]; then
            exec bash "${ES_SCRIPT}"
        else
            echo "[cartridge-boot] ES script not found at ${ES_SCRIPT}, falling back to Cartridge"
            exec "${CARTRIDGE_BIN}"
        fi
        ;;
    *)
        echo "[cartridge-boot] Launching Cartridge OS"
        if [[ -f "$CARTRIDGE_BIN" ]]; then
            exec "$CARTRIDGE_BIN"
        else
            echo "[cartridge-boot] Cartridge binary not found!"
            exit 1
        fi
        ;;
esac
