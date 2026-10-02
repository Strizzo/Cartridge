#!/usr/bin/env python3
"""Generate the 720px CartridgeOS boot bitmap and selector artwork."""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / 'assets'
SIZE = 720
SCALE = 3
INK = '#171717'
PAPER = '#F5F1E8'
RED = '#EF4436'
MUTED = '#807B75'


def font(name, size):
    return ImageFont.truetype(str(ASSETS / 'fonts' / name), size * SCALE)


def box(draw, xy, fill=None, outline=None, width=1):
    draw.rectangle(tuple(int(v * SCALE) for v in xy), fill=fill, outline=outline,
                   width=width * SCALE)


def polygon(draw, points, fill):
    draw.polygon([(int(x * SCALE), int(y * SCALE)) for x, y in points], fill=fill)


def centered(draw, text, y, face, color):
    bounds = draw.textbbox((0, 0), text, font=face)
    width = bounds[2] - bounds[0]
    draw.text(((SIZE * SCALE - width) // 2 - bounds[0], y * SCALE - bounds[1]),
              text, font=face, fill=color)


def build():
    image = Image.new('RGB', (SIZE * SCALE, SIZE * SCALE), INK)
    draw = ImageDraw.Draw(image)

    # The framing, solid hazard bar and display type match the Neo launcher.
    box(draw, (30, 30, 690, 690), outline='#494642', width=2)
    box(draw, (30, 30, 43, 690), fill=RED)
    box(draw, (66, 72, 653, 78), fill=RED)
    draw.text((67 * SCALE, 43 * SCALE), 'CARTRIDGE / SYSTEM 01',
              font=font('JetBrainsMono-Bold.ttf', 14), fill=PAPER)
    draw.text((585 * SCALE, 44 * SCALE), 'BOOT',
              font=font('JetBrainsMono-Bold.ttf', 14), fill=RED)

    # A custom cartridge mark, drawn from shapes rather than a font glyph.
    polygon(draw, [(205, 149), (515, 149), (550, 184), (550, 435),
                   (524, 461), (196, 461), (170, 435), (170, 184)], RED)
    polygon(draw, [(218, 166), (502, 166), (533, 197), (533, 426),
                   (515, 444), (205, 444), (187, 426), (187, 197)], INK)
    box(draw, (213, 186, 507, 208), fill=PAPER)
    box(draw, (213, 221, 507, 234), fill=RED)
    box(draw, (241, 258, 479, 397), fill=PAPER)
    box(draw, (259, 276, 461, 379), fill=INK)
    box(draw, (277, 292, 405, 312), fill=PAPER)
    box(draw, (277, 292, 299, 363), fill=PAPER)
    box(draw, (277, 343, 405, 363), fill=PAPER)
    box(draw, (426, 292, 444, 363), fill=RED)
    box(draw, (244, 444, 476, 486), fill=PAPER)
    for x in range(258, 466, 33):
        box(draw, (x, 445, x + 16, 486), fill=INK)

    centered(draw, 'CARTRIDGE', 503, font('BebasNeue-Regular.ttf', 90), PAPER)
    centered(draw, 'OPERATING SYSTEM', 615, font('JetBrainsMono-Bold.ttf', 19), RED)
    box(draw, (66, 667, 654, 673), fill=RED)

    image = image.resize((SIZE, SIZE), Image.Resampling.LANCZOS)
    image.save(ASSETS / 'logo.bmp', format='BMP')
    image.save(ASSETS / 'boot_logo.png', format='PNG', optimize=True)


if __name__ == '__main__':
    build()
