#!/usr/bin/env python3
"""Rehearse offline setup on a verified COPY of a stock ext4 system image.

Run only inside the disposable ARM compatibility VM. The ROMS partition is a
small synthetic exFAT image; no real games partition or physical card is used.
The supplied root image is intentionally modified, so pass a throwaway clone.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess


ROOT = Path(__file__).resolve().parents[2]
BASE = Path('/tmp/cartridge-vm/stock-root-check')
BUNDLE = ROOT/'bundle/Cartridge'


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(4 * 1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def check_stock_tree(root):
    spec = importlib.util.spec_from_file_location('offline_prepare', ROOT/'installer/offline_prepare.py')
    converter = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(converter)
    converter.check_stock(root, '/roms/Cartridge')
    fstab = (root/'etc/fstab').read_text()
    if not any('/roms' in line.split() and 'exfat' in line.split()
               for line in fstab.splitlines() if line.strip() and not line.lstrip().startswith('#')):
        raise RuntimeError('The stock system does not mount exFAT games at /roms')
    recovery = root/'etc/systemd/system/emulationstation.service.d/90-cartridge-recovery.conf'
    if recovery.exists() and 'ExecStart=' in recovery.read_text():
        raise RuntimeError('Existing recovery override changes the startup executable')
    return converter


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root-image', type=Path, required=True, help='Disposable ext4 image copy; this file WILL be modified')
    parser.add_argument('--expected-sha256', required=True, help='Expected SHA-256 of the untouched image copy')
    args = parser.parse_args()
    if args.root_image.is_symlink():
        raise RuntimeError('Refusing a symlinked root image')
    image = args.root_image.resolve()
    if platform.machine() != 'aarch64' or os.geteuid() != 0 or not Path('/etc/cartridge-compat-vm').exists():
        raise RuntimeError('This rehearsal is restricted to the disposable ARM VM')
    if not image.is_file() or not re.fullmatch(r'[0-9a-fA-F]{64}', args.expected_sha256):
        raise RuntimeError('Expected a regular, disposable ext4 image and a SHA-256')
    if not (BUNDLE/'dev/build-revision').is_file():
        raise RuntimeError('CI device bundle is missing')
    if BASE.exists():
        raise RuntimeError('Previous stock-root rehearsal data exists; inspect it before retrying')
    BASE.mkdir(parents=True)
    # No mount, write or conversion occurs before the original image copy is
    # checked against the recovery record supplied by the caller.
    if digest(image) != args.expected_sha256.lower():
        raise RuntimeError('Root clone SHA-256 differs from the verified backup')
    run('e2fsck', '-fn', str(image), stdout=subprocess.DEVNULL)
    mounted_root = BASE/'root'
    mounted_root.mkdir()
    root_active = roms_active = False
    roms_image = BASE/'roms.exfat'
    game_hashes = {}
    try:
        run('mount', '-t', 'ext4', '-o', 'loop,ro,noload', str(image), str(mounted_root))
        root_active = True
        converter = check_stock_tree(mounted_root)
        stock_service = (mounted_root/'etc/systemd/system/emulationstation.service').read_bytes()
        run('umount', str(mounted_root))
        root_active = False

        with roms_image.open('wb') as stream:
            stream.truncate(96 * 1024 * 1024)
        run('mkfs.exfat', str(roms_image), stdout=subprocess.DEVNULL)
        run('mount', '-t', 'ext4', '-o', 'loop', str(image), str(mounted_root))
        root_active = True
        mounted_roms = mounted_root/'roms'
        if not mounted_roms.is_dir():
            raise RuntimeError('The stock root lacks the /roms mountpoint')
        run('mount', '-t', 'exfat-fuse', '-o', 'loop', str(roms_image), str(mounted_roms))
        roms_active = True
        app = mounted_roms/'Cartridge'
        (app/'assets/fonts').mkdir(parents=True)
        (app/'cartridge').write_bytes(b'previous Cartridge executable')
        (app/'ssh').mkdir()
        key = app/'ssh/id_ed25519'
        key.write_bytes(b'private-key-fixture')
        (mounted_roms/'tools').mkdir()
        files = {
            'game': mounted_roms/'psx/fixture-game.chd',
            'save': mounted_roms/'psx/fixture-game.srm',
            'gamelist': mounted_roms/'psx/gamelist.xml',
        }
        files['game'].parent.mkdir()
        for name, path in files.items():
            path.write_bytes(('unchanged '+name).encode())
        game_hashes = {name: digest(path) for name, path in files.items()}

        result = converter.prepare(mounted_root, mounted_roms, BUNDLE, BASE/'previous-installation')
        if result['state'] != 'prepared_and_verified' or not result['cartridge_default_on_next_boot']:
            raise RuntimeError('Offline converter did not prepare Cartridge startup')
        if (mounted_root/'etc/systemd/system/emulationstation.service').read_bytes() != stock_service:
            raise RuntimeError('Stock EmulationStation service was changed')
        if digest(app/'cartridge') != digest(BUNDLE/'cartridge'):
            raise RuntimeError('Installed Cartridge executable differs from the CI bundle')
        # Test the stock dynamic loader and libraries from its own ext4 system.
        # exfat-fuse may impose host-only execute restrictions unlike the device.
        compat_executable = mounted_root/'tmp/cartridge-compat-version'
        if compat_executable.exists():
            raise RuntimeError('Refusing to overwrite an existing compatibility executable')
        shutil.copy2(BUNDLE/'cartridge', compat_executable)
        try:
            version = run('chroot', str(mounted_root), '/tmp/cartridge-compat-version',
                          '--version', capture_output=True, timeout=15)
        finally:
            compat_executable.unlink()
        if not version.stdout.startswith('cartridge '):
            raise RuntimeError('The installed ARM executable did not start under stock libraries')
        if {name: digest(path) for name, path in files.items()} != game_hashes:
            raise RuntimeError('The converter changed a game, save or gamelist')
        if key.read_bytes() != b'private-key-fixture':
            raise RuntimeError('The converter changed a user key')
        if not (mounted_root/'etc/systemd/system/multi-user.target.wants/emulationstation.service').is_symlink():
            raise RuntimeError('The EmulationStation recovery enable link was removed')
    finally:
        if roms_active:
            run('umount', str(mounted_root/'roms'))
        if root_active:
            run('umount', str(mounted_root))
    run('e2fsck', '-fn', str(image), stdout=subprocess.DEVNULL)
    run('fsck.exfat', '-n', str(roms_image), stdout=subprocess.DEVNULL)
    report = {
        'state': 'prepared_and_verified',
        'source': 'verified_stock_root_clone',
        'root_sha256_before': args.expected_sha256.lower(),
        'root_fstype': 'ext4',
        'roms_fstype': 'exfat-fuse fixture',
        'build_revision': (BUNDLE/'dev/build-revision').read_text().strip(),
        'cartridge_default_on_next_boot': True,
        'stock_es_service_preserved': True,
        'stock_userspace_binary_launch': version.stdout.strip(),
        'game_save_and_gamelist_sha256': game_hashes,
        'source_card_or_real_games_partition_used': False,
    }
    (ROOT/'results/stock-root.json').write_text(json.dumps(report, indent=2)+'\n')
    print('STOCK ROOT CHECK PASSED: actual recovered Linux system accepts offline Cartridge setup')


if __name__ == '__main__':
    main()
