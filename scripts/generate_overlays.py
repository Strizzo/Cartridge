#!/usr/bin/env python3
"""Generate overlay PNG textures for the Cartridge cyberdeck atmosphere."""

import struct
import zlib
import os
import math
import random

OUTPUT_DIR = os.path.join(os.path.dirname(__file__), "..", "assets", "overlays")
WIDTH = 720
HEIGHT = 720
GRID_HEIGHT = 1440  # double-height for seamless scroll tiling


def make_png(width, height, rows):
    """Create a PNG file from raw RGBA row data."""
    def chunk(chunk_type, data):
        c = chunk_type + data
        crc = struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)
        return struct.pack(">I", len(data)) + c + crc

    header = b"\x89PNG\r\n\x1a\n"
    ihdr = chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))

    raw = b""
    for row in rows:
        raw += b"\x00" + row  # filter byte 0 (None) per row

    idat = chunk(b"IDAT", zlib.compress(raw, 9))
    iend = chunk(b"IEND", b"")
    return header + ihdr + idat + iend


def generate_scanlines():
    """CRT scanline texture: every-other-row darkening at ~12% opacity."""
    rows = []
    for y in range(HEIGHT):
        if y % 2 == 0:
            # Dark scanline row
            row = bytes([0, 0, 0, 30]) * WIDTH  # RGBA(0,0,0,30) ~ 12%
        else:
            # Transparent row
            row = bytes([0, 0, 0, 0]) * WIDTH
        rows.append(row)
    return rows


def generate_vignette():
    """Radial edge vignette: transparent center, dark edges."""
    cx, cy = WIDTH / 2, HEIGHT / 2
    max_dist = math.sqrt(cx * cx + cy * cy)
    rows = []
    for y in range(HEIGHT):
        row = bytearray()
        for x in range(WIDTH):
            dx = (x - cx) / cx
            dy = (y - cy) / cy
            dist = math.sqrt(dx * dx + dy * dy)
            # Start darkening at 0.6, full dark at 1.2
            t = max(0.0, (dist - 0.6) / 0.6)
            t = min(1.0, t)
            # Ease in
            t = t * t
            alpha = int(t * 120)
            row.extend([0, 0, 0, alpha])
        rows.append(bytes(row))
    return rows


def generate_grid():
    """Circuit grid pattern: 32px spacing, intersection dots, L-traces, fade-out."""
    spacing = 32
    line_color = (40, 55, 90, 25)
    dot_color = (70, 100, 140, 40)
    trace_color = (55, 75, 110, 35)

    # Pre-generate some random L-shaped traces
    random.seed(42)
    traces = set()
    for _ in range(30):
        gx = random.randint(1, WIDTH // spacing - 2)
        gy = random.randint(1, GRID_HEIGHT // spacing - 2)
        # L-shape: horizontal then vertical or vice versa
        length_h = random.randint(1, 3)
        length_v = random.randint(1, 3)
        for dx in range(length_h + 1):
            traces.add((gx + dx, gy))
        for dy in range(length_v + 1):
            traces.add((gx + length_h, gy + dy))

    rows = []
    for y in range(GRID_HEIGHT):
        row = bytearray()
        # Fade out in bottom third
        fade = 1.0
        if y > GRID_HEIGHT * 2 // 3:
            fade = 1.0 - (y - GRID_HEIGHT * 2 // 3) / (GRID_HEIGHT // 3)
            fade = max(0.0, fade)

        for x in range(WIDTH):
            on_grid_h = (y % spacing == 0)
            on_grid_v = (x % spacing == 0)
            is_intersection = on_grid_h and on_grid_v

            # Check if on a trace segment
            gx, gy = x // spacing, y // spacing
            on_trace = False
            if on_grid_h and (gx, gy) in traces:
                on_trace = True
            if on_grid_v and (gx, gy) in traces:
                on_trace = True

            if is_intersection:
                r, g, b, a = dot_color
                a = int(a * fade)
                row.extend([r, g, b, a])
            elif on_trace and (on_grid_h or on_grid_v):
                r, g, b, a = trace_color
                a = int(a * fade)
                row.extend([r, g, b, a])
            elif on_grid_h or on_grid_v:
                r, g, b, a = line_color
                a = int(a * fade)
                row.extend([r, g, b, a])
            else:
                row.extend([0, 0, 0, 0])

        rows.append(bytes(row))
    return rows


def main():
    os.makedirs(OUTPUT_DIR, exist_ok=True)

    print("Generating scanlines.png...")
    scanlines = generate_scanlines()
    data = make_png(WIDTH, HEIGHT, scanlines)
    path = os.path.join(OUTPUT_DIR, "scanlines.png")
    with open(path, "wb") as f:
        f.write(data)
    print(f"  -> {path} ({len(data)} bytes)")

    print("Generating vignette.png...")
    vignette = generate_vignette()
    data = make_png(WIDTH, HEIGHT, vignette)
    path = os.path.join(OUTPUT_DIR, "vignette.png")
    with open(path, "wb") as f:
        f.write(data)
    print(f"  -> {path} ({len(data)} bytes)")

    print("Generating grid_bg.png...")
    grid = generate_grid()
    data = make_png(WIDTH, GRID_HEIGHT, grid)
    path = os.path.join(OUTPUT_DIR, "grid_bg.png")
    with open(path, "wb") as f:
        f.write(data)
    print(f"  -> {path} ({len(data)} bytes)")

    print("Done!")


if __name__ == "__main__":
    main()
