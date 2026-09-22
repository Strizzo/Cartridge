#!/bin/bash
# Rehearse the real boot supervisor with native SDL; no system services involved.
set -euo pipefail
TASK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SIM_SESSION="$CARTRIDGE_HOME/session-device"
mkdir -p "$SIM_SESSION"
ln -sfn "$TASK_ROOT/assets" "$SIM_SESSION/assets"
ln -sfn "$TASK_ROOT/deploy/game-library.py" "$SIM_SESSION/game-library.py"
ln -sfn "$TASK_ROOT/lua_cartridges" "$SIM_SESSION/lua_cartridges"
ln -sfn "$TASK_ROOT/registry.json" "$SIM_SESSION/registry.json"
# Cargo puts binaries in this explicitly chosen directory for isolated worktrees.
SIM_TARGET="${CARGO_TARGET_DIR:-$TASK_ROOT/target}"
PROFILE=debug
BUILD=()
[[ "${CARTRIDGE_SIM_RELEASE:-0}" == 1 ]] && PROFILE=release && BUILD+=(--release)
cargo build -q ${BUILD[@]+"${BUILD[@]}"} --bin cartridge --bin sim-check
if [[ "${1:-}" == --check ]]; then
    # An actual rendered launcher navigates to ES automatically; the parent
    # supervisor must observe exit 20 and invoke our harmless ES stand-in.
    printf '#!/bin/bash\nexec "%s" --handoff\n' "$SIM_TARGET/$PROFILE/sim-check" > "$SIM_SESSION/cartridge"
else
    printf '#!/bin/bash\nexec "%s"\n' "$SIM_TARGET/$PROFILE/cartridge" > "$SIM_SESSION/cartridge"
fi
chmod +x "$SIM_SESSION/cartridge"
printf '#!/bin/bash\necho "Simulator: EmulationStation handoff received."\n' > "$SIM_SESSION/emulationstation.sh"
exec python3 "$TASK_ROOT/deploy/cartridge-session.py" --desktop \
    --cartridge-dir "$SIM_SESSION" --state-dir "$SIM_SESSION/state" \
    --es-script "$SIM_SESSION/emulationstation.sh"
