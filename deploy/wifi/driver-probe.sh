#!/bin/bash
# Temporary, device-specific Wi-Fi probe for ArkOS with a USB RTL8188EU.
# Changes to loaded modules and services last only until reboot. The log is
# written beside Cartridge on the ROMS partition for inspection on a Mac.

set -u
set -o pipefail

if [ -f '/opt/system/Advanced/Switch to main SD for Roms.sh' ]; then
    ROMS_DIR=/roms2
else
    ROMS_DIR=/roms
fi
CARTRIDGE_DIR="${ROMS_DIR}/Cartridge"
if [ ! -d "$CARTRIDGE_DIR" ] || [ ! -e /sys/class/net/wlan0/device/driver ]; then
    echo 'Expected Cartridge installation or wlan0 is missing; nothing changed.'
    sleep 5
    exit 1
fi
LOG="$CARTRIDGE_DIR/wifi-driver-probe-$(date -u +%Y%m%d-%H%M%S).log"
if [ -e "$LOG" ] || ! (set -C; : > "$LOG") 2>/dev/null; then
    echo 'Could not create a fresh Cartridge Wi-Fi log; nothing changed.'
    sleep 5
    exit 1
fi

services_stopped=0
restore_services() {
    if [ "$services_stopped" -eq 1 ]; then
        sudo -n systemctl start wpa_supplicant NetworkManager >/dev/null 2>&1 || true
    fi
}

wifi_state() {
    nmcli -t -f DEVICE,TYPE,STATE device status 2>/dev/null |
        awk -F: '$1 == "wlan0" && $2 == "wifi" { print $3; exit }'
}

show_state() {
    echo '--- NetworkManager devices ---'
    nmcli -t -f DEVICE,TYPE,STATE device status || true
    echo '--- Wi-Fi services ---'
    systemctl is-active NetworkManager wpa_supplicant || true
    echo '--- wlan0 driver ---'
    readlink -f /sys/class/net/wlan0/device/driver || true
    echo '--- loaded RTL8188EU modules ---'
    lsmod | grep -E '^(8188eu|r8188eu)[[:space:]]' || true
}

try_scan() {
    echo '--- scan request ---'
    timeout 12s nmcli device wifi rescan ifname wlan0 || true
    sleep 2
    echo '--- visible networks ---'
    timeout 12s nmcli -t -f SSID,SIGNAL,SECURITY device wifi list --rescan no ifname wlan0 || true
}

probe() {
    trap restore_services EXIT
    echo 'Cartridge Wi-Fi driver probe. All driver changes are temporary.'
    echo "Log: $LOG"
    if ! sudo -n true; then
        echo 'Passwordless device administration is unavailable; nothing changed.'
        return 1
    fi
    local driver
    driver=$(basename "$(readlink -f /sys/class/net/wlan0/device/driver)")
    if [ "$driver" != r8188eu ]; then
        echo "The current driver is $driver, not r8188eu; nothing changed."
        return 1
    fi

    show_state
    echo '--- USB Wi-Fi device ---'
    lsusb 2>&1 | grep -Ei 'realtek|0bda|8179|0179' || true
    echo '--- initial WPA service failure ---'
    sudo -n systemctl status wpa_supplicant --no-pager -l 2>&1 | tail -n 35 || true

    echo 'Trying a service restart first...'
    sudo -n systemctl reset-failed wpa_supplicant || true
    sudo -n systemctl restart wpa_supplicant || true
    sleep 3
    show_state
    if [ "$(wifi_state)" != unavailable ]; then
        try_scan
        echo 'Wi-Fi left the unavailable state after the service restart.'
        return 0
    fi

    echo 'Still unavailable. Capturing direct WPA startup output...'
    sudo -n systemctl stop NetworkManager wpa_supplicant || true
    services_stopped=1
    if command -v timeout >/dev/null 2>&1; then
        sudo -n timeout 6s /sbin/wpa_supplicant -u \
            -c/etc/wpa_supplicant/wpa_supplicant.conf -O/run/wpa_supplicant 2>&1 || true
    fi

    echo 'Testing the other already-installed RTL8188EU driver...'
    if ! sudo -n modprobe -r r8188eu; then
        echo 'Could not unload r8188eu; no driver switch was made.'
        return 1
    fi
    if [ ! -e /sys/class/net/wlan0/device/driver ] ||
       [ "$(basename "$(readlink -f /sys/class/net/wlan0/device/driver)")" != 8188eu ]; then
        sudo -n modprobe -r 8188eu || true
        sudo -n modprobe 8188eu || true
    fi
    sleep 2
    sudo -n systemctl reset-failed wpa_supplicant || true
    sudo -n systemctl start wpa_supplicant || true
    sudo -n systemctl start NetworkManager || true
    services_stopped=0
    sleep 4
    show_state
    try_scan
    echo '--- final WPA service status ---'
    sudo -n systemctl status wpa_supplicant --no-pager -l 2>&1 | tail -n 35 || true
    echo '--- recent Wi-Fi kernel messages ---'
    sudo -n dmesg 2>&1 | grep -Ei '8188|wlan|wifi|firmware|cfg80211|0bda' | tail -n 50 || true
    if [ "$(wifi_state)" = unavailable ]; then
        echo 'RESULT: wlan0 is still unavailable. The log identifies the next repair.'
    else
        echo 'RESULT: wlan0 is available. Try a scan in Cartridge.'
    fi
    echo 'Rebooting restores the original driver selection.'
}

probe 2>&1 | tee "$LOG"
status=${PIPESTATUS[0]}
sync
echo
echo "Probe complete. Log saved to $LOG"
echo 'Press a button to return to EmulationStation.'
read -r -t 30 _ || true
exit "$status"
