#!/bin/bash
# Make Cartridge the primary launcher at the next boot; keep ES as fallback.
set -euo pipefail
ROMS_DIR=/roms
[[ -f '/opt/system/Advanced/Switch to main SD for Roms.sh' ]] && ROMS_DIR=/roms2
CARTRIDGE_DIR="${CARTRIDGE_DIR:-${ROMS_DIR}/Cartridge}"
bash "$CARTRIDGE_DIR/autosetup.sh" --no-reboot
echo 'Setup finished. Restart from the power menu when ready.'
sleep 4
