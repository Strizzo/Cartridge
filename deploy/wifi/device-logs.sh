#!/usr/bin/env bash
# Tail the launcher's logs from the device.
#
#   deploy/wifi/device-logs.sh            # follow the journal
#   deploy/wifi/device-logs.sh --dump     # print the other log files and exit
#
# The systemd unit sets RUST_LOG=...=info, so info-level breadcrumbs show up
# here. Cartridge crashes land in crash.log next to the binary.

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

DUMP=0
parse_common_args "$@"
set -- ${REST[@]+"${REST[@]}"}
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dump)    DUMP=1; shift ;;
        -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
        *)         fail "Unknown argument: $1" ;;
    esac
done

require_host

ROMS="$(remote_roms_dir)"
DEST="${ROMS}/Cartridge"

if [[ "$DUMP" == "1" ]]; then
    for f in "/home/ark/.cartridges/session/session.log" "/home/ark/.cartridges/session/last-session.json" "${DEST}/crash.log" "${DEST}/setup.log" "/tmp/cartridge_wifi.log"; do
        echo
        echo "===== ${f} ====="
        dev_ssh "tail -n 40 '${f}' 2>/dev/null || echo '(not present)'"
    done
    echo
    echo "===== journal (last 40) ====="
    dev_ssh "journalctl -u emulationstation -u cartridge-boot -n 40 --no-pager 2>/dev/null || echo '(no journal for cartridge-boot)'"
    exit 0
fi

info "Following journalctl -u emulationstation -u cartridge-boot on ${HOST} (Ctrl-C to stop)"
dev_ssh "journalctl -u emulationstation -u cartridge-boot -f -n 30 --no-pager"
