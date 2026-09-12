#!/usr/bin/env bash
# Grab a screenshot from the real panel and open it here.
#
#   deploy/wifi/device-shot.sh                 # -> screenshots/device-<stamp>.png
#   deploy/wifi/device-shot.sh --keep          # leave the file on the device too
#
# Sends SIGUSR1 to the running launcher, which captures the next rendered
# frame to <roms>/Cartridge/screenshots/. Read-back works on the accelerated
# KMSDRM renderer for the launcher's own frames; if a capture comes out empty,
# the fallback is CARTRIDGE_SOFTWARE=1 on the device.

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

KEEP=0
parse_common_args "$@"
set -- ${REST[@]+"${REST[@]}"}
while [[ $# -gt 0 ]]; do
    case "$1" in
        --keep)    KEEP=1; shift ;;
        -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
        *)         fail "Unknown argument: $1" ;;
    esac
done

require_host

ROMS="$(remote_roms_dir)"
DEST="${ROMS}/Cartridge"
REMOTE_DIR="${DEST}/screenshots"
LOCAL_DIR="${REPO_DIR}/screenshots"
mkdir -p "$LOCAL_DIR"

info "Signalling the launcher..."
dev_ssh "pkill -USR1 -f '/cartridge$'" || fail "No running launcher found on the device."

# Give it a couple of frames to render and write the PNG.
sleep 1.5

NEWEST="$(dev_ssh "ls -t '${REMOTE_DIR}'/*.png 2>/dev/null | head -1" || true)"
[[ -n "$NEWEST" ]] || fail "No screenshot appeared in ${REMOTE_DIR}.
      The launcher may be showing a cartridge rather than the launcher itself,
      or the frame read-back failed on this renderer."

STAMP="$(date +%Y%m%d-%H%M%S)"
OUT="${LOCAL_DIR}/device-${STAMP}.png"
scp "${SSH_OPTS[@]}" -q "${USER_NAME}@${HOST}:${NEWEST}" "$OUT"
ok "Saved ${OUT}"

if [[ "$KEEP" == "0" ]]; then
    dev_ssh "rm -f '${NEWEST}'" || true
fi

command -v open >/dev/null && open "$OUT"
