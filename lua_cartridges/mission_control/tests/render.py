"""Capture Mission Control with the shared main-thread SDL app-check runner.

All settings and derivative fixtures live in temporary storage. No live network.
Run: python3 lua_cartridges/mission_control/tests/render.py
"""
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
output = Path(os.environ.get('MISSION_CONTROL_CAPTURES', '/tmp/mission-control-captures'))
fixture = root / 'sim/fixtures/mission-control.json'
subprocess.run(['cargo', 'build', '--bin', 'app-check'], cwd=root, check=True)
scenarios = {
    'board': [],
    'detail': ['10:a'],
    'output': ['10:a', '12:y'],
    'confirm': ['10:a', '12:a'],
    'settings': ['10:start'],
    'edit': ['10:start', '12:x'],
    'keyboard': ['10:start', '12:x', '14:a'],
    'ack': ['10:a', '12:a', '14:right', '16:a'],
    'disconnected': ['10:start', '12:r2', '14:b'],
    'first-run': [], 'offline': [], 'empty': [], 'loading': [],
}
with tempfile.TemporaryDirectory(prefix='mission-control-render-') as tmp:
    temp = Path(tmp)
    seed = temp / 'settings.json'
    seed.write_text(json.dumps({'version': 1, 'servers': [{'name': 'Local fixture', 'url': 'http://127.0.0.1:8766'}], 'selected': 1, 'interval': 2}))
    for name, presses in scenarios.items():
        selected_fixture = fixture
        if name in ('empty', 'offline', 'loading'):
            data = json.loads(fixture.read_text())
            if name == 'empty': data[0]['body'] = {'sessions': {}}
            if name == 'offline': data[0]['status'] = 503
            if name == 'loading': data[0]['delay_polls'] = 100
            selected_fixture = temp / (name+'.json')
            selected_fixture.write_text(json.dumps(data))
        args = [str(root/'target/debug/app-check'), 'mission_control', '--fixture', str(selected_fixture),
                '--frames', '28', '--capture', '24', '--out', str(output/name)]
        if name != 'first-run': args += ['--seed', 'mission_control_settings='+str(seed)]
        for press in presses: args += ['--press', press]
        subprocess.run(args, cwd=root, check=True)
        png = (output/name/'frame_0024.png').read_bytes()
        assert png[:8] == b'\x89PNG\r\n\x1a\n' and struct.unpack('>II', png[16:24]) == (720, 720)
print('Mission Control captures:', output)
