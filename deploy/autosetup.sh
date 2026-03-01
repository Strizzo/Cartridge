#!/bin/bash
# CartridgeOS Auto-Setup
#
# Automatically configures the boot selector service.
# Called by Cartridge.sh on first run, or can be run manually.
#
# Usage:
#   ./autosetup.sh              # Setup and reboot
#   ./autosetup.sh --no-reboot  # Setup without rebooting

set -eo pipefail

NO_REBOOT=false
[[ "${1:-}" == "--no-reboot" ]] && NO_REBOOT=true

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CARTRIDGE_DIR="${SCRIPT_DIR}"
LOG_FILE="${CARTRIDGE_DIR}/setup.log"

log() { echo "$1" | tee -a "$LOG_FILE"; }

log ""
log "CartridgeOS Auto-Setup"
log "======================"
log "Date: $(date)"
log "User: $(whoami)"
log "Dir:  ${CARTRIDGE_DIR}"

# ── Fix permissions (exFAT doesn't preserve Unix execute bits) ───────────────

log "[0/4] Fixing file permissions..."
chmod +x "${CARTRIDGE_DIR}/cartridge" 2>/dev/null || true
chmod +x "${CARTRIDGE_DIR}/cartridge-boot" 2>/dev/null || true
chmod +x "${CARTRIDGE_DIR}/cartridge-boot.sh" 2>/dev/null || true
chmod +x "${CARTRIDGE_DIR}/autosetup.sh" 2>/dev/null || true
log "  Permissions fixed."

# ── Verify binaries exist ─────────────────────────────────────────────────────

if [[ ! -f "${CARTRIDGE_DIR}/cartridge-boot" ]]; then
    log "ERROR: cartridge-boot binary not found at ${CARTRIDGE_DIR}/cartridge-boot"
    sleep 3
    exit 1
fi

if [[ ! -f "${CARTRIDGE_DIR}/cartridge-boot.sh" ]]; then
    log "ERROR: cartridge-boot.sh not found at ${CARTRIDGE_DIR}/cartridge-boot.sh"
    sleep 3
    exit 1
fi

# ── Find EmulationStation services ────────────────────────────────────────────

log "[1/4] Scanning for EmulationStation services..."

ES_SERVICES_FOUND=()
for svc_name in \
    emulationstation.service \
    emulationstation-es.service \
    es-start.service \
    arkos-emulationstation.service \
    start_es.service; do
    if systemctl list-unit-files "$svc_name" &>/dev/null; then
        ES_SERVICES_FOUND+=("$svc_name")
    fi
done

# Also search service files for emulationstation references
while IFS= read -r svc_path; do
    if [[ -n "$svc_path" ]] && [[ -f "$svc_path" ]]; then
        if grep -qi "emulationstation" "$svc_path" 2>/dev/null; then
            svc_name="$(basename "$svc_path")"
            already_found=false
            for existing in "${ES_SERVICES_FOUND[@]+"${ES_SERVICES_FOUND[@]}"}"; do
                [[ "$existing" == "$svc_name" ]] && already_found=true && break
            done
            $already_found || ES_SERVICES_FOUND+=("$svc_name")
        fi
    fi
done < <(find /etc/systemd/system/ /usr/lib/systemd/system/ /lib/systemd/system/ -name '*.service' 2>/dev/null || true)

if [[ ${#ES_SERVICES_FOUND[@]} -eq 0 ]]; then
    log "  No ES services found (will try common names anyway)"
else
    for svc in "${ES_SERVICES_FOUND[@]}"; do
        log "  Found ES service: $svc"
    done
fi

# ── Install boot selector service ────────────────────────────────────────────

log "[2/4] Installing boot selector service..."

# Build Conflicts= line from all found ES services + common names
ALL_ES_SVCS=()
for s in "${ES_SERVICES_FOUND[@]+"${ES_SERVICES_FOUND[@]}"}"; do
    ALL_ES_SVCS+=("$s")
done
for s in emulationstation.service emulationstation-es.service start_es.service; do
    already=false
    for e in "${ALL_ES_SVCS[@]+"${ALL_ES_SVCS[@]}"}"; do
        [[ "$e" == "$s" ]] && already=true && break
    done
    $already || ALL_ES_SVCS+=("$s")
done
CONFLICTS_LINE=""
if [[ ${#ALL_ES_SVCS[@]} -gt 0 ]]; then
    CONFLICTS_LINE="Conflicts=${ALL_ES_SVCS[*]}"
fi

cat > /etc/systemd/system/cartridge-boot.service << SVCEOF
[Unit]
Description=CartridgeOS Boot Selector
After=local-fs.target systemd-logind.service
Wants=local-fs.target
${CONFLICTS_LINE}

[Service]
Type=simple
ExecStart=${CARTRIDGE_DIR}/cartridge-boot.sh
WorkingDirectory=${CARTRIDGE_DIR}
StandardInput=tty
StandardOutput=tty
StandardError=journal
TTYPath=/dev/tty1
TTYReset=yes
TTYVHangup=yes
TTYVTDisallocate=yes
Environment=HOME=/root
Environment=SDL_VIDEODRIVER=kmsdrm
Environment=SDL_AUDIODRIVER=alsa

[Install]
WantedBy=multi-user.target
SVCEOF

log "  Service file written to /etc/systemd/system/cartridge-boot.service"

systemctl daemon-reload
log "  systemd daemon reloaded"

systemctl enable cartridge-boot.service 2>&1 | tee -a "$LOG_FILE"
log "  Boot selector service enabled."

# Verify it's actually enabled
if systemctl is-enabled cartridge-boot.service &>/dev/null; then
    log "  Verified: cartridge-boot.service is enabled"
else
    log "  WARNING: cartridge-boot.service does not appear enabled!"
fi

# ── Disable and MASK ES services (mask prevents re-activation) ───────────────

log "[3/4] Disabling and masking EmulationStation auto-start..."

for svc in "${ES_SERVICES_FOUND[@]+"${ES_SERVICES_FOUND[@]}"}"; do
    log "  Stopping and masking $svc..."
    systemctl stop "$svc" 2>/dev/null || true
    systemctl disable "$svc" 2>/dev/null || true
    systemctl mask "$svc" 2>/dev/null || true
done

# Try common names regardless (mask is idempotent)
for svc in emulationstation.service emulationstation-es.service start_es.service; do
    systemctl disable "$svc" 2>/dev/null || true
    systemctl mask "$svc" 2>/dev/null || true
done

# Save which services we disabled for undo
echo "${ES_SERVICES_FOUND[*]+"${ES_SERVICES_FOUND[*]}"}" > "${CARTRIDGE_DIR}/.disabled_es_services"

# ── Diagnostic info ──────────────────────────────────────────────────────────

log "[4/4] Verifying setup..."
log ""
log "cartridge-boot.service status:"
systemctl is-enabled cartridge-boot.service 2>&1 | tee -a "$LOG_FILE" || true
log ""
log "ES service states:"
for svc in emulationstation.service emulationstation-es.service start_es.service es-start.service; do
    state=$(systemctl is-enabled "$svc" 2>/dev/null || echo "not found")
    log "  $svc: $state"
done
log ""
log "All enabled services matching cartridge/emulation:"
systemctl list-unit-files --state=enabled --type=service 2>/dev/null | grep -iE "(cartridge|emulation|es-start|start_es)" | tee -a "$LOG_FILE" || true

log ""
log "Setup complete!"

if ! $NO_REBOOT; then
    log "Rebooting in 3 seconds..."
    sleep 3
    reboot
fi
