#!/bin/bash
#
# Cartridge Build & Install Script
#
# Builds the Cartridge binary and sets up a local development environment.
#
# Usage:
#   ./install.sh              # Build release binary
#   ./install.sh --debug      # Build debug binary
#   ./install.sh --prefix /opt/cartridge  # Install to a specific location
#

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────

PREFIX="${PREFIX:-}"
BUILD_TYPE="release"

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
        --debug)
            BUILD_TYPE="debug"
            shift
            ;;
        --prefix)
            PREFIX="$2"
            shift 2
            ;;
        --prefix=*)
            PREFIX="${1#--prefix=}"
            shift
            ;;
        *)
            fail "Unknown argument: $1"
            ;;
    esac
done

# ── Check Dependencies ──────────────────────────────────────────────────────

check_dependencies() {
    if ! command -v cargo &>/dev/null; then
        fail "cargo not found. Install Rust: https://rustup.rs"
    fi

    # Check for SDL2 development libraries
    local missing=()

    if [[ "$(uname -s)" == "Darwin" ]]; then
        # macOS: check Homebrew
        for lib in sdl2 sdl2_ttf sdl2_gfx sdl2_image; do
            if ! brew list "$lib" &>/dev/null 2>&1; then
                missing+=("$lib")
            fi
        done
        if [[ ${#missing[@]} -gt 0 ]]; then
            warn "Missing Homebrew packages: ${missing[*]}"
            info "Install with: brew install ${missing[*]}"
            fail "SDL2 development libraries required."
        fi
    elif [[ "$(uname -s)" == "Linux" ]]; then
        for lib in libsdl2-dev libsdl2-ttf-dev libsdl2-gfx-dev libsdl2-image-dev; do
            if ! dpkg -s "$lib" &>/dev/null 2>&1; then
                missing+=("$lib")
            fi
        done
        if [[ ${#missing[@]} -gt 0 ]]; then
            warn "Missing packages: ${missing[*]}"
            info "Install with: sudo apt-get install ${missing[*]}"
            fail "SDL2 development libraries required."
        fi
    fi
}

# ── Build ────────────────────────────────────────────────────────────────────

build() {
    info "Building Cartridge (${BUILD_TYPE})..."

    if [[ "$BUILD_TYPE" == "release" ]]; then
        cargo build --release
    else
        cargo build
    fi

    ok "Build complete."
}

# ── Install ──────────────────────────────────────────────────────────────────

install_to_prefix() {
    if [[ -z "$PREFIX" ]]; then
        return
    fi

    info "Installing to ${PREFIX}..."

    mkdir -p "$PREFIX"
    mkdir -p "$PREFIX/assets"
    mkdir -p "$PREFIX/cartridges"

    # Copy binary
    if [[ "$BUILD_TYPE" == "release" ]]; then
        cp "target/release/cartridge" "$PREFIX/cartridge"
    else
        cp "target/debug/cartridge" "$PREFIX/cartridge"
    fi
    chmod +x "$PREFIX/cartridge"

    # Copy assets
    cp -r assets/* "$PREFIX/assets/"

    # Copy cartridges
    cp -r cartridges/* "$PREFIX/cartridges/"

    ok "Installed to ${PREFIX}"
    echo ""
    echo "  Binary:     ${PREFIX}/cartridge"
    echo "  Assets:     ${PREFIX}/assets/"
    echo "  Cartridges: ${PREFIX}/cartridges/"
    echo ""
}

# ── Main ─────────────────────────────────────────────────────────────────────

main() {
    echo ""
    echo -e "${BOLD}  Cartridge Build Script${NC}"
    echo "  ======================"
    echo ""

    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    cd "$SCRIPT_DIR"

    check_dependencies
    build
    install_to_prefix

    if [[ -z "$PREFIX" ]]; then
        echo ""
        ok "Build successful. Run with:"
        echo ""
        if [[ "$BUILD_TYPE" == "release" ]]; then
            echo "  cargo run --release"
        else
            echo "  cargo run"
        fi
        echo "  cargo run -- run --path cartridges/calculator"
        echo "  cargo run -- demo"
        echo ""
    fi
}

main "$@"
