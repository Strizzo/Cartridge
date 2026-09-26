#!/usr/bin/env python3
"""Guarded, reversible update of the proven CartridgeOS spare card.

Changes only the Cartridge executable, selector artwork and BOOT splash. The
Linux root, partition table, games and saves are never opened for writing.
Requires a separately verified backup and an explicit read-only preflight.
"""

import argparse
import json
import os
from pathlib import Path
import struct
import subprocess

from card_inventory import inventory
from offline_prepare import atomic_copy, sha256, write_manifest


CHANGES = (
    ('cartridge', 'Cartridge/cartridge', 'Cartridge/cartridge'),
    ('selector', 'Cartridge/assets/boot_logo.png', 'Cartridge/assets/boot_logo.png'),
    ('splash', 'logo.bmp', 'Cartridge/assets/logo.bmp'),
)
SPARE_ENTRIES = {'Cartridge', 'tools', '._Cartridge', '._tools', '.Spotlight-V100',
                 '.fseventsd', '.Trashes', '.TemporaryItems', '.DS_Store'}


def checked_regular(path):
    if path.is_symlink() or not path.is_file():
        raise RuntimeError(f'Expected an ordinary file: {path}')
    return path


def read_backup(backup):
    if backup.is_symlink() or not backup.is_dir():
        raise RuntimeError('Backup directory is missing or linked')
    manifest = json.loads(checked_regular(backup / 'manifest.json').read_text())
    if (manifest.get('state') != 'verified_backup' or
            manifest.get('backup_path') != str(backup) or
            manifest.get('disk') is None or
            manifest.get('files', {}).get('cartridge') is None):
        raise RuntimeError('Backup manifest is not a verified spare-card backup')
    for _, old, _ in CHANGES:
        path = backup / old
        expected = manifest['logo_sha256'] if old == 'logo.bmp' else \
            manifest['files'].get(old.removeprefix('Cartridge/'))
        if not expected or sha256(checked_regular(path)) != expected:
            raise RuntimeError(f'Backup file differs: {old}')
    return manifest


def checked_card(manifest):
    matches = [row for row in inventory() if row['identifier'] == manifest['disk']]
    if len(matches) != 1:
        raise RuntimeError('The recorded spare card is absent')
    row = matches[0]
    parts = row['partitions']
    if (row['status'] != 'preserve_candidate' or
            row['inventory_fingerprint'] != manifest['fingerprint'] or
            row['size_bytes'] != manifest['card_bytes'] or
            len(parts) != 3 or
            [parts[0]['volume_uuid'], parts[2]['volume_uuid']] != manifest['volume_uuids'] or
            (parts[0]['mount_point'], parts[2]['mount_point']) !=
            ('/Volumes/BOOT', '/Volumes/EASYROMS')):
        raise RuntimeError('Inserted card is not the recorded spare, or is not mounted as expected')
    boot, roms = Path(parts[0]['mount_point']), Path(parts[2]['mount_point'])
    if {path.name for path in roms.iterdir()} - SPARE_ENTRIES:
        raise RuntimeError('This card contains other ROMS data; use the preserve workflow')
    for _, old, _ in CHANGES:
        current = boot / old if old == 'logo.bmp' else roms / old
        expected = manifest['logo_sha256'] if old == 'logo.bmp' else \
            manifest['files'].get(old.removeprefix('Cartridge/'))
        if not expected or sha256(checked_regular(current)) != expected:
            raise RuntimeError(f'Card file changed since backup: {old}')
    if sha256(checked_regular(boot / 'logo.bmp.bak')) != manifest['logo_sha256']:
        raise RuntimeError('Stock BOOT logo backup differs')
    return row, boot, roms


def checked_bundle(bundle):
    if bundle.is_symlink() or not bundle.is_dir():
        raise RuntimeError('Device bundle is missing or linked')
    sources = {}
    for name, _, source in CHANGES:
        path = checked_regular(bundle / source)
        sources[name] = {'path': str(path), 'sha256': sha256(path)}
    binary = (bundle / CHANGES[0][2]).read_bytes()[:20]
    if binary[:4] != b'\x7fELF' or binary[4] != 2 or \
            struct.unpack_from('<H', binary, 18)[0] != 183:
        raise RuntimeError('Bundle executable is not AArch64 Linux ELF')
    with (bundle / CHANGES[2][2]).open('rb') as stream:
        header = stream.read(54)
    if (len(header) != 54 or header[:2] != b'BM' or
            struct.unpack_from('<ii', header, 18) != (720, 720) or
            struct.unpack_from('<HHI', header, 26) != (1, 24, 0)):
        raise RuntimeError('Bundle BOOT splash is not an uncompressed 720x720 BMP')
    return sources


def preflight(args):
    backup = args.backup.resolve(strict=True)
    bundle = args.bundle.resolve(strict=True)
    manifest = read_backup(backup)
    row, _, _ = checked_card(manifest)
    sources = checked_bundle(bundle)
    if backup.stat().st_dev in {Path('/Volumes/BOOT').stat().st_dev,
                                Path('/Volumes/EASYROMS').stat().st_dev}:
        raise RuntimeError('Verified backup must be outside the spare card')
    result = {'state': 'preflight_passed', 'disk': row['identifier'],
              'fingerprint': row['inventory_fingerprint'], 'backup': str(backup),
              'bundle': str(bundle), 'sources': sources,
              'old_binary_sha256': manifest['files']['cartridge'],
              'old_splash_sha256': manifest['logo_sha256']}
    write_manifest(backup / 'update-preflight.json', result)
    return result


def verified_volume(part):
    subprocess.run(['/usr/sbin/diskutil', 'unmount', 'force', part], check=True)
    subprocess.run(['/usr/sbin/diskutil', 'verifyVolume', part], check=True)


def apply(args):
    backup = args.backup.resolve(strict=True)
    prior = json.loads(checked_regular(backup / 'update-preflight.json').read_text())
    current = preflight(args)
    if (prior.get('state') != 'preflight_passed' or
            {key: prior.get(key) for key in current} != current):
        raise RuntimeError('Matching read-only preflight is required')
    manifest = read_backup(backup)
    _, boot, roms = checked_card(manifest)
    report_path = backup / 'update-result.json'
    changed = []
    write_manifest(report_path, {'state': 'write_started', 'changed': changed})
    try:
        for name, old, source in CHANGES:
            destination = boot / old if old == 'logo.bmp' else roms / old
            atomic_copy(args.bundle.resolve() / source, destination, preserve_mode=False)
            changed.append(name)
            write_manifest(report_path, {'state': 'writing', 'changed': changed})
            if sha256(destination) != current['sources'][name]['sha256']:
                raise RuntimeError(f'Card readback differs: {name}')
        subprocess.run(['/bin/sync'], check=True)
    except BaseException as exc:
        failures = []
        for name, old, _ in CHANGES:
            if name not in changed:
                continue
            destination = boot / old if old == 'logo.bmp' else roms / old
            try:
                atomic_copy(backup / old, destination, preserve_mode=False)
                expected = manifest['logo_sha256'] if old == 'logo.bmp' else \
                    manifest['files'][old.removeprefix('Cartridge/')]
                if sha256(destination) != expected:
                    raise RuntimeError('rollback readback differs')
            except BaseException as restore_error:
                failures.append(f'{name}: {restore_error}')
        state = 'rollback_verified' if not failures else 'incomplete_do_not_boot'
        write_manifest(report_path, {'state': state, 'error': str(exc),
                                     'rollback_failures': failures})
        raise
    try:
        verified_volume(manifest['disk'] + 's3')
        verified_volume(manifest['disk'] + 's1')
        subprocess.run(['/usr/sbin/diskutil', 'eject', manifest['disk']], check=True)
    except BaseException as exc:
        write_manifest(report_path, {'state': 'written_not_ejected', 'error': str(exc),
                                     'new_sha256': current['sources']})
        raise
    result = {'state': 'verified_and_ejected', 'disk': manifest['disk'],
              'new_sha256': current['sources'], 'backup': str(backup)}
    write_manifest(report_path, result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--preflight', action='store_true')
    mode.add_argument('--apply', action='store_true')
    parser.add_argument('--backup', type=Path, required=True)
    parser.add_argument('--bundle', type=Path, required=True)
    args = parser.parse_args()
    try:
        result = preflight(args) if args.preflight else apply(args)
        print(json.dumps(result, indent=2))
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
        parser.exit(2, f'Spare-card update stopped: {exc}\n')


if __name__ == '__main__':
    main()
