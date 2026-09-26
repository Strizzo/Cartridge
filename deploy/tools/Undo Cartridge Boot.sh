#!/bin/bash
# Remove only our primary-session override. No games or stock services changed.
set -euo pipefail
ROMS_DIR=/roms
[[ -f '/opt/system/Advanced/Switch to main SD for Roms.sh' ]] && ROMS_DIR=/roms2
CARTRIDGE_DIR="${CARTRIDGE_DIR:-${ROMS_DIR}/Cartridge}"
if [[ "$(id -u)" -eq 0 ]]; then
    /usr/bin/python3 "$CARTRIDGE_DIR/setup-primary.py" disable
else
    sudo /usr/bin/python3 "$CARTRIDGE_DIR/setup-primary.py" disable
fi
echo 'Restart from the power menu when ready. Cartridge stays available in Tools.'
sleep 4
