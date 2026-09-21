#!/usr/bin/env bash
# Copy the current build onto a mounted SD card -- files only.
#
#   deploy/sdcard-sync.sh                      # auto-detect the card
#   deploy/sdcard-sync.sh --device /Volumes/EASYROMS
#   deploy/sdcard-sync.sh --apps-only          # cartridges + assets, no binaries
#
# Unlike install_to_device.sh this does NOT touch the boot service: no sudo, no
# unmounting, no debugfs. Use it when Cartridge is already installed on the
# device and you just want the newest build on the card.
#
# Binaries come from target/aarch64-unknown-linux-gnu/release/ -- build them
# first (see docs/wireless-deploy.md for the container setup).

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_DIR="${REPO_DIR}/target/aarch64-unknown-linux-gnu/release"
DEVICE=""
APPS_ONLY=0

RED=$'\033[0;31m'; GREEN=$'\033[0;32m'; YELLOW=$'\033[1;33m'; BLUE=$'\033[0;34m'; NC=$'\033[0m'
info() { echo "${BLUE}[*]${NC} $1"; }
ok()   { echo "${GREEN}[+]${NC} $1"; }
warn() { echo "${YELLOW}[!]${NC} $1"; }
fail() { echo "${RED}[x]${NC} $1" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --device)    DEVICE="$2"; shift 2 ;;
        --device=*)  DEVICE="${1#--device=}"; shift ;;
        --apps-only) APPS_ONLY=1; shift ;;
        -h|--help)   sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *)           fail "Unknown argument: $1" ;;
    esac
done

# -- Find the card ------------------------------------------------------------

if [[ -z "$DEVICE" ]]; then
    for vol in /Volumes/*/; do
        if [[ -d "${vol}roms" || -d "${vol}tools" ]]; then
            DEVICE="${vol%/}"
            break
        fi
    done
    [[ -n "$DEVICE" ]] || fail "No handheld SD card found under /Volumes. Pass --device <path>."
fi
[[ -d "$DEVICE" ]] || fail "Not a directory: $DEVICE"

# The roms partition either has a roms/ subdirectory or is the roms dir itself.
if [[ -d "$DEVICE/roms" ]]; then
    ROMS="$DEVICE/roms"
else
    ROMS="$DEVICE"
fi
DEST="${ROMS}/Cartridge"
TOOLS="${ROMS}/tools"

info "Card:  $DEVICE"
info "Install: $DEST"

# -- Copy ---------------------------------------------------------------------

mkdir -p "$DEST/assets/fonts" "$DEST/assets/overlays" "$TOOLS"

if [[ "$APPS_ONLY" == "0" ]]; then
    [[ -f "$TARGET_DIR/cartridge" ]] || fail "No device binary at $TARGET_DIR/cartridge.
      Build it first, or pass --apps-only to sync just cartridges and assets.
      NOTE: new cartridges can use APIs an old binary lacks, so --apps-only
      against a stale binary may break apps."
    info "Binaries..."
    cp "$TARGET_DIR/cartridge" "$DEST/cartridge"
    cp "$TARGET_DIR/cartridge-boot" "$DEST/cartridge-boot"
    chmod +x "$DEST/cartridge" "$DEST/cartridge-boot" 2>/dev/null || true

    info "Boot scripts..."
    cp "$REPO_DIR/deploy/cartridge-boot.sh" "$DEST/"
    cp "$REPO_DIR/deploy/cartridge-boot.service" "$DEST/"
    cp "$REPO_DIR/deploy/autosetup.sh" "$DEST/"
    chmod +x "$DEST/cartridge-boot.sh" "$DEST/autosetup.sh" 2>/dev/null || true

    info "Tools menu scripts..."
    cp "$REPO_DIR/deploy/tools/"*.sh "$TOOLS/"
    chmod +x "$TOOLS/"*.sh 2>/dev/null || true
fi

info "Registry and assets..."
[[ -f "$REPO_DIR/registry.json" ]] && cp "$REPO_DIR/registry.json" "$DEST/"
cp "$REPO_DIR/assets/fonts/"* "$DEST/assets/fonts/" 2>/dev/null || true
cp "$REPO_DIR/assets/overlays/"* "$DEST/assets/overlays/" 2>/dev/null || true
[[ -f "$REPO_DIR/assets/boot_logo.png" ]] && cp "$REPO_DIR/assets/boot_logo.png" "$DEST/assets/"
[[ -f "$REPO_DIR/assets/gamecontrollerdb.txt" ]] && cp "$REPO_DIR/assets/gamecontrollerdb.txt" "$DEST/assets/"

info "Cartridges..."
# Copy per-cartridge so anything installed on the device from the store and
# not present in the repo is left alone. The bench cartridge is dev-only.
for dir in "$REPO_DIR"/lua_cartridges/*/; do
    name="$(basename "$dir")"
    [[ "$name" == "bench" ]] && continue
    mkdir -p "$DEST/lua_cartridges/$name"
    cp -R "$dir". "$DEST/lua_cartridges/$name/"
done

# An earlier version of install_to_device.sh copied lua_cartridges into an
# existing lua_cartridges/, leaving a nested duplicate behind.
if [[ -d "$DEST/lua_cartridges/lua_cartridges" ]]; then
    warn "Removing stale nested lua_cartridges/lua_cartridges"
    rm -rf "$DEST/lua_cartridges/lua_cartridges"
fi

# macOS sprinkles AppleDouble files on exFAT; harmless but noisy on the device.
find "$DEST" -name '._*' -delete 2>/dev/null || true

sync
ok "Card updated. Eject it, put it back in the device, and boot."
[[ "$APPS_ONLY" == "0" ]] && ok "Version on card: $("$DEST/cartridge" --version 2>/dev/null || echo 'aarch64 binary (cannot run here)')"
exit 0
