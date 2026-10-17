#!/bin/bash
# Remove only the WPA service drop-in installed by wpa-repair-trial.sh.

set -u
OVERRIDE=/etc/systemd/system/wpa_supplicant.service.d/90-cartridge-wifi.conf
if ! sudo -n true; then
    echo 'Passwordless administration is unavailable; nothing changed.'
    exit 1
fi
if ! sudo -n test -e "$OVERRIDE"; then
    echo 'No Cartridge WPA service repair is installed.'
    exit 0
fi
actual=$(sudo -n cat "$OVERRIDE") || exit 1
restart_only=$(printf '[Service]\nRestart=on-failure\nRestartSec=3s')
stock_command=$(printf '[Service]\nExecStart=\nExecStart=/sbin/wpa_supplicant -u -s -O /run/wpa_supplicant\nRestart=on-failure\nRestartSec=3s')
if [ "$actual" != "$restart_only" ] && [ "$actual" != "$stock_command" ]; then
    echo 'The service drop-in has changed; refusing to delete it.'
    exit 1
fi
sudo -n rm "$OVERRIDE" || exit 1
sudo -n systemctl daemon-reload || exit 1
sudo -n systemctl reset-failed wpa_supplicant || true
sudo -n systemctl restart wpa_supplicant NetworkManager || true
sync
echo 'Cartridge WPA repair removed. Original systemd unit is active.'
echo 'Press Enter to return to EmulationStation.'
read -r -t 30 _ || true
