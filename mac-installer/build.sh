#!/bin/bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
app="${1:-${TMPDIR:-/private/tmp}/CartridgeOS Installer.app}"
resources="$app/Contents/Resources"
mkdir -p "$app/Contents/MacOS" "$resources/installer" "$resources/sim/device"

export CLANG_MODULE_CACHE_PATH="${TMPDIR:-/private/tmp}/cartridge-installer-clang-cache"
export SWIFT_MODULE_CACHE_PATH="${TMPDIR:-/private/tmp}/cartridge-installer-swift-cache"
swiftc -parse-as-library -o "$app/Contents/MacOS/CartridgeInstaller" \
    "$root/mac-installer/CartridgeInstaller.swift"
clang -O2 -Wall -Wextra -framework DiskArbitration -framework CoreFoundation \
    "$root/sim/device/disk-mount-guard.c" -o "$resources/disk-mount-guard"
cp "$root"/installer/*.py "$resources/installer/"
cp "$root"/sim/device/*.py "$resources/sim/device/"
cp "$root/assets/boot_logo.png" "$resources/boot_logo.png"

cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>CartridgeOS Installer</string>
<key>CFBundleDisplayName</key><string>CartridgeOS Installer</string>
<key>CFBundleIdentifier</key><string>dev.cartridge.installer</string>
<key>CFBundleExecutable</key><string>CartridgeInstaller</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

plutil -lint "$app/Contents/Info.plist"
printf 'Built %s\n' "$app"
