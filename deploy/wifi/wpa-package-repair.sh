#!/bin/bash
# Restore only the recorded damaged WPA binaries; never changes Wi-Fi profiles.
set -uo pipefail
ROMS=/roms
[[ -f '/opt/system/Advanced/Switch to main SD for Roms.sh' ]] && ROMS=/roms2
APP="${CARTRIDGE_DIR:-$ROMS/Cartridge}"
REPAIR="$APP/wifi-package-repair"
LOG="$APP/wifi-package-repair.log"
SUMMARY='WPA repair did not complete. Read the repair log before testing again.'
if [[ ! -f "$REPAIR/restore-wpa-package.py" ]]; then
    SUMMARY='The verified WPA repair payload is missing. No changes made.'
elif sudo -n /usr/bin/python3 "$REPAIR/restore-wpa-package.py" --payload "$REPAIR/payload" --restart-network 2>&1 | tee "$LOG"; then
    sleep 3
    {
        printf '\n--- WPA service after package restore ---\n'
        systemctl --no-pager --full status wpa_supplicant.service || true
        printf '\n--- Wireless devices ---\n'
        nmcli -t -f DEVICE,TYPE,STATE dev status || true
        printf '\n--- wlan0 scan ---\n'
        sudo -n nmcli -w 8 dev wifi rescan ifname wlan0 || true
        sleep 2
        nmcli -t -f SSID,SIGNAL dev wifi list ifname wlan0 || true
    } 2>&1 | tee -a "$LOG"
    SUMMARY='Original WPA package files restored and checksummed. Your saved networks were kept. Open Wi-Fi in Cartridge and try connecting. Hardware connection still needs testing.'
fi
printf '\n%s\nLog: %s\n' "$SUMMARY" "$LOG"
if command -v dialog >/dev/null 2>&1; then
    dialog --title 'Cartridge Wi-Fi Repair' --msgbox "$SUMMARY\n\nLog: $LOG\n\nPress OK to return." 16 65
else
    read -r -p 'Press Enter to return to EmulationStation. ' _ || true
fi
