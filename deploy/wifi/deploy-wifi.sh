#!/usr/bin/env bash
# Build for the handheld and push it over WiFi -- no SD card shuffling.
#
#   deploy/wifi/deploy-wifi.sh --host 192.168.1.42 --save   # first time
#   deploy/wifi/deploy-wifi.sh                              # after --save
#   deploy/wifi/deploy-wifi.sh --skip-build                 # assets/apps only
#   deploy/wifi/deploy-wifi.sh --payload-only               # don't restart
#
# Prerequisites on the device (once): ArkOS Options > Enable Remote Services,
# then `ssh-copy-id ark@<ip>` from this machine. See docs/wireless-deploy.md.

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

TARGET="aarch64-unknown-linux-gnu"
SKIP_BUILD=0
PAYLOAD_ONLY=0

parse_common_args "$@"
set -- ${REST[@]+"${REST[@]}"}
while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-build)   SKIP_BUILD=1; shift ;;
        --payload-only) PAYLOAD_ONLY=1; shift ;;
        -h|--help)      sed -n '2,12p' "$0"; exit 0 ;;
        *)              fail "Unknown argument: $1" ;;
    esac
done

require_host
cd "$REPO_DIR"

# -- Build -------------------------------------------------------------------

build() {
    if [[ "$SKIP_BUILD" == "1" ]]; then
        info "Skipping build (--skip-build)"
        return
    fi
    command -v cross >/dev/null || fail "cross not found. Install with: cargo install cross"
    if ! docker info >/dev/null 2>&1 && ! podman info >/dev/null 2>&1; then
        fail "cross needs a container runtime and none is running.
      On macOS:  brew install colima docker && colima start
      Then re-run, or pass --skip-build to push the existing binaries."
    fi
    info "Cross-compiling for ${TARGET} (this takes a few minutes cold)..."
    cross build --release --target "$TARGET"
    ok "Build complete"
}

# -- Push --------------------------------------------------------------------

push() {
    local binary="target/${TARGET}/release/cartridge"
    local boot_binary="target/${TARGET}/release/cartridge-boot"
    [[ -f "$binary" ]] || fail "No device binary at $binary -- build first (drop --skip-build)."

    local roms dest tools
    roms="$(remote_roms_dir)"
    dest="${roms}/Cartridge"
    tools="${roms}/tools"
    info "Device roms dir: ${roms}"

    dev_ssh "mkdir -p '${dest}/assets/fonts' '${dest}/assets/overlays' '${dest}/lua_cartridges' '${tools}'"

    # rsync in one pass. --no-perms because exFAT can't store the exec bit;
    # we chmod +x on the device afterwards instead.
    local rsync_opts=(-az --no-perms --omit-dir-times -e "ssh ${SSH_OPTS[*]}")

    info "Pushing binaries..."
    rsync "${rsync_opts[@]}" "$binary" "$boot_binary" "${USER_NAME}@${HOST}:${dest}/"

    info "Pushing scripts, registry and assets..."
    rsync "${rsync_opts[@]}" \
        deploy/cartridge-boot.sh deploy/cartridge-boot.service deploy/autosetup.sh \
        registry.json \
        "${USER_NAME}@${HOST}:${dest}/"
    rsync "${rsync_opts[@]}" --delete assets/fonts/ "${USER_NAME}@${HOST}:${dest}/assets/fonts/"
    rsync "${rsync_opts[@]}" --delete assets/overlays/ "${USER_NAME}@${HOST}:${dest}/assets/overlays/"
    [[ -f assets/boot_logo.png ]] && rsync "${rsync_opts[@]}" assets/boot_logo.png "${USER_NAME}@${HOST}:${dest}/assets/"
    [[ -f assets/gamecontrollerdb.txt ]] && rsync "${rsync_opts[@]}" assets/gamecontrollerdb.txt "${USER_NAME}@${HOST}:${dest}/assets/"

    info "Pushing cartridges..."
    rsync "${rsync_opts[@]}" --delete lua_cartridges/ "${USER_NAME}@${HOST}:${dest}/lua_cartridges/"

    info "Pushing Tools menu scripts..."
    rsync "${rsync_opts[@]}" deploy/tools/ "${USER_NAME}@${HOST}:${tools}/"

    dev_ssh "chmod +x '${dest}/cartridge' '${dest}/cartridge-boot' '${dest}/cartridge-boot.sh' '${dest}/autosetup.sh' '${tools}'/*.sh 2>/dev/null || true"
    ok "Payload installed to ${dest}"

    local version
    version="$(dev_ssh "cd '${dest}' && ./cartridge --version 2>/dev/null" || true)"
    [[ -n "$version" ]] && ok "Deployed: ${version}"
}

# -- Restart -----------------------------------------------------------------

restart() {
    if [[ "$PAYLOAD_ONLY" == "1" ]]; then
        info "Not restarting (--payload-only)"
        return
    fi
    # The one-shot flag makes cartridge-boot.sh skip its 5 s selector, so a
    # redeploy lands straight back in the launcher. It is consumed on use.
    dev_ssh "touch /tmp/.cartridge_skip_selector" || true

    if dev_ssh "systemctl list-unit-files cartridge-boot.service >/dev/null 2>&1 && systemctl is-enabled cartridge-boot >/dev/null 2>&1"; then
        info "Restarting cartridge-boot.service..."
        dev_ssh "sudo systemctl restart cartridge-boot" \
            || warn "systemctl restart failed -- restart the device or relaunch from the Tools menu."
        ok "Launcher restarting on the device"
    else
        warn "cartridge-boot.service not enabled on this device."
        info "Killing any running launcher; relaunch it from EmulationStation > Tools > Cartridge."
        dev_ssh "pkill -f '/cartridge$' || true"
    fi
}

echo
echo "  CartridgeOS wireless deploy -> ${USER_NAME}@${HOST}"
echo "  ============================================"
echo
check_reachable
build
push
restart
echo
ok "Done. Logs: deploy/wifi/device-logs.sh    Screenshot: deploy/wifi/device-shot.sh"
