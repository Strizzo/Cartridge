#!/bin/bash
#
# Cartridge Device Installer
#
# Cross-compiles for aarch64 and deploys to a connected handheld device.
# Run this on your computer with the device's SD card mounted or device
# connected via USB Mass Storage.
#
# Usage:
#   ./install_to_device.sh                    # Build and install
#   ./install_to_device.sh --dry-run          # Show what would be done
#   ./install_to_device.sh --skip-build       # Deploy without rebuilding
#   ./install_to_device.sh --device /path     # Specify mount point manually
#   ./install_to_device.sh --skip-overlays     # Skip regenerating overlay PNGs
#

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────

TARGET="aarch64-unknown-linux-gnu"
BINARY_NAME="cartridge"
BOOT_BINARY_NAME="cartridge-boot"
DEVICE_INSTALL_DIR="Cartridge"
DRY_RUN=false
SKIP_BUILD=false
SKIP_OVERLAYS=false
DEVICE_PATH=""

# ── Colors ────────────────────────────────────────────────────────────────────

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info()  { echo -e "${BLUE}[*]${NC} $1"; }
ok()    { echo -e "${GREEN}[+]${NC} $1"; }
warn()  { echo -e "${YELLOW}[!]${NC} $1"; }
fail()  { echo -e "${RED}[x]${NC} $1"; exit 1; }

# ── Parse Arguments ──────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --device)
            DEVICE_PATH="$2"
            shift 2
            ;;
        --device=*)
            DEVICE_PATH="${1#--device=}"
            shift
            ;;
        --skip-overlays)
            SKIP_OVERLAYS=true
            shift
            ;;
        *)
            fail "Unknown argument: $1"
            ;;
    esac
done

# ── Locate Cartridge Source ──────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if [[ ! -f "Cargo.toml" ]]; then
    fail "Cannot find Cargo.toml. Run this script from the Cartridge repo directory."
fi

ok "Cartridge source: $SCRIPT_DIR"

# ── Cross-Compile ────────────────────────────────────────────────────────────

cross_compile() {
    if $SKIP_BUILD; then
        info "Skipping build (--skip-build)"
        return
    fi

    if ! command -v cross &>/dev/null; then
        fail "cross-rs not found. Install with: cargo install cross"
    fi

    info "Cross-compiling all binaries for ${TARGET}..."
    cross build --release --target "$TARGET"

    ok "Build complete (cartridge + cartridge-boot)."
}

# ── Generate Overlay Assets ─────────────────────────────────────────────────

generate_overlays() {
    if $SKIP_OVERLAYS; then
        info "Skipping overlay generation (--skip-overlays)"
        return
    fi

    if [[ -f "assets/overlays/scanlines.png" ]] && [[ -f "assets/overlays/vignette.png" ]] && [[ -f "assets/overlays/grid_bg.png" ]]; then
        info "Overlay assets already exist, skipping generation."
        return
    fi

    if ! command -v python3 &>/dev/null; then
        warn "python3 not found. Overlay PNGs must be generated manually."
        warn "Run: python3 scripts/generate_overlays.py"
        return
    fi

    info "Generating overlay assets..."
    python3 scripts/generate_overlays.py
    ok "Overlays generated."
}

# ── Detect Connected Device ──────────────────────────────────────────────────

find_mount_candidates() {
    local candidates=()
    local system
    system="$(uname -s)"

    case "$system" in
        Darwin)
            for vol in /Volumes/*; do
                [[ -d "$vol" ]] && candidates+=("$vol")
            done
            ;;
        Linux)
            for base in /media /run/media; do
                if [[ -d "$base" ]]; then
                    for user_dir in "$base"/*/; do
                        if [[ -d "$user_dir" ]]; then
                            for vol in "$user_dir"*/; do
                                [[ -d "$vol" ]] && candidates+=("${vol%/}")
                            done
                        fi
                    done
                fi
            done
            for entry in /mnt/*; do
                [[ -d "$entry" ]] && candidates+=("$entry")
            done
            ;;
    esac

    echo "${candidates[@]}"
}

looks_like_handheld() {
    local mount_path="$1"
    # Has a roms/ directory (standard for handheld CFWs)
    if [[ -d "$mount_path/roms" ]]; then
        return 0
    fi
    # Or has tools/ and ports/ or bios/ (roms partition mounted directly)
    if [[ -d "$mount_path/tools" ]] && { [[ -d "$mount_path/ports" ]] || [[ -d "$mount_path/bios" ]]; }; then
        return 0
    fi
    return 1
}

get_roms_dir() {
    local mount_path="$1"
    if [[ -d "$mount_path/roms" ]]; then
        echo "$mount_path/roms"
    elif [[ -d "$mount_path/tools" ]]; then
        echo "$mount_path"
    else
        echo "$mount_path/roms"
    fi
}

detect_device() {
    if [[ -n "$DEVICE_PATH" ]]; then
        if [[ ! -d "$DEVICE_PATH" ]]; then
            fail "Specified device path does not exist: $DEVICE_PATH"
        fi
        ok "Using specified device: $DEVICE_PATH"
        return
    fi

    info "Scanning for connected devices..."

    local devices=()
    for mount in $(find_mount_candidates); do
        if looks_like_handheld "$mount"; then
            devices+=("$mount")
        fi
    done

    if [[ ${#devices[@]} -eq 0 ]]; then
        echo ""
        warn "No handheld device detected."
        echo ""
        echo "Make sure your device is:"
        echo "  1. SD card inserted into a card reader connected to this computer"
        echo "  2. Or device connected via USB in Mass Storage mode"
        echo "  3. Mounted as a drive on this computer"
        echo ""
        local system
        system="$(uname -s)"
        case "$system" in
            Darwin) echo "On macOS, the device should appear in Finder and under /Volumes/" ;;
            Linux)  echo "On Linux, the device should appear in your file manager and under /media/ or /mnt/" ;;
        esac
        echo ""
        echo "You can also specify a path manually:"
        echo "  ./install_to_device.sh --device /path/to/mount"
        echo ""
        exit 1
    fi

    if [[ ${#devices[@]} -eq 1 ]]; then
        DEVICE_PATH="${devices[0]}"
        ok "Found device at: $DEVICE_PATH"
    else
        echo ""
        echo "Multiple devices found:"
        for i in "${!devices[@]}"; do
            echo "  $((i + 1)). ${devices[$i]}"
        done
        echo ""
        while true; do
            read -rp "Select device [1-${#devices[@]}]: " choice
            if [[ "$choice" =~ ^[0-9]+$ ]] && [[ "$choice" -ge 1 ]] && [[ "$choice" -le ${#devices[@]} ]]; then
                DEVICE_PATH="${devices[$((choice - 1))]}"
                break
            fi
            echo "Invalid selection, try again."
        done
    fi
}

# ── Install to Device ────────────────────────────────────────────────────────

install_to_device() {
    local roms_dir
    roms_dir="$(get_roms_dir "$DEVICE_PATH")"
    local dest="$roms_dir/$DEVICE_INSTALL_DIR"
    local tools_dir="$roms_dir/tools"
    local binary="target/${TARGET}/release/${BINARY_NAME}"
    local boot_binary="target/${TARGET}/release/${BOOT_BINARY_NAME}"

    if [[ ! -f "$binary" ]]; then
        fail "Binary not found at $binary. Build first or remove --skip-build."
    fi
    if [[ ! -f "$boot_binary" ]]; then
        fail "Boot binary not found at $boot_binary. Build first or remove --skip-build."
    fi

    if $DRY_RUN; then
        echo ""
        info "DRY RUN -- no files will be written"
        echo ""
        echo "  Source:  $SCRIPT_DIR"
        echo "  Device:  $DEVICE_PATH"
        echo "  Install: $dest/"
        echo ""
        echo "  Would copy:"
        echo "    $binary -> $dest/$BINARY_NAME"
        echo "    $boot_binary -> $dest/$BOOT_BINARY_NAME"
        echo "    deploy/cartridge-boot.sh -> $dest/cartridge-boot.sh"
        echo "    deploy/cartridge-boot.service -> $dest/cartridge-boot.service"
        echo "    registry.json -> $dest/registry.json"
        echo "    assets/ -> $dest/assets/"
        echo "    lua_cartridges/ -> $dest/lua_cartridges/"
        echo ""
        echo "  Tools menu scripts:"
        echo "    deploy/tools/Cartridge.sh -> $tools_dir/Cartridge.sh"
        echo "    deploy/tools/Setup Cartridge Boot.sh -> $tools_dir/Setup Cartridge Boot.sh"
        echo "    deploy/tools/Undo Cartridge Boot.sh -> $tools_dir/Undo Cartridge Boot.sh"
        echo ""
        ok "Dry run complete. Run without --dry-run to install."
        return
    fi

    # Confirm
    echo ""
    info "This will install Cartridge to: $dest/"
    echo ""
    read -rp "Proceed? [Y/n] " answer
    answer="${answer:-y}"

    if [[ "${answer,,}" != "y" && "${answer,,}" != "yes" ]]; then
        echo "Cancelled."
        exit 0
    fi

    echo ""
    info "Installing to $dest..."

    # Create directory structure
    mkdir -p "$dest"
    mkdir -p "$dest/assets/fonts"
    mkdir -p "$dest/assets/overlays"
    mkdir -p "$tools_dir"

    # Copy binaries
    info "Copying binaries..."
    cp "$binary" "$dest/$BINARY_NAME"
    chmod +x "$dest/$BINARY_NAME"
    cp "$boot_binary" "$dest/$BOOT_BINARY_NAME"
    chmod +x "$dest/$BOOT_BINARY_NAME"

    # Copy boot scripts
    info "Copying boot scripts..."
    cp "deploy/cartridge-boot.sh" "$dest/cartridge-boot.sh"
    chmod +x "$dest/cartridge-boot.sh"
    cp "deploy/cartridge-boot.service" "$dest/cartridge-boot.service"
    cp "deploy/autosetup.sh" "$dest/autosetup.sh"
    chmod +x "$dest/autosetup.sh"

    # Copy registry
    if [[ -f "registry.json" ]]; then
        cp "registry.json" "$dest/registry.json"
    fi

    # Copy assets (fonts, overlays)
    info "Copying assets..."
    if [[ -d "assets/fonts" ]]; then
        cp -r assets/fonts/* "$dest/assets/fonts/"
    fi
    if [[ -d "assets/overlays" ]]; then
        cp -r assets/overlays/* "$dest/assets/overlays/"
    fi
    if [[ -f "assets/boot_logo.png" ]]; then
        cp assets/boot_logo.png "$dest/assets/"
    fi

    # Copy Lua cartridges (bundled apps with icons)
    if [[ -d "lua_cartridges" ]]; then
        info "Copying Lua cartridges..."
        cp -r lua_cartridges "$dest/lua_cartridges"
    fi

    ok "Installed to $dest"

    # Copy tools menu scripts (add to existing tools dir, don't overwrite)
    info "Installing tools menu scripts to $tools_dir..."
    for script in "Cartridge.sh" "Setup Cartridge Boot.sh" "Undo Cartridge Boot.sh"; do
        cp "deploy/tools/$script" "$tools_dir/$script"
        chmod +x "$tools_dir/$script"
    done
    ok "Tools menu scripts installed (existing tools preserved)"

    # Done
    echo ""
    echo -e "${BOLD}Done!${NC}"
    echo ""
    echo "  1. Eject the SD card and put it back in your device"
    echo "  2. Boot the device into EmulationStation"
    echo "  3. Go to Tools > 'Cartridge' to launch Cartridge"
    echo "  4. Go to Tools > 'Setup Cartridge Boot' to enable the boot selector"
    echo "     (lets you choose Cartridge or EmulationStation at startup)"
    echo ""
}

# ── Main ─────────────────────────────────────────────────────────────────────

main() {
    echo ""
    echo -e "${BOLD}  Cartridge Device Installer${NC}"
    echo "  =========================="
    echo ""

    cross_compile
    generate_overlays
    detect_device
    install_to_device
}

main "$@"
