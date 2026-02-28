#!/usr/bin/env python3
"""
Cartridge Device Installer

Detects a USB-connected Linux handheld device and installs Cartridge onto it.
Run this on your computer with the device plugged in via USB.

Usage:
    python3 install_to_device.py

The installer will:
1. Scan for mounted volumes that look like a handheld device (has a roms/ directory)
2. Copy the Cartridge files to the device
3. Place the install script in the Tools menu
4. Tell you to disconnect and finish setup on-device
"""

import os
import sys
import glob
import shutil
import platform

# ── Colors ───────────────────────────────────────────────────────────────────

if sys.stdout.isatty() and platform.system() != "Windows":
    BLUE = "\033[0;34m"
    GREEN = "\033[0;32m"
    YELLOW = "\033[1;33m"
    RED = "\033[0;31m"
    BOLD = "\033[1m"
    NC = "\033[0m"
else:
    BLUE = GREEN = YELLOW = RED = BOLD = NC = ""


def info(msg: str) -> None:
    print(f"{BLUE}[*]{NC} {msg}")


def ok(msg: str) -> None:
    print(f"{GREEN}[+]{NC} {msg}")


def warn(msg: str) -> None:
    print(f"{YELLOW}[!]{NC} {msg}")


def fail(msg: str) -> None:
    print(f"{RED}[x]{NC} {msg}")
    sys.exit(1)


# ── Locate the Cartridge source ─────────────────────────────────────────────

def find_cartridge_source() -> str:
    """Find the Cartridge repo root.

    Works both when running as a script (from the repo) and as a PyInstaller
    bundle (data is extracted to a temp dir via sys._MEIPASS).
    """
    # PyInstaller bundle: data is bundled alongside the executable
    if getattr(sys, "_MEIPASS", None):
        base = sys._MEIPASS
        if (
            os.path.isfile(os.path.join(base, "pyproject.toml"))
            and os.path.isdir(os.path.join(base, "src"))
            and os.path.isdir(os.path.join(base, "cartridges"))
        ):
            return base

    # Normal script execution: look relative to this file
    script_dir = os.path.dirname(os.path.abspath(__file__))
    for candidate in [script_dir, os.path.dirname(script_dir)]:
        if (
            os.path.isfile(os.path.join(candidate, "pyproject.toml"))
            and os.path.isdir(os.path.join(candidate, "src"))
            and os.path.isdir(os.path.join(candidate, "cartridges"))
        ):
            return candidate
    fail("Cannot find Cartridge source. Run this script from the Cartridge repo directory.")
    return ""  # unreachable


# ── Detect mounted device ───────────────────────────────────────────────────

def find_mount_candidates() -> list[str]:
    """Return a list of mount point search roots based on the OS."""
    system = platform.system()
    if system == "Darwin":
        return glob.glob("/Volumes/*")
    elif system == "Linux":
        candidates = []
        for base in ["/media", "/run/media"]:
            if os.path.isdir(base):
                # /media/<user>/<volume> or /run/media/<user>/<volume>
                for user_dir in glob.glob(os.path.join(base, "*")):
                    if os.path.isdir(user_dir):
                        candidates.extend(glob.glob(os.path.join(user_dir, "*")))
        for entry in glob.glob("/mnt/*"):
            if os.path.isdir(entry):
                candidates.append(entry)
        return candidates
    elif system == "Windows":
        # Check drive letters D: through Z:
        candidates = []
        for letter in "DEFGHIJKLMNOPQRSTUVWXYZ":
            drive = f"{letter}:\\"
            if os.path.isdir(drive):
                candidates.append(drive)
        return candidates
    return []


def looks_like_handheld(mount_path: str) -> bool:
    """Check if a mounted volume looks like a handheld device's roms partition."""
    roms_dir = os.path.join(mount_path, "roms")
    if os.path.isdir(roms_dir):
        return True
    # Some devices mount the roms partition directly
    if os.path.isdir(os.path.join(mount_path, "tools")) and (
        os.path.isdir(os.path.join(mount_path, "ports"))
        or os.path.isdir(os.path.join(mount_path, "bios"))
    ):
        return True
    return False


def get_roms_dir(mount_path: str) -> str:
    """Get the roms directory path for a detected device."""
    roms_dir = os.path.join(mount_path, "roms")
    if os.path.isdir(roms_dir):
        return roms_dir
    # If we're directly in the roms partition
    if os.path.isdir(os.path.join(mount_path, "tools")):
        return mount_path
    return roms_dir


def detect_devices() -> list[tuple[str, str]]:
    """Detect connected handheld devices. Returns list of (mount_path, roms_dir)."""
    devices = []
    for mount in find_mount_candidates():
        if looks_like_handheld(mount):
            devices.append((mount, get_roms_dir(mount)))
    return devices


# ── Install to device ────────────────────────────────────────────────────────

COPY_DIRS = ["src", "cartridges"]
COPY_FILES = ["pyproject.toml", "install.sh", "registry.json", "README.md", "design_catridge.md"]
EXCLUDE_PATTERNS = {"__pycache__", ".git", ".venv", "venv", ".mypy_cache", ".pytest_cache", "*.pyc"}


def should_exclude(name: str) -> bool:
    """Check if a file/directory should be excluded from the copy."""
    # macOS resource fork files
    if name.startswith("._"):
        return True
    if name == ".DS_Store":
        return True
    if name in EXCLUDE_PATTERNS:
        return True
    for pattern in EXCLUDE_PATTERNS:
        if pattern.startswith("*") and name.endswith(pattern[1:]):
            return True
    return False


def copy_tree(src: str, dst: str) -> int:
    """Recursively copy a directory, skipping excluded patterns. Returns file count."""
    count = 0
    os.makedirs(dst, exist_ok=True)
    for entry in os.listdir(src):
        if should_exclude(entry):
            continue
        src_path = os.path.join(src, entry)
        dst_path = os.path.join(dst, entry)
        if os.path.isdir(src_path):
            count += copy_tree(src_path, dst_path)
        else:
            shutil.copy2(src_path, dst_path)
            count += 1
    return count


def count_source_files(source: str) -> int:
    """Count files that would be copied (for dry run display)."""
    count = 0
    for d in COPY_DIRS:
        src_path = os.path.join(source, d)
        if os.path.isdir(src_path):
            for root, dirs, files in os.walk(src_path):
                dirs[:] = [d for d in dirs if not should_exclude(d)]
                count += sum(1 for f in files if not should_exclude(f))
    for f in COPY_FILES:
        if os.path.isfile(os.path.join(source, f)):
            count += 1
    return count


def dry_run(source: str, roms_dir: str) -> None:
    """Show what would be done without writing anything."""
    dest = os.path.join(roms_dir, "Cartridge")
    tools_dir = os.path.join(roms_dir, "tools")

    print()
    info("DRY RUN — no files will be written")
    print()

    file_count = count_source_files(source)

    print(f"  Source:      {source}")
    print(f"  Device:      {roms_dir}")
    print()
    print(f"  Would create: {dest}/")
    print(f"    Directories:")
    for d in COPY_DIRS:
        src_path = os.path.join(source, d)
        if os.path.isdir(src_path):
            print(f"      {dest}/{d}/")
    print(f"    Files:")
    for f in COPY_FILES:
        if os.path.isfile(os.path.join(source, f)):
            print(f"      {dest}/{f}")
    print(f"    Total: {file_count} files")
    print()

    if os.path.isdir(tools_dir):
        print(f"  Would create: {tools_dir}/Cartridge.sh")
        print(f"    (self-bootstrapping launcher — installs on first run, launches after)")
    else:
        warn(f"  Tools directory not found at {tools_dir}")

    # Check what already exists
    print()
    if os.path.exists(dest):
        warn(f"  {dest} already exists — would be overwritten")
    if os.path.isfile(os.path.join(tools_dir, "Cartridge.sh")):
        warn(f"  {tools_dir}/Cartridge.sh already exists — would be overwritten")

    # Show existing contents that would NOT be touched
    print()
    info("Existing contents that will NOT be modified:")
    existing = sorted(os.listdir(roms_dir))
    for entry in existing:
        if entry in ("Cartridge",):
            continue
        print(f"    {entry}/")

    print()
    ok("Dry run complete. Run without --dry-run to install.")


def install_to_device(source: str, roms_dir: str) -> None:
    """Copy Cartridge files to the device and set up the installer."""
    dest = os.path.join(roms_dir, "Cartridge")
    tools_dir = os.path.join(roms_dir, "tools")

    # Copy project files
    info(f"Copying Cartridge to {dest}...")
    total_files = 0
    for d in COPY_DIRS:
        src_path = os.path.join(source, d)
        if os.path.isdir(src_path):
            total_files += copy_tree(src_path, os.path.join(dest, d))

    for f in COPY_FILES:
        src_path = os.path.join(source, f)
        if os.path.isfile(src_path):
            shutil.copy2(src_path, os.path.join(dest, f))
            total_files += 1

    ok(f"Copied {total_files} files to {dest}")

    # Place launcher in Tools menu
    # On first run this installs everything, then restarts ES with Cartridge
    # in the main carousel. On subsequent runs it just launches Cartridge.
    if os.path.isdir(tools_dir):
        launcher_dest = os.path.join(tools_dir, "Cartridge.sh")
        install_sh = os.path.join(source, "install.sh")
        shutil.copy2(install_sh, launcher_dest)
        os.chmod(launcher_dest, 0o755)
        ok(f"Launcher placed at {launcher_dest}")
    else:
        warn(f"Tools directory not found at {tools_dir}")
        warn("You may need to manually copy install.sh to your device's tools folder.")


# ── Main ─────────────────────────────────────────────────────────────────────

def main() -> None:
    is_dry_run = "--dry-run" in sys.argv

    print()
    print(f"{BOLD}  Cartridge Device Installer{NC}")
    print(f"  ==========================")
    print()

    source = find_cartridge_source()
    ok(f"Cartridge source: {source}")

    info("Scanning for connected devices...")
    devices = detect_devices()

    if not devices:
        print()
        warn("No handheld device detected.")
        print()
        print("Make sure your device is:")
        print("  1. SD card inserted into a card reader connected to this computer")
        print("  2. Or device connected via USB in Mass Storage mode")
        print("  3. Mounted as a drive on this computer")
        print()
        system = platform.system()
        if system == "Darwin":
            print("On macOS, the device should appear in Finder and under /Volumes/")
        elif system == "Linux":
            print("On Linux, the device should appear in your file manager and under /media/ or /mnt/")
        elif system == "Windows":
            print("On Windows, the device should appear as a drive letter in File Explorer")
        print()
        sys.exit(1)

    if len(devices) == 1:
        mount_path, roms_dir = devices[0]
        ok(f"Found device at: {mount_path}")
    else:
        print()
        print("Multiple devices found:")
        for i, (mount, _) in enumerate(devices, 1):
            print(f"  {i}. {mount}")
        print()
        while True:
            try:
                choice = input(f"Select device [1-{len(devices)}]: ").strip()
                idx = int(choice) - 1
                if 0 <= idx < len(devices):
                    mount_path, roms_dir = devices[idx]
                    break
            except (ValueError, EOFError):
                pass
            print("Invalid selection, try again.")

    if is_dry_run:
        dry_run(source, roms_dir)
        return

    # Confirm
    print()
    info(f"This will install Cartridge to: {roms_dir}/Cartridge/")
    print()
    try:
        answer = input("Proceed? [Y/n] ").strip().lower()
    except (EOFError, KeyboardInterrupt):
        print()
        sys.exit(1)

    if answer and answer not in ("y", "yes"):
        print("Cancelled.")
        sys.exit(0)

    print()
    install_to_device(source, roms_dir)

    # Done
    print()
    print(f"{BOLD}Done! Now:{NC}")
    print()
    print("  1. Eject the SD card and put it back in your device")
    print("  2. Boot the device and connect to WiFi")
    print("  3. Go to Tools > Cartridge")
    print("  4. First launch installs dependencies (~2-5 min), then")
    print("     EmulationStation restarts and Cartridge appears in the")
    print("     main carousel permanently. No more steps needed.")
    print()


if __name__ == "__main__":
    main()
