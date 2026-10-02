#!/usr/bin/env python3
"""Read back a flashed spare card and compare every byte with its virtual image.

The card is opened read-only. This tool writes only a JSON report on the source
SSD and ejects the card only after a complete matching readback.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import stat
import subprocess
import sys


MODULE_PATH = Path(__file__).with_name('flash-firstboot-trial.py')
SPEC = importlib.util.spec_from_file_location('flash_firstboot_trial', MODULE_PATH)
flash = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(flash)

CHUNK = 4 * 1024 * 1024
PROGRESS = 8 * 1024**3


def disk_layout(name):
    result = flash.command('/usr/sbin/diskutil', 'list', '-plist', name)
    listing = plistlib.loads(result.stdout).get('AllDisksAndPartitions', [])
    if len(listing) != 1 or listing[0].get('DeviceIdentifier') != name:
        raise RuntimeError(f'Could not verify partition layout for {name}')
    return listing[0]


def check_card(args, source_disk):
    whole = flash.disk_info(args.disk)
    if (whole.get('DeviceIdentifier') != args.disk or
            whole.get('ParentWholeDisk') != args.disk or
            whole.get('WholeDisk') is not True or
            whole.get('VirtualOrPhysical') != 'Physical' or
            whole.get('Internal') is not False or
            whole.get('RemovableMedia') is not True or
            whole.get('Ejectable') is not True or
            whole.get('DeviceBlockSize') != 512 or
            whole.get('TotalSize') != args.card_bytes):
        raise RuntimeError('Selected disk is not the expected physical spare card')
    card = disk_layout(args.disk)
    source = disk_layout(source_disk)
    if (card.get('Content') != 'FDisk_partition_scheme' or
            card.get('Size') != args.card_bytes or
            source.get('Size') != args.card_bytes):
        raise RuntimeError('Card or source disk size/partition map changed')
    card_parts, source_parts = card.get('Partitions', []), source.get('Partitions', [])
    if len(card_parts) != 3 or len(source_parts) != 3:
        raise RuntimeError('Expected three partitions on card and source image')
    for index, (part, expected) in enumerate(zip(card_parts, source_parts), 1):
        if (part.get('DeviceIdentifier') != f'{args.disk}s{index}' or
                expected.get('DeviceIdentifier') != f'{source_disk}s{index}' or
                part.get('Content') != expected.get('Content') or
                part.get('Size') != expected.get('Size')):
            raise RuntimeError(f'Card partition {index} differs from source image')
    raw = Path('/dev/r' + args.disk)
    if not stat.S_ISCHR(raw.stat().st_mode):
        raise RuntimeError('Selected card has no raw character device')
    return raw


def compare(source, card, length, root_bytes):
    full_source = hashlib.sha256()
    full_card = hashlib.sha256()
    boot_source = hashlib.sha256()
    boot_card = hashlib.sha256()
    root_source = hashlib.sha256()
    root_card = hashlib.sha256()
    mismatch_chunks = 0
    first_mismatch = None
    read = 0
    next_progress = PROGRESS
    boot_end = flash.ROOT_OFFSET
    root_end = boot_end + root_bytes
    with open(source, 'rb', buffering=0) as left, open(card, 'rb', buffering=0) as right:
        while read < length:
            size = min(CHUNK, length - read, boot_end - read if read < boot_end else CHUNK,
                       root_end - read if read < root_end else CHUNK)
            expected, actual = left.read(size), right.read(size)
            if len(expected) != size or len(actual) != size:
                raise RuntimeError(f'Short read at byte {read}; card is not verified')
            full_source.update(expected)
            full_card.update(actual)
            if read < boot_end:
                boot_source.update(expected)
                boot_card.update(actual)
            elif read < root_end:
                root_source.update(expected)
                root_card.update(actual)
            if expected != actual:
                mismatch_chunks += 1
                if first_mismatch is None:
                    first_mismatch = read + next(i for i, (a, b) in enumerate(zip(expected, actual))
                                                 if a != b)
            read += size
            if read >= next_progress:
                print(f'Read back {read / 1e9:.1f} of {length / 1e9:.1f} GB', flush=True)
                next_progress += PROGRESS
    return {
        'bytes_compared': read,
        'source_full_sha256': full_source.hexdigest(),
        'card_full_sha256': full_card.hexdigest(),
        'source_boot_sha256': boot_source.hexdigest(),
        'card_boot_sha256': boot_card.hexdigest(),
        'source_root_sha256': root_source.hexdigest(),
        'card_root_sha256': root_card.hexdigest(),
        'mismatch_chunks': mismatch_chunks,
        'first_mismatch_byte': first_mismatch,
    }


def verify(args):
    if os.geteuid() != 0:
        raise RuntimeError('Administrator access is required to read the raw card')
    report = args.report_dir / 'spare-readback-report.json'
    source = None
    try:
        if args.report_dir.is_symlink() or args.report_dir.resolve() != Path(args.image).resolve().parent:
            raise RuntimeError('Report directory must be the virtual image directory')
        source, image_report = flash.attach_source(args)
        source_disk = Path(source).name
        raw = check_card(args, source_disk)
        flash.command('/usr/sbin/diskutil', 'unmountDisk', 'force', args.disk)
        raw = check_card(args, source_disk)
        flash.require_unmounted(args.disk)
        print('Comparing the entire card with the read-only virtual image...', flush=True)
        result = compare(source, raw, args.card_bytes, args.root_bytes)
        result.update({'target_disk': args.disk,
                       'source_image': str(Path(args.image).resolve()),
                       'build_revision': image_report['build_revision']})
        source_valid = (result['source_boot_sha256'] == args.boot_sha256 and
                        result['source_root_sha256'] == args.root_sha256)
        match = (result['mismatch_chunks'] == 0 and
                 result['source_full_sha256'] == result['card_full_sha256'] and
                 result['card_boot_sha256'] == args.boot_sha256 and
                 result['card_root_sha256'] == args.root_sha256)
        result['state'] = 'verified_and_ejected' if source_valid and match else 'mismatch_do_not_boot'
        flash.require_unmounted(args.disk)
        if result['state'] == 'verified_and_ejected':
            flash.command('/bin/sync')
            flash.command('/usr/sbin/diskutil', 'eject', args.disk)
        flash.write_manifest(report, result)
        return result
    except BaseException as exc:
        flash.write_manifest(report, {'state': 'incomplete_do_not_boot',
                                      'target_disk': args.disk, 'error': str(exc)})
        raise
    finally:
        if source is not None:
            flash.command('/usr/bin/hdiutil', 'detach', source)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--disk', required=True)
    parser.add_argument('--image', type=Path, required=True)
    parser.add_argument('--source-volume', type=Path, required=True)
    parser.add_argument('--source-volume-uuid', required=True)
    parser.add_argument('--boot-sha256', required=True)
    parser.add_argument('--root-sha256', required=True)
    parser.add_argument('--root-bytes', type=int, required=True)
    parser.add_argument('--card-bytes', type=int, required=True)
    parser.add_argument('--report-dir', type=Path, required=True)
    args = parser.parse_args()
    try:
        result = verify(args)
        print(json.dumps(result, indent=2))
        if result['state'] != 'verified_and_ejected':
            parser.exit(2, 'Card differs from virtual image; do not boot it.\n')
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
        parser.exit(2, f'Spare-card readback stopped: {exc}\n')


if __name__ == '__main__':
    main()
