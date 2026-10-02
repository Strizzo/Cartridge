#!/usr/bin/env python3
"""Generate tiny original simulator fixtures; these are not playable ROM dumps."""
import json
from pathlib import Path
import struct
import sys
import zlib

def png(path, accent):
    w,h=180,240
    rows=[]
    for y in range(h):
        row=bytearray()
        for x in range(w):
            color=accent if (x//24+y//24)%3 == 0 else (19,20,22)
            if 28 < x < 152 and 45 < y < 195 and (x+y)%40 < 9:color=(244,239,222)
            row.extend(color)
        rows.append(b'\0'+bytes(row))
    def chunk(t,data):return struct.pack('>I',len(data))+t+data+struct.pack('>I',zlib.crc32(t+data)&0xffffffff)
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>2I5B',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b''.join(rows)))+chunk(b'IEND',b''))

root=Path(sys.argv[1]);root.mkdir(parents=True,exist_ok=True)
for system,extension,names,color in [('snes','sfc',['Paper Rally','Signal Runner','Pixel Garden'],(230,53,33)),('psp','iso',['Orbit Pilot','Block Circuit'],(40,128,180))]:
    roms=root/'roms'/system;roms.mkdir(parents=True,exist_ok=True)
    games=[]
    for i,name in enumerate(names):
        file=roms/(name+'.'+extension)
        if not file.exists():file.write_text('Cartridge simulator fixture. Not a playable ROM.\n')
        png(roms/(name+'.png'),color)
        games.append('<game><path>./'+file.name+'</path><name>'+name+'</name><image>./'+name+'.png</image><players>1</players><desc>Original simulator fixture for library and launch testing.</desc></game>')
    (roms/'gamelist.xml').write_text('<gameList>'+''.join(games)+'</gameList>')
(root/'es_systems.cfg').write_text('''<systemList>
<system><name>snes</name><fullname>Nintendo - Super Nintendo</fullname><path>/roms/snes</path><extension>.sfc</extension><theme>snes</theme><command>retroarch -L /home/ark/.config/retroarch/cores/%CORE%_libretro.so %ROM%</command><emulators><emulator name="retroarch"><cores><core>snes9x</core></cores></emulator></emulators></system>
<system><name>ppsspp</name><fullname>Sony - PlayStation Portable</fullname><path>/roms/psp</path><extension>.iso</extension><theme>psp</theme><command>/usr/local/bin/ppsspp.sh %EMULATOR% %ROM%</command><emulators><emulator name="standalone"/></emulators></system>
</systemList>''')
(root/'es_settings.cfg').write_text('<string name="GlobalPerformanceGovernor" value="powersave"/>')
