#!/bin/bash
#
# Cartridge Installer
# Installs the Cartridge app ecosystem on Linux handheld devices and desktops.
#
# USB install (recommended for devices):
#   1. Connect your device to your computer via USB
#   2. Copy the Cartridge/ folder to roms/Cartridge/ on the device
#   3. Copy this file to roms/tools/Install Cartridge.sh
#   4. Run "Install Cartridge" from Tools in EmulationStation
#
# Network install (via SSH):
#   curl -sSL https://raw.githubusercontent.com/Strizzo/Cartridge/main/install.sh | bash
#
# Environment variables:
#   CARTRIDGE_DIR    Install directory (default: /opt/cartridge)
#   CARTRIDGE_BRANCH Git branch to install from (default: main)
#   CARTRIDGE_REPO   Git repo URL (default: https://github.com/Strizzo/Cartridge.git)
#

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────

CARTRIDGE_DIR="${CARTRIDGE_DIR:-/opt/cartridge}"
CARTRIDGE_BRANCH="${CARTRIDGE_BRANCH:-main}"
CARTRIDGE_REPO="${CARTRIDGE_REPO:-https://github.com/Strizzo/Cartridge.git}"

VENV_DIR="$CARTRIDGE_DIR/venv"
INSTALLED_DIR="$HOME/.cartridges/installed"
MIN_PYTHON_MINOR=9  # Python 3.9+ (required by pygame-ce and aiohttp)

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

# ── Platform Detection ────────────────────────────────────────────────────────

detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)  PLATFORM="linux" ;;
        Darwin) PLATFORM="macos" ;;
        MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
        *)      PLATFORM="unknown" ;;
    esac

    # Detect handheld CFW
    CFW="none"
    if [ "$PLATFORM" = "linux" ]; then
        if [ -f /etc/emulationstation/es_systems.cfg ] || [ -d /roms ]; then
            if [ -f /opt/system/Advanced/Switch\ to\ RetroArch.sh ] || grep -qi "arkos" /etc/hostname 2>/dev/null; then
                CFW="arkos"
            elif [ -f /usr/share/rocknix/rocknix.conf ] || grep -qi "rocknix" /etc/hostname 2>/dev/null; then
                CFW="rocknix"
            elif grep -qi "knulli" /etc/hostname 2>/dev/null || [ -f /usr/share/knulli/knulli.conf ]; then
                CFW="knulli"
            elif [ -d /roms/tools ]; then
                CFW="generic-handheld"
            fi
        fi
    fi

    IS_HANDHELD=false
    if [ "$CFW" != "none" ]; then
        IS_HANDHELD=true
    fi
}

# ── Privilege Helper ──────────────────────────────────────────────────────────

maybe_sudo() {
    if [ "$(id -u)" -eq 0 ]; then
        "$@"
    elif command -v sudo &>/dev/null; then
        sudo "$@"
    else
        warn "No sudo available, trying without..."
        "$@"
    fi
}

# ── Python Detection & Installation ──────────────────────────────────────────

find_python() {
    # Try python3.12, python3.11, python3.10, python3.9, python3.8, python3 in order
    for cmd in python3.12 python3.11 python3.10 python3.9 python3.8 python3; do
        if command -v "$cmd" &>/dev/null; then
            local ver
            ver="$("$cmd" -c 'import sys; print(f"{sys.version_info.minor}")'  2>/dev/null || echo "0")"
            if [ "$ver" -ge "$MIN_PYTHON_MINOR" ]; then
                PYTHON_CMD="$cmd"
                PYTHON_VERSION="$("$cmd" --version 2>&1)"
                return 0
            fi
        fi
    done
    return 1
}

install_python_linux() {
    info "Installing Python 3.9+..."

    if command -v apt-get &>/dev/null; then
        maybe_sudo apt-get update -qq || true

        # Try to install Python 3.11, 3.10, or 3.9 from repos
        local installed=false
        for pyver in 3.11 3.10 3.9; do
            if apt-cache show "python${pyver}" &>/dev/null 2>&1; then
                info "Installing python${pyver} from repos..."
                maybe_sudo apt-get install -y -qq "python${pyver}" "python${pyver}-venv" "python${pyver}-dev" 2>/dev/null && installed=true && break
            fi
        done

        if ! $installed; then
            # Repos don't have 3.9+. Build Python 3.11 from source.
            info "Python 3.9+ not in repos. Building Python 3.11 from source..."
            info "This will take 10-20 minutes on a handheld device."
            maybe_sudo apt-get install -y -qq build-essential libssl-dev zlib1g-dev \
                libffi-dev libbz2-dev libreadline-dev libsqlite3-dev wget curl \
                libncurses5-dev libncursesw5-dev xz-utils liblzma-dev 2>/dev/null || true

            local py_src="/tmp/python-build"
            mkdir -p "$py_src"
            cd "$py_src"
            wget -q "https://www.python.org/ftp/python/3.11.11/Python-3.11.11.tgz" -O python.tgz
            tar xzf python.tgz
            cd Python-3.11.11
            ./configure --prefix=/usr/local --enable-optimizations --with-ensurepip=install 2>&1 | tail -1
            make -j"$(nproc)" 2>&1 | tail -5
            maybe_sudo make altinstall
            cd /
            rm -rf "$py_src"
            ok "Built Python 3.11 from source"
        fi

        # SDL2 dev libs for pygame-ce
        maybe_sudo apt-get install -y -qq libsdl2-dev libsdl2-image-dev libsdl2-mixer-dev libsdl2-ttf-dev 2>/dev/null || true

    elif command -v dnf &>/dev/null; then
        maybe_sudo dnf install -y python3 python3-pip python3-devel SDL2-devel SDL2_image-devel SDL2_mixer-devel SDL2_ttf-devel

    elif command -v pacman &>/dev/null; then
        maybe_sudo pacman -Sy --noconfirm python python-pip sdl2 sdl2_image sdl2_mixer sdl2_ttf

    elif command -v apk &>/dev/null; then
        maybe_sudo apk add python3 py3-pip sdl2-dev sdl2_image-dev sdl2_mixer-dev sdl2_ttf-dev

    else
        fail "Could not detect package manager. Install Python 3.9+ manually."
    fi
}

ensure_python() {
    if find_python; then
        ok "Found $PYTHON_VERSION"
        return
    fi

    if [ "$PLATFORM" = "macos" ]; then
        if command -v brew &>/dev/null; then
            info "Installing Python via Homebrew..."
            brew install python@3.12
        else
            fail "Python 3.11+ not found. Install via: brew install python@3.12"
        fi
    elif [ "$PLATFORM" = "linux" ]; then
        install_python_linux
    elif [ "$PLATFORM" = "windows" ]; then
        fail "Python 3.11+ not found. Download from https://www.python.org/downloads/"
    else
        fail "Python 3.11+ not found. Install it for your platform."
    fi

    # Verify install worked
    if ! find_python; then
        fail "Python 3.9+ is required but could not be installed. Your system has $(python3 --version 2>&1 || echo 'no python3'). Dependencies (pygame-ce, aiohttp) need Python 3.9+."
    fi

    ok "Installed $PYTHON_VERSION"
}

# ── Local Source Detection ───────────────────────────────────────────────────

detect_local_source() {
    # Check if the script is sitting inside (or next to) the Cartridge repo.
    # This is the case when the user copied the folder to the SD card.
    LOCAL_SOURCE=""

    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || pwd)"

    for candidate in "$SCRIPT_DIR" "$SCRIPT_DIR/.." "$SCRIPT_DIR/../Cartridge"; do
        if [ -f "$candidate/pyproject.toml" ] && [ -d "$candidate/src" ] && [ -d "$candidate/cartridges" ]; then
            LOCAL_SOURCE="$(cd "$candidate" && pwd)"
            return
        fi
    done

    # Common SD card locations where user might have copied the folder
    if $IS_HANDHELD; then
        for candidate in /roms/Cartridge /roms/cartridge /storage/roms/Cartridge /storage/roms/cartridge; do
            if [ -f "$candidate/pyproject.toml" ] && [ -d "$candidate/src" ] && [ -d "$candidate/cartridges" ]; then
                LOCAL_SOURCE="$(cd "$candidate" && pwd)"
                return
            fi
        done
    fi
}

# ── Git Detection ────────────────────────────────────────────────────────────

ensure_git() {
    if command -v git &>/dev/null; then
        return
    fi

    info "Installing git..."
    if [ "$PLATFORM" = "linux" ]; then
        if command -v apt-get &>/dev/null; then
            maybe_sudo apt-get install -y -qq git
        elif command -v dnf &>/dev/null; then
            maybe_sudo dnf install -y git
        elif command -v pacman &>/dev/null; then
            maybe_sudo pacman -Sy --noconfirm git
        fi
    elif [ "$PLATFORM" = "macos" ]; then
        # macOS ships git via xcode-select
        xcode-select --install 2>/dev/null || true
    fi

    command -v git &>/dev/null || fail "git not found. Install git manually."
}

# ── Install Cartridge ────────────────────────────────────────────────────────

install_cartridge() {
    info "Installing Cartridge to $CARTRIDGE_DIR..."

    # Create install directory
    if [ ! -d "$CARTRIDGE_DIR" ]; then
        maybe_sudo mkdir -p "$CARTRIDGE_DIR"
        maybe_sudo chown "$(id -u):$(id -g)" "$CARTRIDGE_DIR"
    fi

    if [ -n "$LOCAL_SOURCE" ] && [ "$LOCAL_SOURCE" != "$CARTRIDGE_DIR" ]; then
        # ── Local install from SD card / USB ──
        info "Installing from local source: $LOCAL_SOURCE"
        cp -r "$LOCAL_SOURCE/src" "$CARTRIDGE_DIR/"
        cp -r "$LOCAL_SOURCE/cartridges" "$CARTRIDGE_DIR/"
        cp -f "$LOCAL_SOURCE/pyproject.toml" "$CARTRIDGE_DIR/"
        cp -f "$LOCAL_SOURCE/install.sh" "$CARTRIDGE_DIR/" 2>/dev/null || true
        [ -f "$LOCAL_SOURCE/registry.json" ] && cp -f "$LOCAL_SOURCE/registry.json" "$CARTRIDGE_DIR/"
        [ -f "$LOCAL_SOURCE/README.md" ] && cp -f "$LOCAL_SOURCE/README.md" "$CARTRIDGE_DIR/"
        ok "Copied from SD card"

    elif [ -n "$LOCAL_SOURCE" ] && [ "$LOCAL_SOURCE" = "$CARTRIDGE_DIR" ]; then
        # ── Already in install dir (re-running installer) ──
        info "Source already at $CARTRIDGE_DIR"

    elif [ -d "$CARTRIDGE_DIR/.git" ]; then
        # ── Update existing git install ──
        info "Updating existing installation..."
        cd "$CARTRIDGE_DIR"
        ensure_git
        git fetch origin
        git checkout "$CARTRIDGE_BRANCH"
        git pull origin "$CARTRIDGE_BRANCH"

    else
        # ── Fresh network install (needs git + internet) ──
        ensure_git
        info "Cloning Cartridge..."
        git clone --branch "$CARTRIDGE_BRANCH" --depth 1 "$CARTRIDGE_REPO" "$CARTRIDGE_DIR"
    fi

    cd "$CARTRIDGE_DIR"

    # Create virtual environment
    if [ ! -d "$VENV_DIR" ]; then
        info "Creating virtual environment..."
        "$PYTHON_CMD" -m venv "$VENV_DIR"
    fi

    # Activate venv and install
    source "$VENV_DIR/bin/activate"

    info "Installing Python dependencies..."
    pip install --upgrade pip -q
    pip install -e ".[stock]" -q

    ok "Cartridge SDK installed"
}

# ── Install Bundled Apps ─────────────────────────────────────────────────────

install_bundled_apps() {
    info "Installing bundled cartridges..."

    mkdir -p "$INSTALLED_DIR"

    for app_dir in "$CARTRIDGE_DIR"/cartridges/*/; do
        app_name="$(basename "$app_dir")"
        if [ -f "$app_dir/cartridge.toml" ]; then
            dest="$INSTALLED_DIR/$app_name"
            if [ -d "$dest" ]; then
                rm -rf "$dest"
            fi
            cp -r "$app_dir" "$dest"
        fi
    done

    app_count=$(find "$INSTALLED_DIR" -maxdepth 1 -mindepth 1 -type d | wc -l | tr -d ' ')
    ok "Installed $app_count bundled cartridges"
}

# ── EmulationStation Integration ─────────────────────────────────────────────

setup_emulationstation() {
    if ! $IS_HANDHELD; then
        return
    fi

    info "Registering Cartridge as EmulationStation system..."

    # ── 1. Create the launcher script ──
    mkdir -p "$CARTRIDGE_DIR/bin"
    cat > "$CARTRIDGE_DIR/bin/launch.sh" << 'LAUNCH'
#!/bin/bash
# Cartridge launcher — called by EmulationStation
SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
source "$SCRIPT_DIR/venv/bin/activate"
cartridge home "$HOME/.cartridges/installed/cartridge_client" --fullscreen
LAUNCH
    chmod +x "$CARTRIDGE_DIR/bin/launch.sh"

    # ── 2. Create a dummy entry so ES has something to list ──
    # ES requires at least one file matching <extension> inside <path>
    CARTRIDGE_ROMS="$CARTRIDGE_DIR/es"
    mkdir -p "$CARTRIDGE_ROMS"
    cat > "$CARTRIDGE_ROMS/Cartridge.sh" << ENTRY
#!/bin/bash
exec "$CARTRIDGE_DIR/bin/launch.sh"
ENTRY
    chmod +x "$CARTRIDGE_ROMS/Cartridge.sh"

    # ── 3. Register the system in es_systems.cfg ──
    ES_CONFIG=""
    for candidate in "$HOME/.emulationstation/es_systems.cfg" /etc/emulationstation/es_systems.cfg; do
        if [ -f "$candidate" ]; then
            ES_CONFIG="$candidate"
            break
        fi
    done

    if [ -z "$ES_CONFIG" ]; then
        warn "Could not find es_systems.cfg. Skipping system registration."
        warn "You can run Cartridge manually: $CARTRIDGE_DIR/bin/launch.sh"
        return
    fi

    # Check if already registered
    if grep -q '<name>cartridge</name>' "$ES_CONFIG" 2>/dev/null; then
        ok "Cartridge system already registered in ES"
    else
        info "Adding Cartridge to $ES_CONFIG..."

        # Insert our system entry before </systemList>
        SYSTEM_ENTRY='  <system>\n    <name>cartridge</name>\n    <fullname>Cartridge</fullname>\n    <path>'"$CARTRIDGE_ROMS"'</path>\n    <extension>.sh</extension>\n    <command>%ROM%</command>\n    <platform>pc</platform>\n    <theme>cartridge</theme>\n  </system>'

        # Back up the original
        cp "$ES_CONFIG" "$ES_CONFIG.bak"

        # Insert before closing tag
        if [ "$(id -u)" -eq 0 ] || [ -w "$ES_CONFIG" ]; then
            sed -i "s|</systemList>|${SYSTEM_ENTRY}\n</systemList>|" "$ES_CONFIG"
        else
            maybe_sudo sed -i "s|</systemList>|${SYSTEM_ENTRY}\n</systemList>|" "$ES_CONFIG"
        fi

        ok "Cartridge registered as top-level ES system"
    fi

    # ── 4. Set up theme entry ──
    setup_es_theme

    info "Restart EmulationStation to see Cartridge in the main carousel."
}

# ── EmulationStation Theme ──────────────────────────────────────────────────

setup_es_theme() {
    # Find the active theme directory
    THEME_DIR=""
    for candidate in /roms/themes /storage/roms/themes "$HOME/.emulationstation/themes" /etc/emulationstation/themes; do
        if [ -d "$candidate" ]; then
            THEME_DIR="$candidate"
            break
        fi
    done

    if [ -z "$THEME_DIR" ]; then
        warn "Could not find ES themes directory. Cartridge will appear without a logo."
        return
    fi

    # Find the currently active theme to add our system to it
    # Most themes have per-system subdirectories; we add a "cartridge" folder
    # Try to detect which theme is in use by finding one with system folders
    ACTIVE_THEME=""
    for theme in "$THEME_DIR"/*/; do
        # A theme usually has subfolders matching system names (nes, snes, gba, etc.)
        if [ -d "$theme/nes" ] || [ -d "$theme/snes" ] || [ -d "$theme/gba" ] || [ -d "$theme/tools" ]; then
            ACTIVE_THEME="$theme"
            break
        fi
    done

    if [ -z "$ACTIVE_THEME" ]; then
        # Fallback: create a standalone theme
        ACTIVE_THEME="$THEME_DIR/cartridge-theme/"
        mkdir -p "$ACTIVE_THEME"
    fi

    CART_THEME="$ACTIVE_THEME/cartridge"
    mkdir -p "$CART_THEME"

    # Create a minimal theme.xml for the Cartridge system
    cat > "$CART_THEME/theme.xml" << 'THEME_XML'
<?xml version="1.0"?>
<theme>
  <formatVersion>7</formatVersion>
  <view name="system">
    <text name="systemInfo">
      <text>Apps &amp; Tools</text>
    </text>
  </view>
  <view name="basic, detailed">
    <text name="description">
      <text>Cartridge - Apps for your handheld</text>
    </text>
  </view>
</theme>
THEME_XML

    ok "Theme entry added at $CART_THEME"
}

# ── Desktop Instructions ─────────────────────────────────────────────────────

print_desktop_instructions() {
    echo ""
    echo -e "${BOLD}Cartridge installed successfully!${NC}"
    echo ""
    echo "To run an app:"
    echo "  source $VENV_DIR/bin/activate"
    echo "  cartridge run --path $CARTRIDGE_DIR/cartridges/hacker_news"
    echo ""
    echo "To run the full launcher:"
    echo "  source $VENV_DIR/bin/activate"
    echo "  cartridge home $CARTRIDGE_DIR/cartridges/cartridge_client"
    echo ""
    echo "To install a cartridge from GitHub:"
    echo "  source $VENV_DIR/bin/activate"
    echo "  cartridge install https://github.com/user/their-app"
    echo ""
    echo "To create a new cartridge:"
    echo "  source $VENV_DIR/bin/activate"
    echo "  cartridge new \"My App\""
    echo ""
}

print_device_instructions() {
    echo ""
    echo -e "${BOLD}Cartridge installed successfully!${NC}"
    echo ""
    echo "Restart EmulationStation. Cartridge will appear in the main carousel"
    echo "alongside your game systems."
    echo ""
    echo "To install more apps via SSH:"
    echo "  source $VENV_DIR/bin/activate"
    echo "  cartridge install https://github.com/user/their-app"
    echo ""
}

# ── Uninstall ────────────────────────────────────────────────────────────────

uninstall() {
    warn "Uninstalling Cartridge..."

    # Remove ES system registration
    for candidate in "$HOME/.emulationstation/es_systems.cfg" /etc/emulationstation/es_systems.cfg; do
        if [ -f "$candidate" ] && grep -q '<name>cartridge</name>' "$candidate" 2>/dev/null; then
            info "Removing Cartridge from $candidate..."
            if [ "$(id -u)" -eq 0 ] || [ -w "$candidate" ]; then
                sed -i '/<system>/,/<\/system>/{/<name>cartridge<\/name>/,/<\/system>/d; /<system>/d}' "$candidate"
            else
                maybe_sudo sed -i '/<system>/,/<\/system>/{/<name>cartridge<\/name>/,/<\/system>/d; /<system>/d}' "$candidate"
            fi
            # Clean up any leftover empty lines
            ok "Removed from ES config"
            break
        fi
    done

    # Remove ES theme entry
    for theme_base in /roms/themes /storage/roms/themes "$HOME/.emulationstation/themes" /etc/emulationstation/themes; do
        if [ -d "$theme_base" ]; then
            find "$theme_base" -type d -name "cartridge" -exec rm -rf {} + 2>/dev/null || true
        fi
    done

    # Remove old-style Tools launcher (if any)
    for f in /roms/tools/Cartridge.sh /roms/ports/Cartridge.sh /storage/roms/tools/Cartridge.sh /storage/roms/ports/Cartridge.sh; do
        [ -f "$f" ] && rm -f "$f"
    done

    # Remove install dir
    if [ -d "$CARTRIDGE_DIR" ]; then
        maybe_sudo rm -rf "$CARTRIDGE_DIR"
        ok "Removed $CARTRIDGE_DIR"
    fi

    # Ask about user data
    if [ -d "$HOME/.cartridges" ]; then
        echo ""
        read -rp "Remove app data (~/.cartridges)? [y/N] " answer
        if [ "$answer" = "y" ] || [ "$answer" = "Y" ]; then
            rm -rf "$HOME/.cartridges"
            ok "Removed ~/.cartridges"
        else
            info "Kept ~/.cartridges"
        fi
    fi

    ok "Cartridge uninstalled."
    exit 0
}

# ── Cleanup Staging Files ───────────────────────────────────────────────────

cleanup_staging() {
    info "Cleaning up staging files from SD card..."

    # Remove the Cartridge source folder that was copied via USB
    if [ -d "$LOCAL_SOURCE" ]; then
        rm -rf "$LOCAL_SOURCE"
        ok "Removed staging folder: $LOCAL_SOURCE"
    fi

    # Remove the Tools launcher (Cartridge now lives in the main carousel)
    for f in /roms/tools/Cartridge.sh /roms/tools/"Install Cartridge.sh" \
             /roms/ports/Cartridge.sh /roms/ports/"Install Cartridge.sh" \
             /storage/roms/tools/Cartridge.sh /storage/roms/tools/"Install Cartridge.sh" \
             /storage/roms/ports/Cartridge.sh /storage/roms/ports/"Install Cartridge.sh"; do
        if [ -f "$f" ]; then
            rm -f "$f"
            ok "Removed: $f"
        fi
    done
}

# ── Restart EmulationStation ─────────────────────────────────────────────────

restart_emulationstation() {
    info "Restarting EmulationStation..."
    if command -v systemctl &>/dev/null; then
        # ArkOS uses 'emustation', others use 'emulationstation'
        if systemctl list-units --type=service | grep -q emustation; then
            maybe_sudo systemctl restart emustation
        elif systemctl list-units --type=service | grep -q emulationstation; then
            maybe_sudo systemctl restart emulationstation
        fi
    fi
}

# ── Main ─────────────────────────────────────────────────────────────────────

main() {
    CUR_TTY="${CUR_TTY:-/dev/tty0}"

    # If running on a handheld, redirect output to the console so the user
    # can see progress on screen (EmulationStation hides stdout otherwise)
    if [ -e "$CUR_TTY" ] && [ ! -t 1 ]; then
        export TERM=linux
        exec > >(tee "$CUR_TTY") 2>&1
        printf "\033c" > "$CUR_TTY"
    fi

    echo ""
    echo -e "${BOLD}  Cartridge Installer${NC}"
    echo "  ==================="
    echo ""

    # Handle --uninstall flag
    if [ "${1:-}" = "--uninstall" ]; then
        detect_platform
        uninstall
    fi

    detect_platform
    info "Platform: $PLATFORM ($ARCH)"
    if $IS_HANDHELD; then
        info "Detected handheld CFW: $CFW"
    fi

    # Check if already installed — if so, just launch
    if $IS_HANDHELD && [ -d "$VENV_DIR" ] && [ -f "$CARTRIDGE_DIR/bin/launch.sh" ]; then
        ok "Cartridge is already installed. Launching..."
        exec "$CARTRIDGE_DIR/bin/launch.sh"
    fi

    detect_local_source
    if [ -n "$LOCAL_SOURCE" ]; then
        ok "Found Cartridge files at: $LOCAL_SOURCE"
    fi

    ensure_python
    install_cartridge
    install_bundled_apps
    setup_emulationstation

    # Clean up staging files from the SD card
    if $IS_HANDHELD && [ -n "$LOCAL_SOURCE" ] && [ "$LOCAL_SOURCE" != "$CARTRIDGE_DIR" ]; then
        cleanup_staging
    fi

    if $IS_HANDHELD; then
        ok "Cartridge installed successfully!"
        info "Restarting EmulationStation in 3 seconds..."
        info "Cartridge will appear in the main carousel."
        sleep 3
        restart_emulationstation
    else
        print_desktop_instructions
    fi
}

main "$@"
