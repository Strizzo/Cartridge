#!/bin/bash
# Device-only RTL8188EU/WPA repair trial. Keep a root change only after an
# actual wlan0 access-point scan. Save the result on EASYROMS for Mac inspection.

set -u
set -o pipefail

if [ -f '/opt/system/Advanced/Switch to main SD for Roms.sh' ]; then
    ROMS_DIR=/roms2
else
    ROMS_DIR=/roms
fi
CARTRIDGE_DIR="${ROMS_DIR}/Cartridge"
if [ ! -d "$CARTRIDGE_DIR" ]; then
    echo 'Cartridge installation not found; nothing changed.'
    exit 1
fi
LOG="$CARTRIDGE_DIR/wifi-service-trial-$(date -u +%Y%m%d-%H%M%S).log"
if [ -e "$LOG" ] || ! (set -C; : > "$LOG") 2>/dev/null; then
    echo 'Cannot create a fresh Wi-Fi trial log; nothing changed.'
    exit 1
fi

RUNTIME_OVERRIDE=/run/systemd/system/wpa_supplicant.service.d/90-cartridge-wifi-trial.conf
PERSISTENT_OVERRIDE=/etc/systemd/system/wpa_supplicant.service.d/90-cartridge-wifi.conf
runtime_created=0
persistent_created=0
temp_file=''

cleanup() {
    if [ "$runtime_created" -eq 1 ]; then
        sudo -n rm -f "$RUNTIME_OVERRIDE" || true
        sudo -n systemctl daemon-reload || true
        sudo -n systemctl restart wpa_supplicant NetworkManager || true
    fi
    if [ -n "$temp_file" ]; then
        rm -f "$temp_file"
    fi
}

show_state() {
    echo '--- interfaces ---'
    nmcli -t -f DEVICE,TYPE,STATE device status || true
    echo '--- services ---'
    systemctl is-active NetworkManager wpa_supplicant || true
    echo '--- wlan0 driver ---'
    readlink -f /sys/class/net/wlan0/device/driver || true
    echo '--- effective WPA service ---'
    systemctl show -p ExecStart -p Restart wpa_supplicant || true
}

scan_wlan0() {
    local state list
    echo '--- wlan0 access-point scan ---'
    if ! timeout 12s nmcli device wifi rescan ifname wlan0 2>&1; then
        echo 'Scan request failed; this mode is not verified.'
        return 1
    fi
    sleep 3
    state=$(nmcli -t -f DEVICE,TYPE,STATE device status 2>/dev/null |
        awk -F: '$1 == "wlan0" && $2 == "wifi" { print $3; exit }')
    echo "wlan0 state: ${state:-missing}"
    if ! list=$(timeout 12s nmcli -t -f SSID,SIGNAL,SECURITY device wifi list \
        --rescan no ifname wlan0 2>&1); then
        printf '%s\n' "$list"
        return 1
    fi
    if [ -n "$list" ]; then
        printf '%s\n' "$list"
    else
        echo '(no access points)'
    fi
    case "$state" in
        connected|connecting|disconnected) [ -n "$list" ] ;;
        *) return 1 ;;
    esac
}

restart_services() {
    sudo -n systemctl reset-failed wpa_supplicant || true
    sudo -n systemctl restart wpa_supplicant || true
    sleep 3
    sudo -n systemctl restart NetworkManager || true
    sleep 5
    show_state
}

write_override() {
    local destination=$1 mode=$2
    if sudo -n test -e "$destination" || sudo -n test -L "$destination"; then
        echo "Refusing to overwrite existing $destination"
        return 1
    fi
    temp_file=$(mktemp /tmp/cartridge-wifi-unit.XXXXXX) || return 1
    {
        printf '[Service]\n'
        if [ "$mode" = stock ]; then
            printf 'ExecStart=\nExecStart=/sbin/wpa_supplicant -u -s -O /run/wpa_supplicant\n'
        fi
        printf 'Restart=on-failure\nRestartSec=3s\n'
    } > "$temp_file" || return 1
    sudo -n install -d -m 0755 "$(dirname "$destination")" || return 1
    if ! sudo -n install -m 0644 "$temp_file" "$destination"; then
        sudo -n rm -f "$destination" || true
        return 1
    fi
    if ! sudo -n cmp -s "$temp_file" "$destination"; then
        sudo -n rm -f "$destination" || true
        return 1
    fi
    if [ "$destination" = "$RUNTIME_OVERRIDE" ]; then
        runtime_created=1
    else
        persistent_created=1
    fi
    rm -f "$temp_file"
    temp_file=''
    sudo -n systemctl daemon-reload
}

rollback_persistent() {
    if [ "$persistent_created" -eq 1 ]; then
        sudo -n rm -f "$PERSISTENT_OVERRIDE" || return 1
        persistent_created=0
        sudo -n systemctl daemon-reload || return 1
    fi
}

probe() {
    trap cleanup EXIT
    echo "Cartridge Wi-Fi service trial. Log: $LOG"
    if ! sudo -n true; then
        echo 'Passwordless administration is unavailable; nothing changed.'
        return 1
    fi
    if [ ! -e /sys/class/net/wlan0/device/driver ] ||
       [ "$(basename "$(readlink -f /sys/class/net/wlan0/device/driver)")" != rtl8188eu ]; then
        echo 'Expected wlan0 on the alternate RTL8188EU driver; nothing changed.'
        return 1
    fi
    if sudo -n test -e "$RUNTIME_OVERRIDE" || sudo -n test -L "$RUNTIME_OVERRIDE" ||
       sudo -n test -e "$PERSISTENT_OVERRIDE" || sudo -n test -L "$PERSISTENT_OVERRIDE"; then
        echo 'A Cartridge Wi-Fi service override already exists; nothing changed.'
        return 1
    fi
    show_state
    echo '--- initial WPA failure ---'
    sudo -n systemctl status wpa_supplicant --no-pager -l 2>&1 | tail -n 30 || true

    echo 'Trying a normal service restart with the current command...'
    restart_services
    local mode=restart
    if ! scan_wlan0; then
        echo 'Still no access points. Trying the standard WPA D-Bus command in RAM...'
        if ! write_override "$RUNTIME_OVERRIDE" stock; then
            echo 'Could not set temporary service command; no persistent change.'
            return 1
        fi
        restart_services
        if ! scan_wlan0; then
            echo 'RESULT: No access points with either service command. No persistent repair installed.'
            sudo -n systemctl status wpa_supplicant --no-pager -l 2>&1 | tail -n 30 || true
            sudo -n dmesg 2>&1 | grep -Ei 'wpa|segfault|8188|usb 1-1' | tail -n 50 || true
            return 1
        fi
        mode=stock
    fi

    echo "Access points found with $mode service mode. Installing a reversible service drop-in..."
    if ! write_override "$PERSISTENT_OVERRIDE" "$mode"; then
        if ! rollback_persistent; then
            echo 'RESULT: Could not verify rollback of the persistent drop-in; inspect the log before reboot.'
            return 1
        fi
        echo 'Could not install the persistent drop-in; no persistent repair installed.'
        return 1
    fi
    if [ "$runtime_created" -eq 1 ]; then
        if ! sudo -n rm -f "$RUNTIME_OVERRIDE"; then
            echo 'Could not remove the temporary override. Keeping the verified service, but persistence is unverified.'
            return 1
        fi
        runtime_created=0
        if ! sudo -n systemctl daemon-reload; then
            echo 'Could not reload the persistent service. Keeping the verified service, but persistence is unverified.'
            return 1
        fi
    fi
    restart_services
    if ! scan_wlan0; then
        echo 'The persistent drop-in did not pass a repeat scan; rolling it back.'
        if ! rollback_persistent; then
            echo 'RESULT: Rollback failed; leave the card powered off and inspect the log.'
            return 1
        fi
        restart_services
        echo 'RESULT: Repair rolled back. No persistent service change remains.'
        return 1
    fi
    echo 'RESULT: Access points visible with the persistent service drop-in.'
    echo 'The original systemd unit is untouched. Remove the Cartridge drop-in to undo.'
    sync
}

probe 2>&1 | tee "$LOG"
status=${PIPESTATUS[0]}
sync
echo
echo "Trial complete. Log saved to $LOG"
echo 'Press Enter to return to EmulationStation.'
read -r -t 30 _ || true
exit "$status"
