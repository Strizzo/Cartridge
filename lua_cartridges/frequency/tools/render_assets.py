#!/usr/bin/env python3
"""Bake Frequency's map and original radio-dial icon; Pillow is build-time only.
Usage: python3 tools/render_assets.py /path/to/ne_110m_admin_0_countries.geojson
Source: Natural Earth v5.1.2, public domain. See assets/attribution.json.
"""
from pathlib import Path
import hashlib
import json
import math
import sys
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parents[1]
source = Path(sys.argv[1])
data = json.loads(source.read_text())
scale = 3
W, H = 672, 252
sea, land, edge, grid = '#192F38', '#466063', '#69807B', '#29424A'
image = Image.new('RGB', (W*scale, H*scale), sea)
draw = ImageDraw.Draw(image)
def xy(lon, lat): return ((lon+180)/360*W*scale, (90-lat)/180*H*scale)
for lon in range(-150, 180, 30):
    draw.line([xy(lon,90),xy(lon,-90)], fill=grid, width=scale)
for lat in range(-60, 90, 30):
    draw.line([xy(-180,lat),xy(180,lat)], fill=grid, width=scale)
for feature in data['features']:
    g=feature['geometry']
    polygons = [g['coordinates']] if g['type']=='Polygon' else g['coordinates']
    for polygon in polygons:
        for n, ring in enumerate(polygon):
            points = [xy(*p[:2]) for p in ring]
            draw.polygon(points,fill=land if n==0 else sea)
            draw.line(points,fill=edge,width=1)
# Small cartographic ticks, kept out of the runtime draw loop.
for lon in range(-180,181,10):
    x,_=xy(lon,0)
    draw.line([(x,0),(x,(5 if lon%30==0 else 2)*scale)],fill='#8BA29B',width=scale)
image.resize((W,H),Image.Resampling.LANCZOS).save(root/'assets/world.png')
# Original geometry: a warm radio dial with a tilted orange tuning needle.
s=4
icon=Image.new('RGB',(256*s,256*s),'#EAE5D8');d=ImageDraw.Draw(icon)
def line(points,color,width):d.line([(int(x*s),int(y*s)) for x,y in points],fill=color,width=width*s)
def circle(box,color):d.ellipse(tuple(int(v*s) for v in box),fill=color)
circle((24,24,232,232),'#192F38')
circle((34,34,222,222),'#29424A')
circle((48,48,208,208),'#192F38')
for deg in range(135,406,15):
    a=math.radians(deg);r=83;inner=71 if deg%45==0 else 76
    line([(128+inner*math.cos(a),128+inner*math.sin(a)),(128+r*math.cos(a),128+r*math.sin(a))],'#D8DDCE',3)
line([(111,155),(167,82)],'#F48652',7)
circle((116,116,140,140),'#F48652')
line([(94,184),(162,184)],'#D8DDCE',4)
icon=icon.resize((256,256),Image.Resampling.LANCZOS)
icon.save(root/'assets/icon.png')
icon.save(root/'icon.png')
for name,size,color in [('pin',7,'#9AB2AC'),('pin-selected',19,'#EFEBDF')]:
    marker=Image.new('RGBA',(size*4,size*4),(0,0,0,0));pen=ImageDraw.Draw(marker)
    pen.ellipse((4,4,size*4-4,size*4-4),fill=color)
    if name=='pin-selected':
        pen.ellipse((16,16,size*4-16,size*4-16),fill='#C14621')
        pen.ellipse((29,29,size*4-29,size*4-29),fill='#F9F7EF')
    marker.resize((size,size),Image.Resampling.LANCZOS).save(root/('assets/'+name+'.png'))
metadata={
 'map': {'title':'Natural Earth 1:110m admin 0 countries','version':'5.1.2','license':'Public domain',
 'source':'https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_110m_admin_0_countries.geojson',
 'terms':'https://www.naturalearthdata.com/about/terms-of-use/',
 'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),
 'projection':'Equirectangular; x=(lon+180)/360*672; y=(90-lat)/180*252; full world; image origin (24,150).',
 'notes':'Generalized boundaries are illustrative, not a political position. Station pins use Radio Browser coordinates only.'},
 'icon': {'creator':'Cartridge Team','description':'Original radio dial drawn from circles, lines and a tuning needle.'},
 'directory':{'source':'https://www.radio-browser.info/','api':'https://docs.radio-browser.info/',
 'license':'Community data; Radio Browser states its database is public domain.',
 'notes':'Stream content and station marks remain with their respective owners. No station logos are fabricated or redistributed. Coordinates and codec labels are contributor-supplied and may be inaccurate.'}}
(root/'assets/attribution.json').write_text(json.dumps(metadata,indent=2)+'\n')
