#!/usr/bin/env bash
# dev-run.sh -- thin alias for ./sim.sh (kept for muscle memory).
#
#   ./dev-run.sh              # run the launcher with the FPS overlay
#   ./dev-run.sh --release    # release build
#   ./dev-run.sh --no-fps     # without overlay
#
# All flags are forwarded; see ./sim.sh --help and docs/simulator.md.
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/sim.sh" "$@"
