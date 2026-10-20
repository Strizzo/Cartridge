#!/bin/bash
# Launch CartridgeOS from EmulationStation without changing boot services.
# Boot setup is a separate, explicit action: Tools > Setup Cartridge Boot.

# Detect active roms directory using ArkOS convention.
if [ -f "/opt/system/Advanced/Switch to main SD for Roms.sh" ]; then
    ROMS_DIR="/roms2"
else
    ROMS_DIR="/roms"
fi
# The override also lets the manual launch path be tested off-device.
CARTRIDGE_DIR="${CARTRIDGE_DIR:-${ROMS_DIR}/Cartridge}"

if [[ ! -f "${CARTRIDGE_DIR}/cartridge" ]]; then
    echo "CartridgeOS not found at ${CARTRIDGE_DIR}"
    echo "Extract the CartridgeOS zip to your roms/ folder first."
    sleep 5
    exit 1
fi

# exFAT does not preserve Unix execute bits.
chmod +x "${CARTRIDGE_DIR}/cartridge" 2>/dev/null
cd "${CARTRIDGE_DIR}" || exit 1
export SDL_VIDEODRIVER="${SDL_VIDEODRIVER:-kmsdrm}"
export SDL_AUDIODRIVER="${SDL_AUDIODRIVER:-alsa}"
export HOME="${HOME:-/root}"
export CARTRIDGE_ASSETS="${CARTRIDGE_DIR}/assets"
export RUST_LOG="${RUST_LOG:-cartridge=info,cartridge_launcher=info,cartridge_core=info,cartridge_lua=info}"

# This handheld can have journald disabled; retain startup errors beside the
# app so they remain accessible when the card is connected to a computer.
LOG_FILE="${CARTRIDGE_DIR}/launch.log"
if ! : >> "${LOG_FILE}"; then
    echo "Cannot write Cartridge launch log: ${LOG_FILE}"
    exit 1
fi
{
    printf '\n=== Cartridge launch: %s ===\n' "$(date)"
    printf 'Working directory: %s\nVideo driver: %s\n' "$PWD" "$SDL_VIDEODRIVER"
    ./cartridge "$@"
} >> "${LOG_FILE}" 2>&1
STATUS=$?
# These are intentional session handoffs, not errors. ES already owns the
# parent session when this entry point is launched from its Tools menu.
[[ "$STATUS" -eq 20 || "$STATUS" -eq 30 ]] && STATUS=0

if [[ "$STATUS" -ne 0 ]]; then
    echo "Cartridge exited with code ${STATUS}. Returning to EmulationStation."
    echo "Details saved in ${LOG_FILE}:"
    tail -n 12 "${LOG_FILE}"
    sleep 5
fi
exit "$STATUS"
