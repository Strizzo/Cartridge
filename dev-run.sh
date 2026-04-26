#!/usr/bin/env bash
# Dev runner: iterate on Cartridge locally with the FPS overlay enabled.
#
# Usage:
#   ./dev-run.sh              # run with FPS overlay
#   ./dev-run.sh --release    # release build (closer to device perf characteristics)
#   ./dev-run.sh --no-fps     # without overlay
#
# The FPS overlay shows: fps | last frame ms | avg ms | max ms | text cache stats.
# Logs to stderr every 5s with the same info plus cache hit rate.
set -euo pipefail

cd "$(dirname "$0")"

ARGS=()
RUN_FLAGS=()
SHOW_FPS=1
for arg in "$@"; do
    case "$arg" in
        --release) RUN_FLAGS+=("--release") ;;
        --no-fps) SHOW_FPS=0 ;;
        *) ARGS+=("$arg") ;;
    esac
done

export RUST_LOG="${RUST_LOG:-cartridge=info,cartridge_launcher=info,cartridge_core=info,cartridge_lua=info}"
if [[ "$SHOW_FPS" == "1" ]]; then
    export CARTRIDGE_FPS=1
fi

echo "FPS overlay: ${SHOW_FPS:-on}    RUST_LOG=$RUST_LOG"
echo
exec cargo run "${RUN_FLAGS[@]}" -- "${ARGS[@]}"
