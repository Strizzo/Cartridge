#!/usr/bin/env bash
# sim.sh -- run CartridgeOS on the desktop as a simulated R36S Plus.
#
# Usage:
#   ./sim.sh                          # launcher (FPS overlay on)
#   ./sim.sh app lua_cartridges/hacker_news   # one cartridge, hot reload on
#   ./sim.sh check                    # automated navigation/app screenshots
#   ./sim.sh session                  # real startup supervisor + native UI
#   ./sim.sh boot                     # the 5 s boot selector
#   ./sim.sh demo                     # drawing-primitives demo
#
# Flags (anywhere on the command line; everything after "--" goes to the binary):
#   --scale N        window = 720*N points (default 1)
#   --true-size      scale so the window is 71.8 mm wide on the main display
#   --fullscreen     desktop fullscreen, 720x720 letterboxed
#   --release        release build (closer to device perf; still not device numbers)
#   --profile <json> simulated device profile (default sim/profiles/r36s-plus.json)
#   --battery N      override battery percent
#   --wifi off|ssid  override WiFi state
#   --home <dir>     CARTRIDGE_HOME (default .sim/home, gitignored)
#   --no-fps         disable the FPS overlay (CARTRIDGE_FPS=1 is on by default)
#   --software       software renderer (exact F12 screenshots)
#   --hidden         hidden window (headless smoke tests)
#
# Environment it sets: CARTRIDGE_SIM=1, CARTRIDGE_ASSETS, CARTRIDGE_HOME,
# CARTRIDGE_SIM_PROFILE, RUST_LOG (unless already set), plus CARTRIDGE_SCALE /
# CARTRIDGE_FULLSCREEN / CARTRIDGE_SOFTWARE / CARTRIDGE_HIDDEN / CARTRIDGE_FPS /
# CARTRIDGE_HOT_RELOAD / CARTRIDGE_SIM_BATTERY / CARTRIDGE_SIM_WIFI as requested.
#
# Keys: arrows=D-pad  Z=A  X=B  C=X  V=Y  A=L1  S=R1  Q=L2  W=R2
#       Enter=Start  Space=Select  Esc=quit  F12=screenshot (screenshots/)
# See docs/simulator.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

# Physical width of the R36S Plus panel: 720 px @ ~254.6 ppi (4.0" square).
PANEL_MM=71.8

MODE="launcher"
APP_DIR=""
SCALE=""
TRUE_SIZE=0
FULLSCREEN=0
RELEASE=0
PROFILE=""
BATTERY=""
WIFI=""
SIM_HOME=""
SHOW_FPS=1
SOFTWARE=0
HIDDEN=0
PASS=()

usage() { sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        app)          MODE="app"; APP_DIR="${2:-}"; [[ -n "$APP_DIR" ]] || { echo "app: missing cartridge dir" >&2; exit 2; }; shift 2 ;;
        check)        MODE="check"; HIDDEN=1; SOFTWARE=1; SHOW_FPS=0; shift ;;
        session)      MODE="session"; shift ;;
        boot)         MODE="boot"; shift ;;
        demo)         MODE="demo"; shift ;;
        --scale)      SCALE="$2"; shift 2 ;;
        --scale=*)    SCALE="${1#--scale=}"; shift ;;
        --true-size)  TRUE_SIZE=1; shift ;;
        --fullscreen) FULLSCREEN=1; shift ;;
        --release)    RELEASE=1; shift ;;
        --profile)    PROFILE="$2"; shift 2 ;;
        --profile=*)  PROFILE="${1#--profile=}"; shift ;;
        --battery)    BATTERY="$2"; shift 2 ;;
        --battery=*)  BATTERY="${1#--battery=}"; shift ;;
        --wifi)       WIFI="$2"; shift 2 ;;
        --wifi=*)     WIFI="${1#--wifi=}"; shift ;;
        --home)       SIM_HOME="$2"; shift 2 ;;
        --home=*)     SIM_HOME="${1#--home=}"; shift ;;
        --no-fps)     SHOW_FPS=0; shift ;;
        --software)   SOFTWARE=1; shift ;;
        --hidden)     HIDDEN=1; shift ;;
        -h|--help)    usage; exit 0 ;;
        --)           shift; PASS+=("$@"); break ;;
        *)            PASS+=("$1"); shift ;;
    esac
done

# ── --true-size: derive the scale from the main display's physical size ──────
#
# scale = PANEL_MM / (720 * mm_per_point). mm_per_point needs the display's
# physical width (mm) and its "looks like" width in points.
true_size_scale() {
    local width_mm="" points_w="" fallback="0.6"

    # 1. EDID physical size (Intel Macs / external displays): "DisplayPhysicalSize"
    #    is not exposed on Apple Silicon built-in panels.
    if command -v ioreg >/dev/null 2>&1; then
        width_mm=$(ioreg -lw0 2>/dev/null | grep -m1 -oE '"DisplayPhysicalSize"[^}]*' | grep -oE '"width"=[0-9]+' | grep -oE '[0-9]+' || true)
    fi
    # 2. Explicit override.
    if [[ -z "$width_mm" && -n "${CARTRIDGE_DISPLAY_WIDTH_MM:-}" ]]; then
        width_mm="$CARTRIDGE_DISPLAY_WIDTH_MM"
    fi
    # 3. Known Apple built-in panels (16:10; width = diagonal * 0.848).
    if [[ -z "$width_mm" ]]; then
        local model
        model=$(sysctl -n hw.model 2>/dev/null || true)
        case "$model" in
            MacBookAir10,1|MacBookPro17,1|MacBookPro18,3|MacBookPro18,4) width_mm=286 ;; # 13.3"
            MacBookPro18,1|MacBookPro18,2|Mac14,6|Mac14,10|Mac15,7|Mac15,9|Mac15,11|Mac16,5|Mac16,7) width_mm=349 ;; # 16.2"
            MacBookPro18,3-|Mac14,5|Mac14,9|Mac15,3|Mac15,6|Mac15,8|Mac15,10|Mac16,1|Mac16,6|Mac16,8) width_mm=306 ;; # 14.2"
            Mac14,2|Mac15,12|Mac16,12) width_mm=293 ;;  # 13.6" Air
            Mac14,15|Mac15,13|Mac16,13) width_mm=330 ;; # 15.3" Air
            Mac14,7) width_mm=286 ;;                     # 13.3" Pro M2
            iMac21,1|iMac21,2|Mac15,4|Mac15,5|Mac16,2|Mac16,3) width_mm=506 ;; # 23.5" iMac
        esac
    fi

    # Width in points ("looks like" resolution) of the main display. The JSON
    # output's _spdisplays_resolution is the scaled/points resolution (e.g.
    # "1440 x 900 @ 60.00Hz"); the text output only shows native pixels.
    points_w=$(system_profiler SPDisplaysDataType -json 2>/dev/null \
        | grep -m1 -oE '"_spdisplays_resolution" *: *"[0-9]+' | grep -oE '[0-9]+$' || true)
    if [[ -z "$points_w" ]]; then
        points_w=$(system_profiler SPDisplaysDataType 2>/dev/null | grep -m1 -E 'UI Looks like:' | grep -oE '[0-9]+' | head -1 || true)
    fi

    if [[ -n "$width_mm" && -n "$points_w" && "$points_w" -gt 0 ]]; then
        awk -v mm="$width_mm" -v pw="$points_w" -v panel="$PANEL_MM" 'BEGIN { printf "%.3f", panel / (720 * (mm / pw)) }'
    else
        echo "sim: could not determine display size; --true-size falling back to $fallback (set CARTRIDGE_DISPLAY_WIDTH_MM)" >&2
        echo "$fallback"
    fi
}

if [[ "$TRUE_SIZE" == "1" && -z "$SCALE" ]]; then
    SCALE=$(true_size_scale)
    echo "sim: true-size scale = $SCALE"
fi

# ── Environment ──────────────────────────────────────────────────────────────
SIM_HOME="${SIM_HOME:-$ROOT/.sim/home}"
mkdir -p "$SIM_HOME"
SIM_HOME="$(cd "$SIM_HOME" && pwd)"

if [[ -z "$PROFILE" && -f "$ROOT/sim/profiles/r36s-plus.json" ]]; then
    PROFILE="$ROOT/sim/profiles/r36s-plus.json"
fi

# Small generated library lets game discovery and launch/return work offline.
if [[ -z "${CARTRIDGE_ES_SYSTEMS:-}" ]]; then
    python3 "$ROOT/sim/setup-game-fixture.py" "$SIM_HOME/device"
    export CARTRIDGE_ES_SYSTEMS="$SIM_HOME/device/es_systems.cfg"
    export CARTRIDGE_ES_SETTINGS="$SIM_HOME/device/es_settings.cfg"
    export CARTRIDGE_ES_HOME="$SIM_HOME/device"
    export CARTRIDGE_ROMS="$SIM_HOME/device/roms"
fi
export CARTRIDGE_SIM=1
export CARTRIDGE_ASSETS="$ROOT/assets"
export CARTRIDGE_HOME="$SIM_HOME"
export RUST_LOG="${RUST_LOG:-cartridge=info,cartridge_launcher=info,cartridge_core=info,cartridge_lua=info}"
[[ -n "$PROFILE" ]]        && export CARTRIDGE_SIM_PROFILE="$PROFILE"
[[ -n "$BATTERY" ]]        && export CARTRIDGE_SIM_BATTERY="$BATTERY"
[[ -n "$WIFI" ]]           && export CARTRIDGE_SIM_WIFI="$WIFI"
[[ -n "$SCALE" ]]          && export CARTRIDGE_SCALE="$SCALE"
[[ "$FULLSCREEN" == "1" ]] && export CARTRIDGE_FULLSCREEN=1
[[ "$SOFTWARE" == "1" ]]   && export CARTRIDGE_SOFTWARE=1
[[ "$HIDDEN" == "1" ]]     && export CARTRIDGE_HIDDEN=1
[[ "$SHOW_FPS" == "1" ]]   && export CARTRIDGE_FPS=1
[[ "$MODE" == "app" ]]     && export CARTRIDGE_HOT_RELOAD="${CARTRIDGE_HOT_RELOAD:-1}"

CARGO_FLAGS=()
[[ "$RELEASE" == "1" ]] && CARGO_FLAGS+=("--release")

echo "sim: mode=$MODE scale=${SCALE:-1} fullscreen=$FULLSCREEN software=$SOFTWARE fps=$SHOW_FPS"
echo "sim: home=$CARTRIDGE_HOME profile=${CARTRIDGE_SIM_PROFILE:-<defaults>} battery=${BATTERY:-profile} wifi=${WIFI:-profile}"
echo

case "$MODE" in
    check)
        export CARTRIDGE_READY_FILE="$SIM_HOME/check-ready"
        rm -f "$CARTRIDGE_READY_FILE"
        exec cargo run -q ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"} --bin sim-check -- ${PASS[@]+"${PASS[@]}"}
        ;;
    session)
        export CARTRIDGE_SIM_RELEASE="$RELEASE"
        exec bash "$ROOT/sim/session.sh" ${PASS[@]+"${PASS[@]}"}
        ;;
    launcher) exec cargo run -q ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"} --bin cartridge -- ${PASS[@]+"${PASS[@]}"} ;;
    app)      exec cargo run -q ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"} --bin cartridge -- run --path "$APP_DIR" ${PASS[@]+"${PASS[@]}"} ;;
    demo)     exec cargo run -q ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"} --bin cartridge -- demo ${PASS[@]+"${PASS[@]}"} ;;
    boot)     exec cargo run -q ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"} -p cartridge-boot --bin cartridge-boot -- ${PASS[@]+"${PASS[@]}"} ;;
esac
