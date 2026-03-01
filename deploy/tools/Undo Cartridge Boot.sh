#!/bin/bash
# Undo Cartridge Boot Selector
#
# Run this from the EmulationStation Tools menu to remove the boot selector
# and go back to booting directly into EmulationStation.

# Detect active roms directory using ArkOS convention
if [ -f "/opt/system/Advanced/Switch to main SD for Roms.sh" ]; then
    ROMS_DIR="/roms2"
else
    ROMS_DIR="/roms"
fi
CARTRIDGE_DIR="${ROMS_DIR}/Cartridge"

echo ""
echo "==================================="
echo "  Undo Cartridge Boot Selector"
echo "==================================="
echo ""

# ── Disable boot selector ────────────────────────────────────────────────────

echo "[1/2] Removing boot selector..."

if systemctl is-enabled cartridge-boot.service &>/dev/null; then
    systemctl disable cartridge-boot.service
    echo "  Boot selector disabled."
else
    echo "  Boot selector was not enabled."
fi

if [[ -f /etc/systemd/system/cartridge-boot.service ]]; then
    rm /etc/systemd/system/cartridge-boot.service
    systemctl daemon-reload
    echo "  Service file removed."
fi

# ── Re-enable EmulationStation ────────────────────────────────────────────────

echo "[2/2] Re-enabling EmulationStation auto-start..."

# Try to restore the services that Setup originally disabled
RESTORED=false
if [[ -f "${CARTRIDGE_DIR}/.disabled_es_services" ]]; then
    while IFS= read -r svc; do
        if [[ -n "$svc" ]]; then
            systemctl enable "$svc" 2>/dev/null && RESTORED=true && echo "  Re-enabled $svc"
        fi
    done < "${CARTRIDGE_DIR}/.disabled_es_services"
    rm -f "${CARTRIDGE_DIR}/.disabled_es_services"
fi

# Also try the common name as fallback
if ! $RESTORED; then
    systemctl enable emulationstation.service 2>/dev/null || true
    echo "  Attempted to re-enable emulationstation.service"
fi

# ── Done ─────────────────────────────────────────────────────────────────────

echo ""
echo "==================================="
echo "  Done!"
echo "==================================="
echo ""
echo "EmulationStation will start directly on next boot."
echo "Cartridge is still available from the Tools menu."
echo ""
echo "Rebooting in 5 seconds..."
sleep 5
reboot
