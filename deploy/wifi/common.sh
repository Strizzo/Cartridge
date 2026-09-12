#!/usr/bin/env bash
# Shared helpers for the wireless deploy scripts.
#
# Host selection, in order: --host flag, $CARTRIDGE_HOST, the value saved in
# deploy/wifi/.device (written by --save). Same for the login user, which
# defaults to ArkOS's `ark`.

set -euo pipefail

WIFI_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${WIFI_DIR}/../.." && pwd)"
DEVICE_FILE="${WIFI_DIR}/.device"

HOST="${CARTRIDGE_HOST:-}"
USER_NAME="${CARTRIDGE_USER:-ark}"
SAVE_DEVICE=0

if [[ -f "$DEVICE_FILE" ]]; then
    # shellcheck disable=SC1090
    source "$DEVICE_FILE"
    HOST="${CARTRIDGE_HOST:-$HOST}"
    USER_NAME="${CARTRIDGE_USER:-$USER_NAME}"
fi

RED=$'\033[0;31m'; GREEN=$'\033[0;32m'; YELLOW=$'\033[1;33m'; BLUE=$'\033[0;34m'; NC=$'\033[0m'
info() { echo "${BLUE}[*]${NC} $1"; }
ok()   { echo "${GREEN}[+]${NC} $1"; }
warn() { echo "${YELLOW}[!]${NC} $1"; }
fail() { echo "${RED}[x]${NC} $1" >&2; exit 1; }

# Parse the host/user flags every script shares. Unrecognized arguments are
# collected into REST so each script can handle its own flags.
REST=()
parse_common_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --host)   HOST="$2"; shift 2 ;;
            --host=*) HOST="${1#--host=}"; shift ;;
            --user)   USER_NAME="$2"; shift 2 ;;
            --user=*) USER_NAME="${1#--user=}"; shift ;;
            --save)   SAVE_DEVICE=1; shift ;;
            *)        REST+=("$1"); shift ;;
        esac
    done
}

require_host() {
    if [[ -z "$HOST" ]]; then
        fail "No device host. Pass --host <ip|name>, set CARTRIDGE_HOST, or run once with --save.
      Find the address on the device: Settings > WiFi (it shows the current IP).
      First time on the device: ArkOS Options > Enable Remote Services, then
      ssh-copy-id ${USER_NAME}@<ip> from this machine."
    fi
    if [[ "$SAVE_DEVICE" == "1" ]]; then
        printf 'CARTRIDGE_HOST=%s\nCARTRIDGE_USER=%s\n' "$HOST" "$USER_NAME" > "$DEVICE_FILE"
        ok "Saved ${USER_NAME}@${HOST} to deploy/wifi/.device"
    fi
}

SSH_OPTS=(-o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new -o BatchMode=yes)

dev_ssh() { ssh "${SSH_OPTS[@]}" "${USER_NAME}@${HOST}" "$@"; }

check_reachable() {
    dev_ssh true 2>/dev/null || fail "Cannot reach ${USER_NAME}@${HOST} over SSH.
      Check the device is awake and on WiFi, that Remote Services is enabled
      (ArkOS Options menu), and that your key is installed:
        ssh-copy-id ${USER_NAME}@${HOST}"
    ok "Connected to ${USER_NAME}@${HOST}"
}

# ArkOS keeps roms on /roms2 when the "main SD for Roms" switch is active.
# Same probe the Tools launcher uses, run on the device.
remote_roms_dir() {
    dev_ssh 'if [ -f "/opt/system/Advanced/Switch to main SD for Roms.sh" ]; then echo /roms2; else echo /roms; fi'
}
