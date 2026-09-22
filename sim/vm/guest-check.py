#!/usr/bin/env python3
"""Disposable ARM VM checks. Refuses machines without the VM marker."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
app = root/'bundle/Cartridge'
report_dir = root/'results'

def run(*argv, **kwargs):
    return subprocess.run(argv, check=True, text=True, **kwargs)


def main():
    if platform.machine() != 'aarch64' or not Path('/etc/cartridge-compat-vm').is_file():
        raise RuntimeError('Only run inside the disposable Cartridge ARM VM')
    if os.geteuid() != 0: raise RuntimeError('Run inside the VM with sudo')
    report_dir.mkdir(exist_ok=True)
    run(str(app/'cartridge'), '--version')
    linked = run('ldd',str(app/'cartridge'),capture_output=True).stdout
    (report_dir/'libraries.txt').write_text(linked)
    if 'not found' in linked: raise RuntimeError('ARM runtime library missing')
    run('python3','-m','unittest','discover','-s',str(root/'tests'),'-p','test_*.py',cwd=root)
    home = root/'home';home.mkdir(exist_ok=True)
    run('python3',str(root/'sim/setup-game-fixture.py'),str(home/'device'))
    env = dict(os.environ, CARTRIDGE_SIM='1', CARTRIDGE_HOME=str(home),
               CARTRIDGE_ASSETS=str(app/'assets'), CARTRIDGE_HIDDEN='1',
               CARTRIDGE_SOFTWARE='1', SDL_VIDEODRIVER='dummy',
               CARTRIDGE_READY_FILE=str(home/'ready'),
               CARTRIDGE_ES_SYSTEMS=str(home/'device/es_systems.cfg'),
               CARTRIDGE_ES_SETTINGS=str(home/'device/es_settings.cfg'),
               CARTRIDGE_ES_HOME=str(home/'device'),CARTRIDGE_ROMS=str(home/'device/roms'))
    run(str(app/'dev/sim-check'),env=env,cwd=app)
    # Install an explicitly fake stock service in this disposable VM, then run
    # the real setup/undo against Linux systemd rather than an offline fixture.
    if subprocess.run(['id','ark'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
        run('useradd','-m','-s','/bin/bash','ark')
    deployed=Path('/roms/Cartridge');deployed.mkdir(parents=True,exist_ok=True)
    for name in ['assets','lua_cartridges','registry.json','game-library.py']:
        path=deployed/name
        if not path.is_symlink() and path.exists(): raise RuntimeError('Unexpected fixture path '+str(path))
        if path.is_symlink():path.unlink()
        path.symlink_to(app/name)
    binary=deployed/'cartridge'
    binary.write_text('#!/bin/bash\nif [[ "${1:-}" == --version ]]; then exec "'+str(app/'cartridge')+'" --version; fi\nif [[ -f /roms/Cartridge/crash-test ]]; then exit 9; fi\nexec "'+str(app/'dev/sim-check')+'" --handoff\n')
    binary.chmod(0o755)
    es=Path('/usr/bin/emulationstation/emulationstation.sh');es.parent.mkdir(parents=True,exist_ok=True)
    es.write_text('#!/bin/sh\necho "VM ES fallback" > /home/ark/es-fallback-seen\n');es.chmod(0o755)
    unit=Path('/etc/systemd/system/emulationstation.service')
    unit.write_text('[Unit]\nDescription=Cartridge VM stock ES fixture\n[Service]\nType=oneshot\nUser=ark\nWorkingDirectory=/home/ark\nExecStart=/usr/bin/emulationstation/emulationstation.sh\n[Install]\nWantedBy=multi-user.target\n')
    dropins=unit.parent/'emulationstation.service.d';dropins.mkdir(exist_ok=True)
    vm_env='[Service]\n'
    for k,v in env.items():
        if k.startswith('CARTRIDGE_') or k=='SDL_VIDEODRIVER':
            if k=='CARTRIDGE_READY_FILE':continue # supervisor supplies a fresh one
            vm_env+='Environment="'+k+'='+v+'"\n'
    (dropins/'00-vm-only.conf').write_text(vm_env)
    run('chown','-R','ark:ark',str(home))
    setup=root/'deploy/setup-primary.py'
    run('systemctl','daemon-reload');run('systemctl','enable','emulationstation.service')
    (deployed/'crash-test').unlink(missing_ok=True)
    flag=Path('/home/ark/es-fallback-seen');flag.unlink(missing_ok=True)
    session=Path('/home/ark/.cartridges/session/last-session.json')
    try:
        run('python3',str(setup),'enable','--cartridge-dir',str(deployed))
        run('systemctl','start','emulationstation.service')
        if not flag.exists() or json.loads(session.read_text())['reason']!='emulationstation_requested':
            raise RuntimeError('systemd did not hand the rendered UI back to ES')
        (report_dir/'systemd-handoff.json').write_text(session.read_text())
        flag.unlink();(deployed/'crash-test').touch()
        run('systemctl','start','emulationstation.service')
        if not flag.exists() or json.loads(session.read_text())['reason']!='startup_failed':
            raise RuntimeError('systemd startup failure did not fall back')
        (report_dir/'systemd-failure.json').write_text(session.read_text())
        flag.unlink();run('systemctl','start','emulationstation.service')
        if not flag.exists() or json.loads(session.read_text())['reason']!='previous_failure':
            raise RuntimeError('systemd did not honor the failure latch')
        flag.unlink();run('python3',str(setup),'disable')
        run('systemctl','start','emulationstation.service')
        if not flag.exists() or 'cartridge-session.py' in run('systemctl','show','emulationstation.service','-p','ExecStart',capture_output=True).stdout:
            raise RuntimeError('Undo failed to restore the stock service')
    finally:
        (deployed/'crash-test').unlink(missing_ok=True)
        run('python3',str(setup),'disable')
        run('systemctl','disable','emulationstation.service')
        journal=run('journalctl','-u','emulationstation.service','--no-pager','-n','100',capture_output=True).stdout
        (report_dir/'systemd.log').write_text(journal)
    shutil.copytree(home/'checks',report_dir/'screenshots',dirs_exist_ok=True)
    result={'architecture':platform.machine(),'kernel':platform.release(),
            'binary_sha256':hashlib.sha256((app/'cartridge').read_bytes()).hexdigest(),
            'checks':['ARM ELF libraries','Python unit tests','native ARM simulator scenarios',
                      'real systemd setup','rendered ES handoff','startup failure and latch','undo'],
            'hardware_performance_validated':False}
    (report_dir/'verification.json').write_text(json.dumps(result,indent=2)+'\n')
    print('ARM LINUX VM CHECK PASSED. Results: '+str(report_dir))

if __name__=='__main__':main()
