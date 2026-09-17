#!/usr/bin/env python3
"""Render the Neo-Tokyo cartridge icons.

Each icon is a handful of filled shapes on a 48-unit grid in three tones:
W (warm white body), R (one red accent), K (black cutouts). Two PNGs are
produced per cartridge:

  icon.png          for the dark tile
  icon_focused.png  for the red (focused) tile: red -> black so the icon
                    stays two-tone on red

Shapes tagged RW/KW flip to warm white on the focused tile instead (they
sit on top of a black or red area that itself turns black).

Usage:
  python3 scripts/render_icons.py            # writes SVG sources + PNGs
  python3 scripts/render_icons.py --svg-only # just refresh assets/icons/src

Rendering uses headless Chrome (macOS path below, or CHROME=...) and Pillow
to crop the sheet; both are dev-machine-only dependencies.
"""

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

W, R, K = "#F2EDE4", "#E23D2C", "#0C0C0E"
RW = "#E23D2D"  # red normally, warm white when focused
KW = "#0C0C0F"  # black normally, warm white when focused

ROOT = Path(__file__).resolve().parent.parent
SRC_DIR = ROOT / "assets" / "icons" / "src"
OUT_DIR = ROOT / "assets" / "icons"
CARTS = ROOT / "lua_cartridges"
SIZE = 128

ICONS = {
    "vibeboy": [
        f'<rect x="3" y="11" width="42" height="28" rx="5" fill="{W}"/>',
        f'<rect x="15" y="15" width="18" height="20" fill="{K}"/>',
        f'<path d="M19 20l4 4-4 4" fill="none" stroke="{RW}" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>',
        f'<rect x="24" y="28" width="6" height="2.6" fill="{RW}"/>',
        f'<rect x="5.5" y="23.5" width="8" height="3" fill="{K}"/>',
        f'<rect x="8" y="21" width="3" height="8" fill="{K}"/>',
        f'<circle cx="40" cy="22" r="2.2" fill="{K}"/>',
        f'<circle cx="36.5" cy="28" r="2.2" fill="{R}"/>',
    ],
    "hacker_news": [
        f'<path d="M6 5h11l7 12 7-12h11L27 27v16h-6V27z" fill="{W}"/>',
        f'<circle cx="24" cy="24" r="5.5" fill="{R}"/>',
    ],
    "stock_market": [
        f'<rect x="10" y="18" width="2.5" height="24" fill="{W}"/>',
        f'<rect x="7" y="24" width="8.5" height="14" fill="{W}"/>',
        f'<rect x="23" y="11" width="2.5" height="26" fill="{W}"/>',
        f'<rect x="20" y="16" width="8.5" height="16" fill="{W}"/>',
        f'<rect x="36" y="3" width="2.5" height="28" fill="{R}"/>',
        f'<rect x="33" y="7" width="8.5" height="18" fill="{R}"/>',
        f'<rect x="4" y="42" width="40" height="3" fill="{W}"/>',
    ],
    "weather": [
        f'<g stroke="{R}" stroke-width="3.5" stroke-linecap="round"><path d="M31 2v4M31 28v4M16 17h4M42 17h4M20.4 6.4l2.8 2.8M38.8 24.8l2.8 2.8M20.4 27.6l2.8-2.8M38.8 9.2l2.8-2.8"/></g>',
        f'<circle cx="31" cy="17" r="9" fill="{R}"/>',
        f'<circle cx="16" cy="34" r="8" fill="{W}"/>',
        f'<circle cx="27" cy="30" r="10.5" fill="{W}"/>',
        f'<circle cx="37" cy="36" r="7" fill="{W}"/>',
        f'<rect x="16" y="34" width="21" height="9" fill="{W}"/>',
    ],
    "calculator": [
        f'<rect x="9" y="3" width="30" height="42" rx="3" fill="{W}"/>',
        f'<rect x="13" y="7" width="22" height="10" fill="{K}"/>',
        f'<rect x="13" y="21" width="6" height="6" fill="{K}"/>',
        f'<rect x="21" y="21" width="6" height="6" fill="{K}"/>',
        f'<rect x="29" y="21" width="6" height="6" fill="{K}"/>',
        f'<rect x="13" y="29" width="6" height="6" fill="{K}"/>',
        f'<rect x="21" y="29" width="6" height="6" fill="{K}"/>',
        f'<rect x="13" y="37" width="6" height="5" fill="{K}"/>',
        f'<rect x="21" y="37" width="6" height="5" fill="{K}"/>',
        f'<rect x="29" y="29" width="6" height="13" fill="{R}"/>',
    ],
    "pomodoro": [
        f'<circle cx="24" cy="28" r="17" fill="{R}"/>',
        f'<path d="M24 13c-2.5-6.5-9-8-14-6 3.5 1.5 6 4.5 7 8z" fill="{W}"/>',
        f'<path d="M24 13c2.5-6.5 9-8 14-6-3.5 1.5-6 4.5-7 8z" fill="{W}"/>',
        f'<rect x="22.4" y="5" width="3.2" height="9" rx="1" fill="{W}"/>',
        f'<path d="M24 28V17" stroke="{KW}" stroke-width="3" stroke-linecap="round"/>',
        f'<path d="M24 28l7 4" stroke="{KW}" stroke-width="3" stroke-linecap="round"/>',
    ],
    "system_monitor": [
        f'<rect x="4" y="7" width="40" height="28" rx="2" fill="{W}"/>',
        f'<rect x="19" y="37" width="10" height="4" fill="{W}"/>',
        f'<rect x="13" y="41" width="22" height="3.5" fill="{W}"/>',
        f'<path d="M8 23h8l4-9 5 17 5-12 3 4h7" fill="none" stroke="{R}" stroke-width="3.5" stroke-linejoin="round" stroke-linecap="round"/>',
    ],
    "network_tool": [
        f'<path d="M24 24V4a20 20 0 0 1 20 20z" fill="{R}"/>',
        f'<circle cx="24" cy="24" r="20" fill="none" stroke="{W}" stroke-width="3"/>',
        f'<circle cx="24" cy="24" r="11.5" fill="none" stroke="{W}" stroke-width="3"/>',
        f'<circle cx="24" cy="24" r="4" fill="{W}"/>',
        f'<circle cx="13" cy="32" r="3.2" fill="{W}"/>',
    ],
    "todo": [
        f'<rect x="9" y="7" width="30" height="38" rx="3" fill="{W}"/>',
        f'<rect x="17" y="3" width="14" height="9" rx="2" fill="{K}"/>',
        f'<rect x="20" y="5.5" width="8" height="4" fill="{W}"/>',
        f'<path d="M15 27l6.5 6.5L34 20" fill="none" stroke="{R}" stroke-width="5" stroke-linecap="round" stroke-linejoin="round"/>',
    ],
    "ai_papers": [
        f'<rect x="10" y="4" width="28" height="40" rx="2" fill="{W}"/>',
        f'<rect x="15" y="11" width="18" height="3" fill="{K}"/>',
        f'<rect x="15" y="18" width="18" height="3" fill="{K}"/>',
        f'<rect x="15" y="25" width="11" height="3" fill="{K}"/>',
        f'<rect x="10" y="35" width="28" height="9" fill="{R}"/>',
    ],
    "hello_world": [
        f'<path d="M5 7h38v25H21l-9 9v-9H5z" fill="{W}"/>',
        f'<circle cx="24" cy="19.5" r="4.5" fill="{R}"/>',
    ],
}


def shapes_for(name, focused):
    s = "".join(ICONS[name])
    if focused:
        return s.replace(RW, W).replace(KW, W).replace(R, K)
    return s.replace(RW, R).replace(KW, K)


def svg_for(name, focused, size):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" '
        f'viewBox="0 0 48 48">{shapes_for(name, focused)}</svg>'
    )


def write_sources():
    SRC_DIR.mkdir(parents=True, exist_ok=True)
    for name in ICONS:
        (SRC_DIR / f"{name}.svg").write_text(svg_for(name, False, 48) + "\n")
        (SRC_DIR / f"{name}_focused.svg").write_text(svg_for(name, True, 48) + "\n")


def find_chrome():
    env = os.environ.get("CHROME")
    if env:
        return env
    for p in (
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
    ):
        if os.path.exists(p):
            return p
    return None


def render_pngs():
    from PIL import Image

    chrome = find_chrome()
    if not chrome:
        sys.exit("No Chrome/Chromium found; set CHROME=/path/to/chrome")

    names = list(ICONS)
    cells = [(n, f) for n in names for f in (False, True)]
    cols = 5
    rows = (len(cells) + cols - 1) // cols
    html = ["<!doctype html><meta charset='utf-8'><body style='margin:0;background:transparent'>"]
    html.append(
        f"<div style='display:grid;grid-template-columns:repeat({cols},{SIZE}px);width:{cols*SIZE}px'>"
    )
    for name, focused in cells:
        html.append(f"<div style='width:{SIZE}px;height:{SIZE}px'>{svg_for(name, focused, SIZE)}</div>")
    html.append("</div></body>")

    with tempfile.TemporaryDirectory() as tmp:
        page = Path(tmp) / "sheet.html"
        page.write_text("".join(html))
        shot = Path(tmp) / "sheet.png"
        subprocess.run(
            [
                chrome,
                "--headless=new",
                "--disable-gpu",
                "--hide-scrollbars",
                "--default-background-color=00000000",
                f"--window-size={cols*SIZE},{rows*SIZE}",
                f"--screenshot={shot}",
                page.as_uri(),
            ],
            check=True,
            capture_output=True,
        )
        sheet = Image.open(shot).convert("RGBA")

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for i, (name, focused) in enumerate(cells):
        x, y = (i % cols) * SIZE, (i // cols) * SIZE
        img = sheet.crop((x, y, x + SIZE, y + SIZE))
        suffix = "_focused" if focused else ""
        img.save(OUT_DIR / f"{name}{suffix}.png")
        cart_dir = CARTS / name
        if cart_dir.is_dir():
            img.save(cart_dir / f"icon{suffix}.png")
    print(f"rendered {len(cells)} icons into {OUT_DIR} and lua_cartridges/*/")


if __name__ == "__main__":
    write_sources()
    if "--svg-only" not in sys.argv:
        render_pngs()
