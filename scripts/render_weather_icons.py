#!/usr/bin/env python3
"""Original weather illustrations extending the launcher's warm-white/red icons.

ImageMagick (`magick`) is needed only to regenerate PNGs. All artwork uses filled
vector shapes: committed SVG sources plus 192px PNGs, cached by the device runtime.
"""
import argparse
import math
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / 'assets/icons/src/weather_conditions'
OUTPUT = ROOT / 'lua_cartridges/weather/assets/conditions'
W, R = '#F2EDE4', '#E23D2C'
CLOUD = f'<path fill="{W}" d="M13 34a8 8 0 1 1 1-16 11 11 0 0 1 21-3 9.5 9.5 0 0 1 1 19z"/>'
def line(x1,y1,x2,y2,width,color):
    length=math.hypot(x2-x1,y2-y1)
    dx,dy=-(y2-y1)/length*width/2,(x2-x1)/length*width/2
    points=' '.join(f'{x:.3f},{y:.3f}' for x,y in [(x1+dx,y1+dy),(x2+dx,y2+dy),(x2-dx,y2-dy),(x1-dx,y1-dy)])
    return f'<polygon points="{points}" fill="{color}"/>'

def sun(cx,cy,r):
    shapes=f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{R}"/>'
    for angle in range(0,360,45):
        dx,dy=math.sin(math.radians(angle)),math.cos(math.radians(angle))
        shapes+=line(cx+dx*(r+4),cy+dy*(r+4),cx+dx*(r+8),cy+dy*(r+8),2.8,R)
    return shapes

SUN = sun(24,24,11)
SMALL_SUN = sun(32,14,8)
RAIN = ''.join(f'<path d="M{x} 37l-3 8h3l3-8z" fill="{R}"/>' for x in [14,25,36])
SNOW = ''.join(line(x-4,37,x+4,45,1.8,R)+line(x+4,37,x-4,45,1.8,R)+line(x-5,41,x+5,41,1.8,R) for x in [12,32])
ICONS = {
    'clear': SUN,
    'partly_cloudy': SMALL_SUN + f'<g transform="translate(0 8)">{CLOUD}</g>',
    'cloudy': f'<g transform="translate(0 5)">{CLOUD}</g>',
    'fog': f'<g transform="translate(5 -1) scale(.8)">{CLOUD}</g><rect x="8" y="31" width="32" height="3" rx="1.5" fill="{W}"/><rect x="4" y="37" width="29" height="3" rx="1.5" fill="{W}"/><rect x="16" y="43" width="28" height="3" rx="1.5" fill="{W}"/><rect x="5" y="43" width="5" height="3" rx="1.5" fill="{R}"/>',
    'drizzle': CLOUD + ''.join(f'<circle cx="{x}" cy="40" r="1.8" fill="{R}"/>' for x in [14,25,36]),
    'rain': CLOUD + RAIN,
    'showers': SMALL_SUN + CLOUD + RAIN,
    'snow': CLOUD + SNOW,
    'storm': CLOUD + f'<path d="M26 23l-9 15h8l-4 10 15-17h-9l5-8z" fill="{R}"/>',
    'unknown': CLOUD + f'<rect x="16" y="39" width="18" height="4" rx="2" fill="{R}"/>',
}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--svg-only', action='store_true')
    args = parser.parse_args()
    renderer = shutil.which('magick')
    if not args.svg_only and not renderer:
        parser.error('ImageMagick is needed for PNGs; use --svg-only for sources')
    SOURCE.mkdir(parents=True, exist_ok=True)
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for name, shapes in ICONS.items():
        source = SOURCE / (name + '.svg')
        source.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="192" height="192" viewBox="0 0 48 48">{shapes}</svg>\n')
        if not args.svg_only:
            subprocess.run([renderer,'-background','none',str(source),str(OUTPUT/(name+'.png'))],check=True,timeout=15)
    print(f'Generated {len(ICONS)} weather illustrations')

if __name__ == '__main__':
    main()
