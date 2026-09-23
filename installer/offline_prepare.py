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
from pathlib import PurePosixPath
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
    directory = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def managed_roms_path(relative):
    """Only Cartridge payload and three owned Tools entries may be restored."""
    path = PurePosixPath(relative)
    parts = path.parts
    if not parts or path.is_absolute() or str(path) != relative or '..' in parts:
        return False
    if len(parts) == 2 and parts[0] == 'tools':
        return parts[1] in {'Cartridge.sh', 'Setup Cartridge Boot.sh', 'Undo Cartridge Boot.sh'}
    if len(parts) < 2 or parts[0] != 'Cartridge':
        return False
    if len(parts) == 2:
        return parts[1] in {'cartridge', 'autosetup.sh', 'cartridge-session.py',
                            'setup-primary.py', 'game-library.py', 'registry.json'}
    if parts[1] == 'assets':
        return (parts[2:] == ('boot_logo.png',) or
                (len(parts) >= 4 and parts[2] in {'fonts', 'overlays'}))
    return parts[1] == 'lua_cartridges' and len(parts) >= 3


def restore_roms(roms, backup, *, require_mount=True):
    """Resume or perform a verified rollback of Cartridge-owned ROMS files."""
    roms, backup = Path(roms), Path(backup)
    if roms.is_symlink() or backup.is_symlink():
        raise RuntimeError('ROMS mount and backup must not be symlinks')
    roms, backup = roms.resolve(strict=True), backup.resolve(strict=True)
    if not roms.is_dir() or not backup.is_dir() or backup == roms or roms in backup.parents:
        raise RuntimeError('ROMS rollback needs a separate backup directory')
    if require_mount and not os.path.ismount(roms):
        raise RuntimeError('ROMS must be a mounted offline partition')
    manifest_path = backup/'manifest.json'
    if manifest_path.is_symlink() or not manifest_path.is_file():
        raise RuntimeError('Preparation backup manifest is missing or symlinked')
    manifest = json.loads(manifest_path.read_text())
    if manifest.get('state') not in {'backed_up', 'roms_backed_up', 'prepared_and_verified',
                                    'roms_rollback_started', 'roms_rollback_incomplete'}:
        raise RuntimeError('Preparation is not in a restorable state')
    rows = manifest.get('files')
    if not isinstance(rows, list):
        raise RuntimeError('Preparation backup file list is invalid')
    selected, seen = [], set()
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get('path'), str):
            raise RuntimeError('Preparation backup row is invalid')
        path = row['path']
        if path.startswith('root/'):
            continue
        if not path.startswith('roms/') or not managed_roms_path(path[5:]) or path in seen:
            raise RuntimeError('Preparation backup contains an unexpected ROMS path')
        old, installed = row.get('previous_sha256'), row.get('installed_sha256')
        if (old is not None and (not isinstance(old, str) or len(old) != 64) or
                not isinstance(installed, str) or len(installed) != 64):
            raise RuntimeError('Preparation backup checksum is invalid')
        seen.add(path)
        selected.append((path[5:], old, installed))
    if not selected or not any(rel == 'Cartridge/cartridge' for rel, _, _ in selected):
        raise RuntimeError('Preparation backup lacks the Cartridge executable')
    # Validate every current file and every original backup before changing one.
    for rel, old, installed in selected:
        target = roms/rel
        if target.is_symlink() or any(parent.is_symlink() for parent in target.parents if parent != roms and roms in parent.parents):
            raise RuntimeError('Refusing symlink in ROMS rollback target: '+rel)
        if target.exists():
            if not target.is_file() or sha256(target) not in {old, installed}:
                raise RuntimeError('ROMS target changed outside this install: '+rel)
        elif old is not None:
            raise RuntimeError('Previously existing ROMS target disappeared: '+rel)
        if old is not None:
            previous = backup/'previous/roms'/rel
            if previous.is_symlink() or not previous.is_file() or sha256(previous) != old:
                raise RuntimeError('Original ROMS backup is missing or changed: '+rel)
            if any(parent.is_symlink() for parent in previous.parents if parent != backup and backup in parent.parents):
                raise RuntimeError('Original ROMS backup has a symlink parent: '+rel)
    manifest['state'] = 'roms_rollback_started'
    write_manifest(manifest_path, manifest)
    try:
        for rel, old, installed in reversed(selected):
            target = roms/rel
            if target.is_symlink() or (target.exists() and
                    (not target.is_file() or sha256(target) not in {old, installed})):
                raise RuntimeError('ROMS target changed during rollback: '+rel)
            if old is None:
                target.unlink(missing_ok=True)
            else:
                atomic_copy(backup/'previous/roms'/rel, target, preserve_mode=False)
                if sha256(target) != old:
                    raise RuntimeError('ROMS rollback readback differs: '+rel)
        os.sync()
        manifest['state'] = 'roms_rollback_verified'
    except BaseException as exc:
        manifest.update(state='roms_rollback_incomplete', rollback_error=str(exc))
        write_manifest(manifest_path, manifest)
        raise
    write_manifest(manifest_path, manifest)
    return manifest


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


def bundle_digest(bundle):
    """Bind both preparation phases to every file that will be installed."""
    bundle = Path(bundle)
    digest = hashlib.sha256()
    for source, _ in bundle_files(bundle, Path('/unused-roms')):
        digest.update(str(source.relative_to(bundle)).encode('utf-8') + b'\0')
        digest.update(sha256(source).encode('ascii') + b'\n')
    digest.update(b'dev/build-revision\0')
    digest.update(sha256(bundle/'dev/build-revision').encode('ascii') + b'\n')
    return digest.hexdigest()


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
    source_for_target = {target: source for source, target in files}
    expected_new = {}
    for target in targets:
        base = root if target == managed or target == session or target == fallback else roms
        rel = ('root' if base == root else 'roms') + '/' + str(target.relative_to(base))
        old = sha256(target) if target.is_file() else None
        if old is not None:
            previous = backup/'previous'/rel
            atomic_copy(target, previous)
            if sha256(previous) != old:
                raise RuntimeError('Backup checksum mismatch: '+rel)
        source = source_for_target.get(target)
        installed = sha256(source) if source is not None else None
        rows.append({'path': rel, 'previous_sha256': old, 'installed_sha256': installed})
        if installed is not None:
            expected_new[target] = installed
    manifest = {'state': 'backed_up', 'build_revision': (bundle/'dev/build-revision').read_text().strip(),
                'app_path': app, 'cartridge_sha256': sha256(bundle/'cartridge'),
                'session_sha256': sha256(bundle/'cartridge-session.py'), 'files': rows,
                'games_and_saves_written': False}
    os.sync()
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
            if sha256(target) != expected_new[target]:
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
    os.sync()
    write_manifest(backup/'manifest.json', manifest)
    return manifest


def prepare_root(root, bundle, backup, app_path='/roms/Cartridge', *, require_mount=True):
    """Configure only an offline ext4 clone; no ROMS partition is needed in Linux."""
    root, bundle, backup = Path(root), Path(bundle), Path(backup)
    if root.is_symlink() or bundle.is_symlink() or backup.is_symlink():
        raise RuntimeError('Root, bundle and backup must not be symlinks')
    root, bundle = root.resolve(strict=True), bundle.resolve(strict=True)
    backup = backup.parent.resolve(strict=True)/backup.name
    if root == Path('/') or not root.is_dir() or backup.exists():
        raise RuntimeError('Expected an offline root and a new backup directory')
    if backup == root or root in backup.parents or backup == bundle or bundle in backup.parents:
        raise RuntimeError('Backup must be outside the root image and bundle')
    if require_mount:
        if not os.path.ismount(root):
            raise RuntimeError('Linux root must be a mounted offline image')
        fstype = subprocess.check_output(['findmnt', '-n', '-o', 'FSTYPE', '-T', str(root)], text=True).strip()
        if fstype != 'ext4':
            raise RuntimeError('Expected an ext4 Linux root image, found '+fstype)
    if app_path not in {'/roms/Cartridge', '/roms2/Cartridge'}:
        raise RuntimeError('Unsupported Cartridge installation path')
    if not bundle.is_dir() or not (bundle/'dev/build-revision').is_file():
        raise RuntimeError('Expected a CI device bundle with a build revision')
    # Validate the entire payload before changing the clone. Its ROMS files
    # are installed by stage_roms after the root image has passed inspection.
    bundle_sha = bundle_digest(bundle)
    check_stock(root, app_path)
    managed = root/'etc/systemd/system/emulationstation.service.d/99-cartridge-primary.conf'
    session = root/'usr/local/lib/cartridge/cartridge-session.py'
    fallback = root/'home/ark/.cartridges/session/fallback.json'
    targets = (managed, session, fallback)
    for target in targets:
        ancestor = target.parent
        while ancestor != root:
            if ancestor.is_symlink():
                raise RuntimeError('Refusing symlink parent '+str(ancestor))
            ancestor = ancestor.parent
        if target.is_symlink() or (target.exists() and not target.is_file()):
            raise RuntimeError('Refusing unsafe root target '+str(target))
    backup.mkdir(mode=0o700)
    rows = []
    for target in targets:
        rel = 'root/'+str(target.relative_to(root))
        old = sha256(target) if target.is_file() else None
        if old is not None:
            previous = backup/'previous'/rel
            atomic_copy(target, previous)
            if sha256(previous) != old:
                raise RuntimeError('Backup checksum mismatch: '+rel)
        rows.append({'path': rel, 'previous_sha256': old, 'installed_sha256': None})
    manifest = {'state': 'root_backed_up', 'build_revision': (bundle/'dev/build-revision').read_text().strip(),
                'app_path': app_path, 'cartridge_sha256': sha256(bundle/'cartridge'),
                'session_sha256': sha256(bundle/'cartridge-session.py'),
                'bundle_sha256': bundle_sha, 'files': rows,
                'games_and_saves_written': False}
    os.sync()
    write_manifest(backup/'manifest.json', manifest)
    try:
        spec = importlib.util.spec_from_file_location('cartridge_setup_offline', bundle/'setup-primary.py')
        setup = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(setup)
        setup.configure(root, Path(app_path), 'enable', offline_bundle=bundle)
        expected = ('# Managed by Cartridge primary session v1\n[Service]\nExecStart=\n'
                    'ExecStart=/usr/bin/python3 /usr/local/lib/cartridge/cartridge-session.py'
                    ' --cartridge-dir '+app_path+'\n')
        if managed.read_text() != expected or sha256(session) != manifest['session_sha256']:
            raise RuntimeError('Offline root setup did not produce the expected startup files')
        if not (root/'etc/systemd/system/multi-user.target.wants/emulationstation.service').is_symlink():
            raise RuntimeError('Stock ES recovery link disappeared')
        rows[0]['installed_sha256'] = sha256(managed)
        rows[1]['installed_sha256'] = sha256(session)
        if fallback.exists():
            raise RuntimeError('Old Cartridge failure latch was not cleared')
    except BaseException:
        try:
            for row in reversed(rows):
                target = root/row['path'][5:]
                if row['previous_sha256'] is None:
                    target.unlink(missing_ok=True)
                else:
                    atomic_copy(backup/'previous'/row['path'], target)
                    if sha256(target) != row['previous_sha256']:
                        raise RuntimeError('Root rollback checksum mismatch: '+row['path'])
            manifest['state'] = 'root_rolled_back_after_failure'
        except BaseException:
            manifest['state'] = 'root_rollback_incomplete'
        write_manifest(backup/'manifest.json', manifest)
        raise
    manifest['state'] = 'root_prepared_and_verified'
    os.sync()
    write_manifest(backup/'manifest.json', manifest)
    return manifest


def stage_roms(roms, bundle, backup, *, require_mount=True):
    """Stage Cartridge-owned files on mounted exFAT after clone preparation."""
    roms, bundle, backup = Path(roms), Path(bundle), Path(backup)
    if roms.is_symlink() or bundle.is_symlink() or backup.is_symlink():
        raise RuntimeError('ROMS, bundle and backup must not be symlinks')
    roms, bundle, backup = (path.resolve(strict=True) for path in (roms, bundle, backup))
    if not roms.is_dir() or not backup.is_dir() or roms == backup or roms in backup.parents:
        raise RuntimeError('ROMS needs a separate preparation backup')
    if require_mount and not os.path.ismount(roms):
        raise RuntimeError('ROMS must be a mounted offline partition')
    path = backup/'manifest.json'
    if path.is_symlink() or not path.is_file():
        raise RuntimeError('Root preparation manifest is missing')
    manifest = json.loads(path.read_text())
    if manifest.get('state') != 'root_prepared_and_verified':
        raise RuntimeError('Root clone must be prepared before ROMS staging')
    if (manifest.get('build_revision') != (bundle/'dev/build-revision').read_text().strip() or
            manifest.get('cartridge_sha256') != sha256(bundle/'cartridge') or
            manifest.get('session_sha256') != sha256(bundle/'cartridge-session.py') or
            manifest.get('bundle_sha256') != bundle_digest(bundle)):
        raise RuntimeError('Bundle differs from the prepared root')
    if not (roms/'Cartridge').is_dir() or not (roms/'tools').is_dir():
        raise RuntimeError('Expected existing ROMS/Cartridge and ROMS/tools directories')
    files = bundle_files(bundle, roms)
    for _, target in files:
        ancestor = target.parent
        while ancestor != roms:
            if ancestor.is_symlink():
                raise RuntimeError('Refusing symlink parent '+str(ancestor))
            ancestor = ancestor.parent
        if target.is_symlink() or (target.exists() and not target.is_file()):
            raise RuntimeError('Refusing unsafe ROMS target '+str(target))
    rows = manifest['files']
    if not isinstance(rows, list) or not all(isinstance(row, dict) and
            isinstance(row.get('path'), str) and row['path'].startswith('root/') for row in rows):
        raise RuntimeError('Root preparation file list is invalid')
    staged = []
    for source, target in files:
        rel = 'roms/'+str(target.relative_to(roms))
        if not managed_roms_path(rel[5:]):
            raise RuntimeError('Bundle contains an unmanaged ROMS path: '+rel)
        old = sha256(target) if target.is_file() else None
        if old is not None:
            previous = backup/'previous'/rel
            atomic_copy(target, previous, preserve_mode=False)
            if sha256(previous) != old:
                raise RuntimeError('Backup checksum mismatch: '+rel)
        staged.append({'path': rel, 'previous_sha256': old, 'installed_sha256': sha256(source)})
    rows.extend(staged)
    manifest['state'] = 'roms_backed_up'
    os.sync()
    write_manifest(path, manifest)
    try:
        for (source, target), row in zip(files, staged):
            atomic_copy(source, target, preserve_mode=False)
            if sha256(target) != row['installed_sha256']:
                raise RuntimeError('ROMS readback checksum mismatch: '+row['path'])
    except BaseException:
        try:
            for row in reversed(staged):
                target = roms/row['path'][5:]
                if row['previous_sha256'] is None:
                    target.unlink(missing_ok=True)
                else:
                    atomic_copy(backup/'previous'/row['path'], target, preserve_mode=False)
                    if sha256(target) != row['previous_sha256']:
                        raise RuntimeError('ROMS rollback checksum mismatch: '+row['path'])
            manifest['state'] = 'roms_rollback_verified'
        except BaseException:
            manifest['state'] = 'roms_rollback_incomplete'
        write_manifest(path, manifest)
        raise
    manifest['state'] = 'prepared_and_verified'
    manifest['cartridge_default_on_next_boot'] = True
    os.sync()
    write_manifest(path, manifest)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--phase', choices=('combined', 'root', 'roms'), default='combined')
    parser.add_argument('--root', type=Path, help='Mounted OFFLINE ext4 root image')
    parser.add_argument('--roms', type=Path, help='Mounted ROMS partition')
    parser.add_argument('--bundle', type=Path, required=True, help='Verified CI device bundle /Cartridge')
    parser.add_argument('--backup', type=Path, required=True, help='Backup directory outside the card images')
    parser.add_argument('--app-path', choices=('/roms/Cartridge', '/roms2/Cartridge'),
                        default='/roms/Cartridge', help='Device path to Cartridge after boot')
    args = parser.parse_args()
    if args.phase == 'combined':
        if args.root is None or args.roms is None:
            parser.error('combined phase requires --root and --roms')
        result = prepare(args.root, args.roms, args.bundle, args.backup)
    elif args.phase == 'root':
        if args.root is None or args.roms is not None:
            parser.error('root phase requires --root and no --roms')
        result = prepare_root(args.root, args.bundle, args.backup, args.app_path)
    else:
        if args.roms is None or args.root is not None:
            parser.error('roms phase requires --roms and no --root')
        result = stage_roms(args.roms, args.bundle, args.backup)
    print(json.dumps({'state': result['state'], 'build_revision': result['build_revision'],
                      'cartridge_default_on_next_boot': result.get('cartridge_default_on_next_boot', False)},
                     indent=2))


if __name__ == '__main__':
    main()
