#!/bin/bash
# CartridgeOS Auto-Setup
#
# Automatically configures the boot selector service.
# Called by Cartridge.sh on first run, or can be run manually.
#
# Usage:
#   ./autosetup.sh              # Setup and reboot
#   ./autosetup.sh --no-reboot  # Setup without rebooting

set -euo pipefail

NO_REBOOT=false
[[ "${1:-}" == "--no-reboot" ]] && NO_REBOOT=true

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CARTRIDGE_DIR="${SCRIPT_DIR}"

echo "CartridgeOS Auto-Setup"
echo "======================"

# ── Verify binaries exist ─────────────────────────────────────────────────────

if [[ ! -x "${CARTRIDGE_DIR}/cartridge-boot" ]]; then
    echo "ERROR: cartridge-boot binary not found."
    sleep 3
    exit 1
fi

# ── Find and disable EmulationStation auto-start ──────────────────────────────

echo "[1/3] Scanning for EmulationStation services..."

ES_SERVICES_FOUND=()
for svc_name in \
    emulationstation.service \
    emulationstation-es.service \
    es-start.service \
    arkos-emulationstation.service \
    start_es.service; do
    if systemctl list-unit-files "$svc_name" &>/dev/null; then
        if systemctl is-enabled "$svc_name" 2>/dev/null; then
            ES_SERVICES_FOUND+=("$svc_name")
        fi
    fi
done

# Also search service files for emulationstation references
while IFS= read -r svc_path; do
    if [[ -n "$svc_path" ]] && [[ -f "$svc_path" ]]; then
        if grep -qi "emulationstation" "$svc_path" 2>/dev/null; then
            svc_name="$(basename "$svc_path")"
            already_found=false
            for existing in "${ES_SERVICES_FOUND[@]}"; do
                [[ "$existing" == "$svc_name" ]] && already_found=true && break
            done
            $already_found || ES_SERVICES_FOUND+=("$svc_name")
        fi
    fi
done < <(find /etc/systemd/system/ /usr/lib/systemd/system/ /lib/systemd/system/ -name '*.service' 2>/dev/null)

for svc in "${ES_SERVICES_FOUND[@]}"; do
    echo "  Found ES service: $svc"
done

# ── Install boot selector service ────────────────────────────────────────────

echo "[2/3] Installing boot selector service..."

cat > /etc/systemd/system/cartridge-boot.service << SVCEOF
[Unit]
Description=CartridgeOS Boot Selector
After=multi-user.target
DefaultDependencies=no

[Service]
Type=simple
ExecStart=${CARTRIDGE_DIR}/cartridge-boot.sh
StandardInput=tty
TTYPath=/dev/tty1
Environment=HOME=/root
Environment=SDL_VIDEODRIVER=kmsdrm
Environment=SDL_AUDIODRIVER=alsa

[Install]
WantedBy=multi-user.target
SVCEOF

systemctl daemon-reload
systemctl enable cartridge-boot.service
echo "  Boot selector service enabled."

# ── Disable ES services ─────────────────────────────────────────────────────

echo "[3/3] Disabling EmulationStation auto-start..."

for svc in "${ES_SERVICES_FOUND[@]}"; do
    echo "  Disabling $svc..."
    systemctl disable "$svc" 2>/dev/null || true
    systemctl stop "$svc" 2>/dev/null || true
done
systemctl disable emulationstation.service 2>/dev/null || true

# Save which services we disabled for undo
echo "${ES_SERVICES_FOUND[*]}" > "${CARTRIDGE_DIR}/.disabled_es_services"

echo ""
echo "Setup complete!"

if ! $NO_REBOOT; then
    echo "Rebooting in 3 seconds..."
    sleep 3
    reboot
fi
