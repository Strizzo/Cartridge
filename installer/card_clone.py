#!/usr/bin/env python3
"""Make a verified, read-only clone of a selected card's Linux partition.

This is an installer building block, not the card writer. It never unmounts or
writes to the source device. A disk must first be selected in card_inventory.py.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess

from card_inventory import DISK_ID, inventory
from offline_prepare import write_manifest


class CloneError(RuntimeError):
    pass


def select_root(records, disk_id, fingerprint):
    matches = [record for record in records if record['identifier'] == disk_id]
    if len(matches) != 1 or matches[0]['inventory_fingerprint'] != fingerprint:
        raise CloneError('Selected card identity changed; run read-only inventory again')
    record = matches[0]
    if record['status'] != 'preserve_candidate':
        raise CloneError('The selected disk is not a compatible preserve candidate')
    parts = record['partitions']
    if (len(parts) != 3 or parts[1]['identifier'] != disk_id+'s2' or
            parts[1]['content'] != 'Linux' or parts[1]['mounted'] or
            parts[1]['device_size_bytes'] != parts[1]['size_bytes'] or
            not isinstance(parts[1]['size_bytes'], int) or parts[1]['size_bytes'] <= 0):
        raise CloneError('The Linux partition is absent, mounted, or changed')
    return record, parts[1]


def check_destination(destination, record, root_bytes):
    if destination.exists() or destination.is_symlink():
        raise CloneError('Clone destination must be a new directory')
    try:
        parent = destination.parent.resolve(strict=True)
    except (OSError, RuntimeError) as exc:
        raise CloneError('Clone destination parent does not exist') from exc
    if not parent.is_dir():
        raise CloneError('Clone destination parent is not a directory')
    for part in record['partitions']:
        mount = part.get('mount_point')
        if mount:
            mounted = Path(mount).resolve()
            if parent == mounted or mounted in parent.parents:
                raise CloneError('Clone destination is on the selected card')
    free = shutil.disk_usage(parent).free
    if free < root_bytes + 1024**3:
        raise CloneError('Clone destination needs the Linux partition size plus 1 GiB free')
    return parent/destination.name


def read_exact_hash(stream, count, output=None):
    digest = hashlib.sha256()
    remaining = count
    while remaining:
        block = stream.read(min(4 * 1024 * 1024, remaining))
        if not block:
            raise CloneError('Source ended before the expected partition size')
        digest.update(block)
        if output is not None:
            if output.write(block) != len(block):
                raise CloneError('Clone destination accepted only a partial write')
        remaining -= len(block)
    return digest.hexdigest()


def clone_root(disk_id, fingerprint, destination, *, inventory_fn=inventory,
               source_override=None, filesystem_check=None):
    if not DISK_ID.fullmatch(disk_id):
        raise CloneError('Select a whole disk, such as disk6')
    record, partition = select_root(inventory_fn(), disk_id, fingerprint)
    expected_bytes = partition['size_bytes']
    destination = check_destination(Path(destination), record, expected_bytes)
    source = Path(source_override) if source_override is not None else Path('/dev/r'+partition['identifier'])
    if source.is_symlink() or not source.exists():
        raise CloneError('Linux partition device is missing or symlinked')
    if source_override is None and not stat.S_ISCHR(source.stat().st_mode):
        raise CloneError('Expected a raw character device for the selected partition')
    if filesystem_check is None:
        command = shutil.which('e2fsck') or '/opt/homebrew/opt/e2fsprogs/sbin/e2fsck'
        if not Path(command).is_file():
            raise CloneError('e2fsck is required to verify the ext4 clone')
        filesystem_check = lambda path: subprocess.run([command, '-fn', str(path)],
                                                        check=True, capture_output=True, text=True)
    destination.mkdir(mode=0o700)
    partial = destination/'root.ext4.partial'
    completed = destination/'root.ext4'
    manifest = {
        'state': 'cloning', 'selected_disk': disk_id,
        'inventory_fingerprint': fingerprint,
        'source_partition': partition['identifier'],
        'source_bytes': expected_bytes, 'writes_to_card': False,
    }
    write_manifest(destination/'manifest.json', manifest)
    try:
        with source.open('rb', buffering=0) as raw, partial.open('xb') as output:
            first = read_exact_hash(raw, expected_bytes, output)
            output.flush()
            os.fsync(output.fileno())
        select_root(inventory_fn(), disk_id, fingerprint)
        with source.open('rb', buffering=0) as raw:
            second = read_exact_hash(raw, expected_bytes)
        if first != second:
            raise CloneError('Two source reads differed; media may be unstable')
        if partial.stat().st_size != expected_bytes:
            raise CloneError('Cloned partition has the wrong size')
        with partial.open('rb') as saved:
            third = read_exact_hash(saved, expected_bytes)
        if first != third:
            raise CloneError('Clone read-back differs from the source')
        filesystem_check(partial)
        select_root(inventory_fn(), disk_id, fingerprint)
        os.replace(partial, completed)
        manifest.update(state='verified', sha256=first, source_read_passes=2,
                        destination_readback_verified=True, ext4_check='clean')
        write_manifest(destination/'manifest.json', manifest)
        return manifest
    except BaseException as exc:
        manifest.update(state='incomplete', error=str(exc))
        write_manifest(destination/'manifest.json', manifest)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--disk', required=True, help='Explicit whole-disk identifier from card_inventory.py')
    parser.add_argument('--fingerprint', required=True, help='Inventory fingerprint shown for that card')
    parser.add_argument('--destination', type=Path, required=True, help='New backup directory outside the selected card')
    args = parser.parse_args()
    try:
        result = clone_root(args.disk, args.fingerprint, args.destination)
    except (CloneError, OSError, subprocess.CalledProcessError) as exc:
        parser.exit(2, f'Card clone stopped: {exc}\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
