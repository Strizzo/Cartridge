#!/usr/bin/env python3
"""Finish an offline prepare by writing s2 and coordinating ROMS rollback.

The preparation step has already updated only Cartridge-owned files on the
selected card's mounted exFAT partition and modified an OFFLINE ext4 clone.
This stage validates that handoff, writes the clone to s2, and restores the
Cartridge-owned exFAT files if s2 was never changed or was fully rolled back.
It never formats BOOT or the games partition and does not eject the card.
"""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

from card_clone import CloneError, select_root
from card_inventory import InventoryError, inventory
from card_writeback import WritebackError, outside_card, writeback
from offline_prepare import managed_roms_path, restore_roms, sha256


class InstallError(RuntimeError):
    pass


def verify_prepared_root(image, preparation):
    """Inspect the offline ext4 image without mounting or modifying it."""
    command = shutil.which('debugfs') or '/opt/homebrew/opt/e2fsprogs/sbin/debugfs'
    if not Path(command).is_file():
        raise InstallError('debugfs is required to verify the prepared Linux image')
    app = preparation['app_path']
    if app not in {'/roms/Cartridge', '/roms2/Cartridge'}:
        raise InstallError('Preparation has an unsupported Cartridge install path')
    def read_file(name):
        result = subprocess.run([command, '-R', 'cat '+name, str(image)],
                                capture_output=True, timeout=30)
        if result.returncode != 0 or not result.stdout:
            raise InstallError('Prepared Linux image lacks '+name)
        return result.stdout
    dropin = read_file('/etc/systemd/system/emulationstation.service.d/99-cartridge-primary.conf')
    expected = ('# Managed by Cartridge primary session v1\n'
                '[Service]\nExecStart=\n'
                'ExecStart=/usr/bin/python3 /usr/local/lib/cartridge/cartridge-session.py'
                ' --cartridge-dir '+app+'\n').encode()
    if dropin != expected:
        raise InstallError('Prepared Linux image does not select the expected Cartridge session')
    session = read_file('/usr/local/lib/cartridge/cartridge-session.py')
    if hashlib.sha256(session).hexdigest() != preparation['session_sha256']:
        raise InstallError('Prepared Linux image has the wrong session supervisor')
    service = read_file('/etc/systemd/system/emulationstation.service').decode('utf-8', errors='replace')
    if (service.count('User=ark') != 1 or
            service.count('ExecStart=/usr/bin/emulationstation/emulationstation.sh') != 1):
        raise InstallError('Prepared Linux image lost the stock EmulationStation service')
    link = subprocess.run([command, '-R',
                           'stat /etc/systemd/system/multi-user.target.wants/emulationstation.service',
                           str(image)], capture_output=True, timeout=30)
    if (link.returncode != 0 or b'Type: symlink' not in link.stdout or
            not any(target in link.stdout for target in
                    (b'../emulationstation.service',
                     b'/etc/systemd/system/emulationstation.service'))):
        raise InstallError('Prepared Linux image lost the stock ES recovery enable link')


def check_handoff(record, roms, preparation_backup):
    parts = record['partitions']
    if len(parts) != 3 or not parts[2]['mounted'] or not parts[2]['mount_point']:
        raise InstallError('The selected card games partition must be mounted')
    roms = Path(roms)
    if roms.is_symlink() or roms.resolve(strict=True) != Path(parts[2]['mount_point']).resolve(strict=True):
        raise InstallError('ROMS mount does not belong to the selected card')
    preparation_backup = outside_card(preparation_backup, record, 'Preparation backup')
    if not preparation_backup.is_dir() or preparation_backup.is_symlink():
        raise InstallError('Preparation backup is missing or symlinked')
    manifest_path = preparation_backup/'manifest.json'
    if manifest_path.is_symlink() or not manifest_path.is_file():
        raise InstallError('Preparation manifest is missing or symlinked')
    manifest = json.loads(manifest_path.read_text())
    if (manifest.get('state') != 'prepared_and_verified' or
            manifest.get('cartridge_default_on_next_boot') is not True or
            manifest.get('games_and_saves_written') is not False or
            not isinstance(manifest.get('build_revision'), str) or
            not manifest['build_revision'] or
            manifest.get('app_path') not in {'/roms/Cartridge', '/roms2/Cartridge'} or
            not isinstance(manifest.get('session_sha256'), str) or
            len(manifest['session_sha256']) != 64):
        raise InstallError('Offline preparation has not completed')
    rows = manifest.get('files')
    if not isinstance(rows, list):
        raise InstallError('Preparation file list is invalid')
    seen = set()
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get('path'), str):
            raise InstallError('Preparation file list contains an invalid row')
        name = row['path']
        if name.startswith('root/'):
            continue
        if not name.startswith('roms/') or not managed_roms_path(name[5:]) or name in seen:
            raise InstallError('Preparation file list contains an unexpected ROMS path')
        seen.add(name)
        expected = row.get('installed_sha256')
        old = row.get('previous_sha256')
        target = roms/name[5:]
        if any(parent.is_symlink() for parent in target.parents
               if parent != roms and roms in parent.parents):
            raise InstallError('Prepared ROMS path has a symlink parent: '+name)
        if (not isinstance(expected, str) or len(expected) != 64 or
                target.is_symlink() or not target.is_file() or sha256(target) != expected):
            raise InstallError('Prepared ROMS file changed: '+name)
        if old is not None:
            previous = preparation_backup/'previous'/name
            if (not isinstance(old, str) or len(old) != 64 or
                    previous.is_symlink() or not previous.is_file() or sha256(previous) != old or
                    any(parent.is_symlink() for parent in previous.parents
                        if parent != preparation_backup and preparation_backup in parent.parents)):
                raise InstallError('Original ROMS backup is missing or changed: '+name)
    if 'roms/Cartridge/cartridge' not in seen or sha256(roms/'Cartridge/cartridge') != manifest.get('cartridge_sha256'):
        raise InstallError('Prepared Cartridge executable is missing or changed')
    return manifest


def root_safe_for_roms_rollback(journal_dir):
    path = Path(journal_dir)/'manifest.json'
    if not path.exists():
        return True
    if path.is_symlink():
        return False
    try:
        state = json.loads(path.read_text()).get('state')
    except (OSError, ValueError):
        return False
    return state in {'checking_target', 'rolled_back_and_verified'}


def finish_install(disk_id, fingerprint, original_backup, prepared_image,
                   prepared_sha256, preparation_backup, roms, journal_dir,
                   *, inventory_fn=inventory, target_override=None,
                   filesystem_check=None, require_mount=True, chunk_hook=None,
                   root_validator=None):
    record, _ = select_root(inventory_fn(), disk_id, fingerprint)
    preparation = check_handoff(record, roms, preparation_backup)
    if Path(journal_dir).exists() or Path(journal_dir).is_symlink():
        raise InstallError('A recovery journal already exists; inspect it before retrying')
    try:
        (root_validator or verify_prepared_root)(prepared_image, preparation)
        result = writeback(disk_id, fingerprint, original_backup, prepared_image,
                           prepared_sha256, journal_dir, inventory_fn=inventory_fn,
                           target_override=target_override,
                           filesystem_check=filesystem_check, chunk_hook=chunk_hook)
    except BaseException as exc:
        if not root_safe_for_roms_rollback(journal_dir):
            raise InstallError('Linux writeback is incomplete; Cartridge files remain on ROMS for recovery. '
                               f'Inspect {journal_dir} before booting the card') from exc
        try:
            restore_roms(roms, preparation_backup, require_mount=require_mount)
        except BaseException as rollback_exc:
            raise InstallError('Linux root is unchanged or restored, but ROMS rollback failed: '
                               +str(rollback_exc)) from exc
        raise InstallError('Linux writeback stopped; original root and Cartridge ROMS files were restored') from exc
    return {'state': 'installed_and_verified', 'disk': disk_id,
            'build_revision': preparation['build_revision'],
            'root_journal': str(journal_dir), 'root_sha256': result['prepared_sha256'],
            'games_and_saves_written': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--disk', required=True)
    parser.add_argument('--fingerprint', required=True)
    parser.add_argument('--original-backup', type=Path, required=True)
    parser.add_argument('--prepared-image', type=Path, required=True)
    parser.add_argument('--prepared-sha256', required=True)
    parser.add_argument('--preparation-backup', type=Path, required=True)
    parser.add_argument('--roms', type=Path, required=True)
    parser.add_argument('--journal', type=Path, required=True)
    args = parser.parse_args()
    try:
        result = finish_install(args.disk, args.fingerprint, args.original_backup,
                                args.prepared_image, args.prepared_sha256,
                                args.preparation_backup, args.roms, args.journal)
    except (InstallError, CloneError, InventoryError, WritebackError, OSError, ValueError) as exc:
        parser.exit(2, f'Offline install stopped: {exc}\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
