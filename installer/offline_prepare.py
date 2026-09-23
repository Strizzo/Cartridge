#!/usr/bin/env python3
"""Prepare an OFFLINE, mounted Linux image to boot Cartridge first.

This handles filesystem content only. A future desktop imager must select the
physical disk, clone and mount its partitions in Linux, run this on the clone,
check it, and write the verified result. Never point this at a running system.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def atomic_copy(source, destination, *, preserve_mode=True):
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=destination.parent, prefix='.cartridge-stage-', delete=False) as temp:
        temporary = Path(temp.name)
        try:
            with source.open('rb') as inp:
                shutil.copyfileobj(inp, temp, 1024 * 1024)
            temp.flush()
            os.fsync(temp.fileno())
            # exFAT permissions come from mount options; chmod may be rejected.
            if preserve_mode:
                os.fchmod(temp.fileno(), source.stat().st_mode & 0o777)
        except BaseException:
            temporary.unlink(missing_ok=True)
            raise
    try:
        if sha256(temporary) != sha256(source):
            raise RuntimeError(f'Copy verification failed: {source}')
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def write_manifest(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode='w', dir=path.parent, prefix='.manifest-', delete=False) as temp:
        temporary = Path(temp.name)
        json.dump(value, temp, indent=2)
        temp.write('\n')
        temp.flush()
        os.fsync(temp.fileno())
    os.replace(temporary, path)


def check_layout(root, roms, backup, require_mount):
    root, roms, backup = (p.resolve() for p in (root, roms, backup))
    if root == Path('/') or not root.is_dir() or not roms.is_dir():
        raise RuntimeError('An offline Linux root and its ROMS mount are required')
    if roms.parent != root or roms.name not in ('roms', 'roms2'):
        raise RuntimeError('ROMS must be mounted directly at <root>/roms or <root>/roms2')
    if backup == root or root in backup.parents or backup == roms or roms in backup.parents:
        raise RuntimeError('Backup must be outside both card partitions')
    if backup.exists():
        raise RuntimeError('Backup destination must be a new directory')
    if require_mount:
        if not os.path.ismount(root) or not os.path.ismount(roms):
            raise RuntimeError('The Linux root and ROMS must be separate offline mounts')
        if root.stat().st_dev == roms.stat().st_dev:
            raise RuntimeError('ROMS is not a separate filesystem')
        fstype = subprocess.check_output(['findmnt', '-n', '-o', 'FSTYPE', '-T', str(root)], text=True).strip()
        if fstype != 'ext4':
            raise RuntimeError('Expected an ext4 Linux root image, found '+fstype)
    return root, roms, backup


def check_stock(root, app):
    service = root/'etc/systemd/system/emulationstation.service'
    if not service.is_file() or service.is_symlink():
        raise RuntimeError('Stock EmulationStation service is missing or masked')
    text = service.read_text()
    section = ''
    service_entries = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith(('#', ';')):
            continue
        if line.startswith('[') and line.endswith(']'):
            section = line
        elif section == '[Service]' and '=' in line:
            service_entries.append(line)
    if (service_entries.count('User=ark') != 1 or
            service_entries.count('ExecStart=/usr/bin/emulationstation/emulationstation.sh') != 1 or
            sum(line.startswith('ExecStart=') for line in service_entries) != 1):
        raise RuntimeError('Unsupported stock service; no conversion attempted')
    if not (root/'usr/bin/emulationstation/emulationstation.sh').is_file():
        raise RuntimeError('Stock ES fallback is missing')
    wants = root/'etc/systemd/system/multi-user.target.wants'
    enabled = wants/'emulationstation.service'
    legacy = wants/'cartridge-boot.service'
    if (not enabled.is_symlink() or os.readlink(enabled) not in
            ('../emulationstation.service', '/etc/systemd/system/emulationstation.service') or
            legacy.exists() or legacy.is_symlink()):
        raise RuntimeError('Stock ES must be enabled and legacy Cartridge boot disabled')
    managed = root/'etc/systemd/system/emulationstation.service.d/99-cartridge-primary.conf'
    dropins = managed.parent
    if dropins.is_symlink() or (dropins.exists() and not dropins.is_dir()):
        raise RuntimeError('Unsupported service override directory')
    if dropins.exists():
        for override in dropins.iterdir():
            if override == managed:
                continue
            if override.name != '90-cartridge-recovery.conf' or not override.is_file() or override.is_symlink():
                raise RuntimeError('Unsupported service override: '+override.name)
            for raw in override.read_text().splitlines():
                line = raw.strip()
                if not line or line.startswith(('#', ';')):
                    continue
                if line == '[Service]':
                    continue
                key, sep, _ = line.partition('=')
                if not sep or key not in {'StandardInput', 'StandardOutput', 'StandardError', 'RestartSec'}:
                    raise RuntimeError('Recovery override changes unsupported service settings')
    if managed.exists() and not managed.read_text().startswith('# Managed by Cartridge primary session v1\n'):
        raise RuntimeError('Unrecognized primary override; no conversion attempted')
    if not app.startswith(('/roms/Cartridge', '/roms2/Cartridge')):
        raise RuntimeError('Invalid app installation path')


def bundle_files(bundle, roms):
    expected = ['cartridge', 'autosetup.sh', 'cartridge-session.py', 'setup-primary.py',
                'game-library.py', 'registry.json', 'assets/boot_logo.png']
    for directory in ('assets/fonts', 'assets/overlays', 'lua_cartridges'):
        source_dir = bundle/directory
        if not source_dir.is_dir():
            raise RuntimeError('Bundle lacks '+directory)
        expected.extend(str(p.relative_to(bundle)) for p in source_dir.rglob('*') if p.is_file())
    result = []
    for name in sorted(set(expected)):
        source = bundle/name
        if not source.is_file() or source.is_symlink() or source.name.startswith('._'):
            raise RuntimeError('Unexpected bundle file '+str(source))
        result.append((source, roms/'Cartridge'/name))
    for name in ('Cartridge.sh', 'Setup Cartridge Boot.sh', 'Undo Cartridge Boot.sh'):
        source = bundle/'tools'/name
        if not source.is_file() or source.is_symlink():
            raise RuntimeError('Bundle lacks tool '+name)
        result.append((source, roms/'tools'/name))
    return result


def prepare(root, roms, bundle, backup, *, require_mount=True):
    root, roms, backup = check_layout(Path(root), Path(roms), Path(backup), require_mount)
    bundle = Path(bundle).resolve()
    if backup == bundle or bundle in backup.parents:
        raise RuntimeError('Backup must be outside the CI bundle')
    if not bundle.is_dir() or not (bundle/'dev/build-revision').is_file():
        raise RuntimeError('Expected a CI device bundle with a build revision')
    app = '/' + str(roms.relative_to(root)) + '/Cartridge'
    check_stock(root, app)
    files = bundle_files(bundle, roms)
    managed = root/'etc/systemd/system/emulationstation.service.d/99-cartridge-primary.conf'
    session = root/'usr/local/lib/cartridge/cartridge-session.py'
    fallback = root/'home/ark/.cartridges/session/fallback.json'
    targets = [target for _, target in files] + [managed, session, fallback]
    for target in targets:
        base = root if target in (managed, session, fallback) else roms
        ancestor = target.parent
        while ancestor != base:
            if ancestor.is_symlink():
                raise RuntimeError('Refusing symlink parent '+str(ancestor))
            ancestor = ancestor.parent
        if target.is_symlink():
            raise RuntimeError('Refusing to replace symlink '+str(target))
        if target.exists() and not target.is_file():
            raise RuntimeError('Refusing non-file target '+str(target))
    if not (roms/'Cartridge').is_dir() or not (roms/'tools').is_dir():
        raise RuntimeError('Expected existing ROMS/Cartridge and ROMS/tools directories')
    if not (bundle/'cartridge').is_file() or not (bundle/'assets/fonts').is_dir():
        raise RuntimeError('Incomplete bundle')
    # The backup is committed before the first image change. The owner can
    # recover from an interrupted run using this manifest and previous/ tree.
    backup.mkdir(parents=True)
    rows = []
    for target in targets:
        base = root if target == managed or target == session or target == fallback else roms
        rel = ('root' if base == root else 'roms') + '/' + str(target.relative_to(base))
        old = sha256(target) if target.is_file() else None
        if old is not None:
            previous = backup/'previous'/rel
            atomic_copy(target, previous)
            if sha256(previous) != old:
                raise RuntimeError('Backup checksum mismatch: '+rel)
        rows.append({'path': rel, 'previous_sha256': old})
    manifest = {'state': 'backed_up', 'build_revision': (bundle/'dev/build-revision').read_text().strip(),
                'cartridge_sha256': sha256(bundle/'cartridge'), 'files': rows,
                'games_and_saves_written': False}
    write_manifest(backup/'manifest.json', manifest)
    def restore():
        for item in reversed(rows):
            location, relative = item['path'].split('/', 1)
            destination = (root if location == 'root' else roms)/relative
            old = item['previous_sha256']
            if old is None:
                destination.unlink(missing_ok=True)
            else:
                atomic_copy(backup/'previous'/item['path'], destination, preserve_mode=(location == 'root'))
                if sha256(destination) != old:
                    raise RuntimeError('Rollback checksum mismatch: '+item['path'])
    try:
        for source, target in files:
            atomic_copy(source, target, preserve_mode=False)
            if sha256(target) != sha256(source):
                raise RuntimeError('Installed file checksum mismatch: '+str(target))
        # The same setup code used on the device applies to the offline root.
        spec = importlib.util.spec_from_file_location('cartridge_setup_offline', bundle/'setup-primary.py')
        setup = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(setup)
        setup.configure(root, Path(app), 'enable')
        if not managed.is_file() or not session.is_file():
            raise RuntimeError('Startup override or supervisor missing after setup')
        if 'ExecStart=/usr/bin/python3 /usr/local/lib/cartridge/cartridge-session.py' not in managed.read_text():
            raise RuntimeError('Startup override did not select Cartridge')
        if not (root/'etc/systemd/system/multi-user.target.wants/emulationstation.service').is_symlink():
            raise RuntimeError('Stock recovery service lost its enable link')
    except BaseException:
        try:
            restore()
            manifest['state'] = 'rolled_back_after_failure'
        except BaseException:
            manifest['state'] = 'rollback_incomplete'
        write_manifest(backup/'manifest.json', manifest)
        raise
    manifest['state'] = 'prepared_and_verified'
    manifest['cartridge_default_on_next_boot'] = True
    write_manifest(backup/'manifest.json', manifest)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True, help='Mounted OFFLINE ext4 root image')
    parser.add_argument('--roms', type=Path, required=True, help='Separate mounted ROMS partition under --root')
    parser.add_argument('--bundle', type=Path, required=True, help='Verified CI device bundle /Cartridge')
    parser.add_argument('--backup', type=Path, required=True, help='New backup directory outside the card images')
    args = parser.parse_args()
    result = prepare(args.root, args.roms, args.bundle, args.backup)
    print(json.dumps({'state': result['state'], 'build_revision': result['build_revision'],
                      'cartridge_default_on_next_boot': result['cartridge_default_on_next_boot']}, indent=2))


if __name__ == '__main__':
    main()
