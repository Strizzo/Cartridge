#!/bin/bash
# Undo Cartridge Boot Selector
#
# Run this from the EmulationStation Tools menu to remove the boot selector
# and go back to booting directly into EmulationStation.

set -euo pipefail

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

systemctl enable emulationstation.service 2>/dev/null || true
echo "  EmulationStation will start on boot."

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
