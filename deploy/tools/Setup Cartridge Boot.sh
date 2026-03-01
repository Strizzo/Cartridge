#!/bin/bash
# Setup Cartridge Boot Selector
#
# Run this from the EmulationStation Tools menu to enable the boot selector.
# On next reboot, you'll be able to choose between Cartridge and EmulationStation.
#
# This script:
#   1. Finds how ES is started on this system
#   2. Installs the boot selector as a systemd service
#   3. Disables ES auto-start (boot selector will launch it when chosen)
#   4. Reboots the device

# Detect active roms directory using ArkOS convention
if [ -f "/opt/system/Advanced/Switch to main SD for Roms.sh" ]; then
    ROMS_DIR="/roms2"
else
    ROMS_DIR="/roms"
fi
CARTRIDGE_DIR="${ROMS_DIR}/Cartridge"

echo ""
echo "==================================="
echo "  Cartridge Boot Selector Setup"
echo "==================================="
echo ""
echo "  Cartridge dir: ${CARTRIDGE_DIR}"
echo ""

# ── Verify Cartridge is installed ────────────────────────────────────────────

if [[ ! -x "${CARTRIDGE_DIR}/cartridge" ]]; then
    echo "ERROR: Cartridge not found at ${CARTRIDGE_DIR}"
    echo ""
    echo "Extract the Cartridge zip to your SD card's roms/ folder first."
    sleep 5
    exit 1
fi

if [[ ! -x "${CARTRIDGE_DIR}/cartridge-boot" ]]; then
    echo "ERROR: Boot selector binary not found."
    echo ""
    echo "Make sure you extracted the full Cartridge zip."
    sleep 5
    exit 1
fi

# ── Find and disable EmulationStation auto-start ────────────────────────────

echo "[1/3] Looking for EmulationStation startup mechanism..."

ES_SERVICES_FOUND=()

# Check common systemd service names
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

# Also search for any enabled service whose ExecStart contains "emulationstation"
while IFS= read -r svc_path; do
    if [[ -n "$svc_path" ]] && [[ -f "$svc_path" ]]; then
        if grep -qi "emulationstation" "$svc_path" 2>/dev/null; then
            svc_name="$(basename "$svc_path")"
            # Deduplicate
            already_found=false
            for existing in "${ES_SERVICES_FOUND[@]}"; do
                if [[ "$existing" == "$svc_name" ]]; then
                    already_found=true
                    break
                fi
            done
            if ! $already_found; then
                ES_SERVICES_FOUND+=("$svc_name")
            fi
        fi
    fi
done < <(find /etc/systemd/system/ /usr/lib/systemd/system/ /lib/systemd/system/ -name '*.service' 2>/dev/null)

if [[ ${#ES_SERVICES_FOUND[@]} -eq 0 ]]; then
    echo "  WARNING: Could not find EmulationStation systemd service."
    echo "  Will still install boot selector, but ES may auto-start alongside it."
    echo ""
    echo "  Checked: emulationstation.service, es-start.service, and others."
    echo "  Also scanned service files for 'emulationstation' references."
    echo ""
else
    for svc in "${ES_SERVICES_FOUND[@]}"; do
        echo "  Found: $svc"
    done
fi

echo ""

# ── Install boot selector service ────────────────────────────────────────────

echo "[2/3] Installing boot selector service..."

# Generate service file with the correct path for this device
cat > /etc/systemd/system/cartridge-boot.service << SVCEOF
[Unit]
Description=Cartridge Boot Selector
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

# Also try the common name even if not in the list (belt and suspenders)
systemctl disable emulationstation.service 2>/dev/null || true

# Save which services we disabled so Undo can re-enable them
echo "${ES_SERVICES_FOUND[*]}" > "${CARTRIDGE_DIR}/.disabled_es_services"

echo "  EmulationStation auto-start disabled."
echo "  (You can still choose it from the boot selector.)"

# ── Done ─────────────────────────────────────────────────────────────────────

echo ""
echo "==================================="
echo "  Setup complete!"
echo "==================================="
echo ""
echo "On next boot, you'll see the boot selector screen"
echo "where you can choose Cartridge or EmulationStation."
echo ""
echo "To undo this, run 'Undo Cartridge Boot' from Tools."
echo ""
echo "Rebooting in 5 seconds..."
sleep 5
reboot
