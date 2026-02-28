"""WiFi status detection for handheld devices."""

from __future__ import annotations

import platform
import subprocess
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class WifiStatus:
    """Current WiFi connection state."""

    connected: bool = False
    ssid: str = ""
    signal_strength: int = -1  # 0-100 percent, -1 = unknown


def get_wifi_status() -> WifiStatus:
    """Query WiFi status. Works on Linux handhelds, placeholder on macOS."""
    system = platform.system()
    if system == "Linux":
        return _get_linux_wifi()
    elif system == "Darwin":
        return WifiStatus(connected=True, ssid="WiFi", signal_strength=75)
    return WifiStatus()


def _find_wireless_interface() -> str | None:
    """Find the first wireless network interface via sysfs."""
    wireless_dir = Path("/sys/class/net")
    if not wireless_dir.exists():
        return None
    for iface in wireless_dir.iterdir():
        if (iface / "wireless").exists():
            return iface.name
    # Fallback to common names
    for name in ("wlan0", "wlan1", "wlp2s0"):
        if (wireless_dir / name).exists():
            return name
    return None


def _get_linux_wifi() -> WifiStatus:
    """Read WiFi status from sysfs and iwconfig."""
    status = WifiStatus()

    iface = _find_wireless_interface()
    if not iface:
        return status

    # Check interface is up via sysfs
    operstate = Path(f"/sys/class/net/{iface}/operstate")
    if operstate.exists():
        state = operstate.read_text().strip()
        status.connected = state == "up"

    if not status.connected:
        return status

    # Get SSID and signal strength via iwconfig
    try:
        result = subprocess.run(
            ["iwconfig", iface],
            capture_output=True,
            text=True,
            timeout=2,
        )
        for line in result.stdout.split("\n"):
            if "ESSID:" in line:
                ssid_part = line.split("ESSID:")[1].strip().strip('"')
                if ssid_part and ssid_part != "off/any":
                    status.ssid = ssid_part
            if "Signal level=" in line:
                sig_part = line.split("Signal level=")[1].split(" ")[0]
                dbm = int(sig_part)
                # Convert dBm to percentage: -30=100%, -90=0%
                status.signal_strength = max(0, min(100, 2 * (dbm + 100)))
    except (FileNotFoundError, subprocess.TimeoutExpired, ValueError, IndexError):
        pass

    return status
