#!/bin/bash
# Build in a disposable ARM64 Linux VM/container, never on a physical SD card.
# Prerequisites: mmdebstrap, Debian archive keyring, meson, ninja, gcc, libcap-dev,
# curl, xz, Python 3, and root rights for the private Debian bootstrap.
set -euo pipefail
[[ "$(uname -s)" == Linux && "$(uname -m)" == aarch64 && "$(id -u)" == 0 ]] || {
    echo 'Run inside a disposable ARM64 Linux build VM/container as root.' >&2; exit 1;
}
[[ $# -eq 1 ]] || { echo 'Usage: build-arm64.sh /absolute/new-component-directory' >&2; exit 2; }
component_dir="$1"
[[ "$component_dir" == /* && ! -e "$component_dir" && ! -L "$component_dir" ]] || {
    echo 'Supply a new absolute output directory. Existing data will not be replaced.' >&2; exit 1;
}
for build_tool in mmdebstrap meson ninja gcc curl xz python3; do command -v "$build_tool" >/dev/null; done
source_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work_dir="$(mktemp -d /tmp/cartridge-browser-build.XXXXXXXX)"
trap '[[ "$work_dir" == /tmp/cartridge-browser-build.* ]] && rm -rf -- "$work_dir"' EXIT
mkdir "$component_dir"
printf '%s\n' 'Cartridge Browser private component build v1' > "$component_dir/.cartridge-browser-build"
mmdebstrap --mode=root --architectures=arm64 --variant=apt \
    --keyring=/usr/share/keyrings/debian-archive-keyring.gpg \
    --include=chromium,ca-certificates,fonts-dejavu-core \
    bookworm "$component_dir/rootfs" \
    'deb https://deb.debian.org/debian bookworm main' \
    'deb https://security.debian.org/debian-security bookworm-security main'
curl --fail --location --proto '=https' --tlsv1.2 \
    https://github.com/containers/bubblewrap/releases/download/v0.13.1/bubblewrap-0.13.1.tar.xz \
    -o "$work_dir/bubblewrap.tar.xz"
printf '%s  %s\n' 0e85c8702248b8dd64144f594147553650cdd0e4c951690345faace79c217c4d "$work_dir/bubblewrap.tar.xz" | sha256sum --check -
tar -xJf "$work_dir/bubblewrap.tar.xz" -C "$work_dir"
# Keep upstream's old-kernel fallbacks; do not set assume_kernel to a modern VM.
meson setup "$work_dir/build" "$work_dir/bubblewrap-0.13.1" \
    --buildtype=release -Dman=disabled -Dbash_completion=disabled \
    -Dzsh_completion=disabled -Dtests=false -Dselinux=disabled \
    -Dprefer_static=true -Dc_args=-march=armv8-a -Dc_link_args=-static
meson compile -C "$work_dir/build"
install -m 755 "$work_dir/build/bwrap" "$component_dir/bubblewrap"
install -m 755 "$source_dir/chromium" "$component_dir/chromium"
mkdir "$component_dir/notices"
cp "$work_dir/bubblewrap-0.13.1/COPYING" "$component_dir/notices/bubblewrap.txt"
cp /usr/share/doc/libc6/copyright "$component_dir/notices/static-libc.txt"
cp /usr/share/doc/libcap2/copyright "$component_dir/notices/static-libcap.txt"
printf '%s\n' \
    'Browser packages: signed Debian bookworm and bookworm-security archives; see packages.tsv and rootfs/usr/share/doc/*/copyright.' \
    'Bubblewrap source: https://github.com/containers/bubblewrap/releases/tag/v0.13.1' \
    'Source tarball SHA-256: 0e85c8702248b8dd64144f594147553650cdd0e4c951690345faace79c217c4d' \
    > "$component_dir/notices/sources.txt"
python3 "$source_dir/prepare_component.py" "$component_dir"
printf 'Private ARM64 component prepared: %s\n' "$component_dir"
printf '%s\n' 'Test it with namespaces enabled before packaging or installing. This script does not publish, sign, or write a card.'
