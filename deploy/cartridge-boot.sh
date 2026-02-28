#!/bin/bash
# cartridge-boot.sh -- Boot selector wrapper for ArkOS / handheld devices
#
# Runs the graphical boot selector, reads the choice, and launches
# the appropriate environment (Cartridge OS or EmulationStation).
#
# Exit codes from cartridge-boot:
#   0 = Cartridge OS
#   1 = EmulationStation

set -euo pipefail

# Resolve paths relative to this script's location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CARTRIDGE_DIR="${SCRIPT_DIR}"

# Binaries may be in the same directory or in a known system location
find_binary() {
    local name="$1"
    # Same directory as this script
    if [[ -x "${CARTRIDGE_DIR}/${name}" ]]; then
        echo "${CARTRIDGE_DIR}/${name}"
        return
    fi
    # System path
    if command -v "$name" &>/dev/null; then
        command -v "$name"
        return
    fi
    # /usr/local/bin fallback
    if [[ -x "/usr/local/bin/${name}" ]]; then
        echo "/usr/local/bin/${name}"
        return
    fi
    echo ""
}

BOOT_BIN="$(find_binary cartridge-boot)"
CARTRIDGE_BIN="$(find_binary cartridge)"
CHOICE_FILE="/tmp/.cartridge_boot_choice"

# SDL environment for direct framebuffer rendering (no X11/Wayland)
export SDL_VIDEODRIVER="${SDL_VIDEODRIVER:-kmsdrm}"
export SDL_AUDIODRIVER="${SDL_AUDIODRIVER:-alsa}"

# Point to assets directory
if [[ -d "${CARTRIDGE_DIR}/assets" ]]; then
    export CARTRIDGE_ASSETS="${CARTRIDGE_DIR}/assets"
fi

# Set working directory so binaries can find assets/ and lua_cartridges/
cd "${CARTRIDGE_DIR}"

# Run the graphical boot selector (if available)
if [[ -n "$BOOT_BIN" ]]; then
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
        if command -v emulationstation &>/dev/null; then
            exec emulationstation
        elif [ -x /usr/bin/emulationstation ]; then
            exec /usr/bin/emulationstation
        elif [ -x /opt/emulationstation/emulationstation ]; then
            exec /opt/emulationstation/emulationstation
        else
            echo "[cartridge-boot] EmulationStation not found, falling back to Cartridge"
            if [[ -n "$CARTRIDGE_BIN" ]]; then
                cd "${CARTRIDGE_DIR}"
                exec "$CARTRIDGE_BIN"
            fi
        fi
        ;;
    *)
        echo "[cartridge-boot] Launching Cartridge OS"
        if [[ -n "$CARTRIDGE_BIN" ]]; then
            cd "${CARTRIDGE_DIR}"
            exec "$CARTRIDGE_BIN"
        else
            echo "[cartridge-boot] Cartridge binary not found!"
            exit 1
        fi
        ;;
esac
