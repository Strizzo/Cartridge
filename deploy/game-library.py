#!/usr/bin/env python3
"""Read the installed ES library and launch with its emulator configuration.

No ROMs, saves, gamelists, settings, or emulator configuration are rewritten.
Only Cartridge's own launch report/log is written. Python 3 stdlib only.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

SKIP_DIRS = {'downloaded_images', 'images', 'media', 'videos', 'manuals', 'gamelists'}
TOKEN = re.compile(r'%([A-Z_]+)%')


def xml(path, fragment=False):
    text = Path(path).read_text(encoding='utf-8-sig')
    if '<!DOCTYPE' in text.upper() or '<!ENTITY' in text.upper():
        raise ValueError('External/entity declarations are not supported')
    # Stock ArkOS uses literal shell '&' in command nodes (not valid XML).
    # Match its tolerant reader without changing anything on the card.
    text = re.sub(r'&(?!amp;|lt;|gt;|quot;|apos;|#\d+;|#x[0-9a-fA-F]+;)', '&amp;', text)
    if fragment:
        text = '<settings>' + re.sub(r'<\?xml.*?\?>', '', text, flags=re.S) + '</settings>'
    return ET.fromstring(text)


def inside(path, root):
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


class Library:
    def __init__(self, config=None, settings=None, rom_root=None):
        home = Path(os.environ.get('CARTRIDGE_ES_HOME', str(Path.home())))
        preferred = home/'.emulationstation/es_systems.cfg'
        self.config = Path(config or os.environ.get('CARTRIDGE_ES_SYSTEMS') or
                           (preferred if preferred.is_file() else '/etc/emulationstation/es_systems.cfg'))
        self.home = home
        self.settings_path = Path(settings or os.environ.get('CARTRIDGE_ES_SETTINGS') or home/'.emulationstation/es_settings.cfg')
        self.settings = {}
        if self.settings_path.is_file():
            self.settings = {x.get('name'): x.get('value', '') for x in xml(self.settings_path, True)}
        override = rom_root or os.environ.get('CARTRIDGE_ROMS')
        self.root = Path(override).resolve() if override else None
        self.systems = {}
        for entry in xml(self.config).findall('system'):
            sid = entry.findtext('name', '').strip()
            raw = entry.findtext('path', '')
            raw_path = Path(str(home)+raw[1:] if raw.startswith('~/') else raw)
            if not sid or entry.findtext('platform') == 'ignore' or not raw_path.is_absolute():
                continue
            # The Options/system tools category is not a game library. Only
            # configured /roms and /roms2 trees are admitted.
            if len(raw_path.parts) < 3 or raw_path.parts[1] not in ('roms', 'roms2'):
                continue
            path = self.root.joinpath(*raw_path.parts[2:]) if self.root else raw_path
            boundary = self.root or Path('/'+raw_path.parts[1])
            if not inside(path, boundary):
                continue
            emulators = []
            for e in entry.findall('emulators/emulator'):
                emulators.append({'name': e.get('name', ''), 'command': e.get('command', ''),
                                  'cores': [c.text or '' for c in e.findall('cores/core')],
                                  'governors': [g.text or '' for g in e.findall('governors/governor')]})
            self.systems[sid] = {'id': sid, 'name': entry.findtext('fullname', sid), 'path': path,
                'extensions': set(entry.findtext('extension', '').lower().split()),
                'command': entry.findtext('command', ''), 'emulators': emulators,
                'theme': entry.findtext('theme', sid)}

    def system(self, sid):
        if sid not in self.systems:
            raise ValueError('Unknown game system: '+sid)
        return self.systems[sid]

    def paths(self, system):
        base = system['path']
        if not base.is_dir(): return
        pending = [base]
        while pending:
            directory = pending.pop()
            with os.scandir(directory) as entries:
                for e in entries:
                    if e.name.startswith('.') or e.is_symlink(): continue
                    p = Path(e.path)
                    if p.suffix.lower() in system['extensions']:
                        yield p
                    elif e.is_dir(follow_symlinks=False) and e.name not in SKIP_DIRS:
                        pending.append(p)

    def list_systems(self):
        result = []
        for s in self.systems.values():
            if next(self.paths(s), None) is not None:
                result.append({'id': s['id'], 'name': s['name'], 'theme': s['theme']})
        return sorted(result, key=lambda s: s['name'].casefold())

    def metadata(self, system):
        candidates = [system['path']/'gamelist.xml', self.home/'.emulationstation/gamelists'/system['id']/'gamelist.xml']
        path = next((p for p in candidates if p.is_file()), None)
        if path is None: return {}
        rows = {}
        for game in xml(path).findall('game'):
            raw = game.findtext('path', '')
            if not raw: continue
            name = Path(raw)
            if name.is_absolute() and self.root and len(name.parts) >= 3 and name.parts[1] in ('roms', 'roms2'):
                name = self.root.joinpath(*name.parts[2:])
            elif not name.is_absolute(): name = system['path']/name
            if inside(name, system['path']):
                rows[str(name.resolve())] = {c.tag: c.text or '' for c in game}
        return rows

    def list_games(self, sid):
        s = self.system(sid); metadata = self.metadata(s); result = []
        for path in self.paths(s):
            md = metadata.get(str(path.resolve()), {})
            if md.get('hidden', '').lower() == 'true': continue
            art = Path(md.get('image', ''))
            if not art.is_absolute(): art = s['path']/art
            if self.root and art.is_absolute() and len(art.parts) >= 3 and art.parts[1] in ('roms','roms2'):
                art = self.root.joinpath(*art.parts[2:])
            artwork = str(art) if inside(art, s['path']) and art.is_file() else None
            result.append({'path': str(path.resolve()), 'name': md.get('name') or path.stem,
                           'sort_name': md.get('sortname') or md.get('name') or path.stem,
                           'image': artwork, 'favorite': md.get('favorite', '').lower() == 'true',
                           'description': md.get('desc', '')[:2000], 'players': md.get('players', '')})
        return sorted(result, key=lambda g: (not g['favorite'], g['sort_name'].casefold()))

    def plan(self, sid, rom):
        s = self.system(sid); path = Path(rom).resolve()
        if not inside(path, s['path']) or not path.exists() or path.suffix.lower() not in s['extensions']:
            raise ValueError('Game path is not a configured ROM for '+sid)
        md = self.metadata(s).get(str(path), {})
        emulators = s['emulators']; names = [e['name'] for e in emulators]
        default = self.settings.get(sid+'.emulator', '')
        default = default if default in names else next(iter(names), '')
        emulator = md.get('emulator') or default
        if emulator and emulator not in names: raise ValueError('Configured emulator unavailable: '+emulator)
        data = next((e for e in emulators if e['name']==emulator), {'cores': [], 'governors': [], 'command': ''})
        core_default = self.settings.get(sid+'.core', '')
        core_default = core_default if core_default in data['cores'] else next(iter(data['cores']), '')
        core = md.get('core') or core_default
        if core and core not in data['cores']: raise ValueError('Configured core unavailable: '+core)
        # Mirror FCAMOD: game metadata, validated emulator governor, game-name
        # setting, system setting, then the global governor. No forced max clock.
        saved_gov = self.settings.get(sid+'.governor', '')
        governor = (md.get('governor') or (saved_gov if saved_gov in data['governors'] else '')
                    or self.settings.get(md.get('name', path.stem)+'.governor') or saved_gov
                    or self.settings.get('GlobalPerformanceGovernor', ''))
        template = data['command'] or s['command']
        if not template.strip(): raise ValueError('No emulator command configured')
        values = {'ROM': str(path), 'ROM_RAW': str(path), 'BASENAME': path.stem,
                  'EMULATOR': emulator, 'CORE': core, 'GOVERNOR': governor,
                  'SYSTEM': sid, 'HOME': str(self.home)}
        command = expand_command(template, values)
        # Preserve emulator failure status across the stock clock-reset suffix.
        command = re.sub(r';\s*sudo\s+perfnorm\s*;?\s*$',
                         '; cartridge_status=$?; sudo perfnorm; exit "$cartridge_status"', command)
        return {'system': sid, 'rom': str(path), 'name': md.get('name') or path.stem,
                'emulator': emulator, 'core': core, 'governor': governor,
                'command': command, 'cwd': str(self.home)}


def expand_command(template, values):
    # Substitute once: %CORE% inside a ROM filename must remain literal. Honor
    # existing quote contexts without letting filenames become shell syntax.
    out = []; quote = None; i = 0
    while i < len(template):
        m = TOKEN.match(template, i)
        if m:
            key = m.group(1)
            if key not in values: raise ValueError('Unsupported command placeholder: '+key)
            value = values[key]
            if '\x00' in value: raise ValueError('NUL in launch argument')
            if quote == "'": out.append(value.replace("'", "'\"'\"'"))
            elif quote == '"': out.append(re.sub(r'([\\$`\"])', r'\\\1', value))
            else: out.append(shlex.quote(value))
            i = m.end(); continue
        c = template[i];out.append(c)
        if c == '\\' and quote != "'" and i+1 < len(template):
            i += 1;out.append(template[i])
        elif c in "'\"":
            if quote == c: quote = None
            elif quote is None: quote = c
        i += 1
    if quote: raise ValueError('Unclosed quote in emulator command')
    return ''.join(out)


def launch(plan):
    state_home = Path(os.environ.get('CARTRIDGE_HOME', str(Path.home())))
    state = state_home/'.cartridges/games';state.mkdir(parents=True, exist_ok=True)
    simulated = os.environ.get('CARTRIDGE_SIM') == '1'
    if not simulated and sys.platform != 'linux':
        raise RuntimeError('Device emulator commands can only run on Linux; use the simulator on this host')
    report = dict(plan, simulated=simulated, started=time.time())
    log_path = state/'launch.log'
    if log_path.exists() and log_path.stat().st_size > 1024*1024:
        os.replace(log_path, state/'launch.previous.log')
    status = 0
    if not simulated:
        env = os.environ.copy()
        env.pop('CARTRIDGE_READY_FILE', None)
        with log_path.open('a') as log:
            print('\nLaunching '+plan['name'], file=log, flush=True)
            status = subprocess.call(['/bin/bash', '-o', 'pipefail', '-c', plan['command']],
                                     cwd=plan['cwd'], env=env, stdout=log, stderr=subprocess.STDOUT)
    report.update(exit_code=status, finished=time.time())
    temporary = state/'last-launch.tmp'
    temporary.write_text(json.dumps(report, indent=2)+'\n');os.replace(temporary, state/'last-launch.json')
    if status: raise RuntimeError('Emulator exited with status %d. See %s' % (status, log_path))
    return report


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=['systems','games','plan','launch'])
    p.add_argument('--system');p.add_argument('--rom')
    p.add_argument('--config', type=Path);p.add_argument('--settings', type=Path);p.add_argument('--rom-root', type=Path)
    args=p.parse_args(); lib=Library(args.config,args.settings,args.rom_root)
    if args.action=='systems': result=lib.list_systems()
    elif args.action=='games': result=lib.list_games(args.system)
    else:
        result=lib.plan(args.system,args.rom or '')
        if args.action=='launch': result=launch(result)
    print(json.dumps(result, ensure_ascii=True))

if __name__=='__main__':
    try: main()
    except Exception as exc:
        print(str(exc), file=sys.stderr);sys.exit(1)
