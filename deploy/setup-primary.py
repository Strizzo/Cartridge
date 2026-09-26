#!/usr/bin/env python3
"""Install one reversible override of the working EmulationStation service."""
import argparse
import os
from pathlib import Path
import re
import subprocess
import tempfile

HEADER = '# Managed by Cartridge primary session v1\n'
DROPIN = Path('etc/systemd/system/emulationstation.service.d/99-cartridge-primary.conf')
SESSION = Path('usr/local/lib/cartridge/cartridge-session.py')


def write_atomic(path, data, mode):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=str(path.parent), delete=False) as f:
        temporary = Path(f.name)
        f.write(data); f.flush(); os.fchmod(f.fileno(), mode); os.fsync(f.fileno())
    os.replace(str(temporary), str(path))


def configure(root, app, action, *, offline_bundle=None):
    root = root.resolve()
    dropin = root / DROPIN
    real = root == Path('/')
    if real and os.geteuid() != 0:
        raise RuntimeError('Run setup with sudo; no changes made')
    if dropin.exists() and not dropin.read_text().startswith(HEADER):
        raise RuntimeError('Existing override is not owned by this installer')
    if action == 'disable':
        if dropin.exists(): dropin.unlink()
        if real: subprocess.run(['systemctl', 'daemon-reload'], check=True)
        return 'Stock EmulationStation restored for next boot. Current session unchanged.'
    if action == 'status':
        return 'Cartridge primary enabled' if dropin.exists() else 'Stock EmulationStation enabled'
    # The installed path is deliberately independent of exFAT mounts. A missing
    # Cartridge folder will still leave the ES fallback executable on root.
    if not app.is_absolute() or not re.fullmatch(r'/[A-Za-z0-9_./-]+', str(app)):
        raise RuntimeError('Use an absolute Cartridge directory without whitespace or systemd specifiers')
    service = root / 'etc/systemd/system/emulationstation.service'
    if not service.is_file() or service.is_symlink():
        raise RuntimeError('Expected recovered EmulationStation service is missing or masked')
    text = service.read_text()
    if 'User=ark' not in text or 'ExecStart=/usr/bin/emulationstation/emulationstation.sh' not in text:
        raise RuntimeError('Unrecognized stock startup service; inspect before replacing ExecStart')
    if not (root/'usr/bin/emulationstation/emulationstation.sh').is_file():
        raise RuntimeError('Stock EmulationStation fallback is missing')
    if (root/'etc/systemd/system/multi-user.target.wants/cartridge-boot.service').is_symlink():
        raise RuntimeError('Legacy Cartridge boot service still enabled; restore stock boot first')
    if not (root/'etc/systemd/system/multi-user.target.wants/emulationstation.service').is_symlink():
        raise RuntimeError('Stock EmulationStation service must already be enabled')
    if offline_bundle is not None and real:
        raise RuntimeError('An offline bundle cannot configure the running system')
    deployed = Path(offline_bundle) if offline_bundle is not None else root / app.relative_to('/')
    if not (deployed/'cartridge').is_file() or not (deployed/'assets/fonts').is_dir():
        raise RuntimeError('Cartridge binary or fonts missing')
    if real:
        # Check the ELF loader and dynamic libraries before installing startup.
        result = subprocess.run([str(deployed/'cartridge'), '--version'], capture_output=True, text=True, timeout=10)
        if result.returncode or not result.stdout.startswith('cartridge '):
            raise RuntimeError('Cartridge executable cannot run: ' + result.stderr)
    source = Path(__file__).with_name('cartridge-session.py')
    write_atomic(root/SESSION, source.read_bytes(), 0o755)
    conf = (HEADER + '[Service]\nExecStart=\nExecStart=/usr/bin/python3 /' + str(SESSION)
            + ' --cartridge-dir ' + str(app) + '\n')
    write_atomic(dropin, conf.encode(), 0o644)
    if real:
        subprocess.run(['systemctl', 'daemon-reload'], check=True)
    # Only reset our own failure marker; user-created recovery flag stays valid.
    failure = root/'home/ark/.cartridges/session/fallback.json'
    if failure.exists(): failure.unlink()
    return 'Cartridge will start directly on next boot. EmulationStation remains enabled as fallback.'


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=['enable', 'disable', 'status'])
    p.add_argument('--cartridge-dir', type=Path, default=Path('/roms/Cartridge'))
    p.add_argument('--root', type=Path, default=Path('/'), help='Offline test root; never runs host systemctl')
    args = p.parse_args()
    try:
        print(configure(args.root, args.cartridge_dir, args.action))
    except Exception as exc:
        p.exit(1, 'Setup stopped: '+str(exc)+'\n')
