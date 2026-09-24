#!/usr/bin/env python3
"""Verify, then flash one explicitly identified empty spare card from a virtual image.

This development tool never selects a card automatically. --preflight is read
only. --write replaces the entire selected spare card and ejects it only after
a complete SHA-256 readback. It must not be used on a card with games or saves.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import selectors
import stat
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'installer'))
from card_inventory import inventory  # noqa: E402
from offline_prepare import write_manifest  # noqa: E402

CHUNK = 4 * 1024 * 1024
ROOT_OFFSET = 128 * 1024 * 1024
EMPTY_ENTRIES = {'.Spotlight-V100', '.fseventsd', '.Trashes', '.TemporaryItems',
                 '.DS_Store', '.metadata_never_index', 'System Volume Information'}


def command(*args):
    return subprocess.run(args, check=True, capture_output=True, timeout=120)


def disk_info(name):
    return plistlib.loads(command('/usr/sbin/diskutil', 'info', '-plist', str(name)).stdout)


def check_target(args, *, mounted=True):
    records = inventory()
    matches = [row for row in records if row['identifier'] == args.disk]
    if len(matches) != 1:
        raise RuntimeError('Selected spare is absent or ambiguous')
    row = matches[0]
    part = row['partitions']
    if (row['inventory_fingerprint'] != args.fingerprint or
            row['status'] != 'unsupported_layout' or
            row['size_bytes'] != args.card_bytes or len(part) != 1 or
            part[0]['filesystem'] != 'exfat' or
            part[0]['volume_uuid'] != args.empty_uuid):
        raise RuntimeError('Selected card is not the recorded empty spare')
    whole = disk_info(args.disk)
    if (whole.get('DeviceIdentifier') != args.disk or
            whole.get('ParentWholeDisk') != args.disk or
            whole.get('WholeDisk') is not True or
            whole.get('VirtualOrPhysical') != 'Physical' or
            whole.get('Internal') is not False or
            whole.get('RemovableMedia') is not True or
            whole.get('Ejectable') is not True or
            whole.get('WritableMedia') is not True or
            whole.get('DeviceBlockSize') != 512 or
            whole.get('TotalSize') != args.card_bytes):
        raise RuntimeError('Target is not the recorded physical removable card')
    if mounted:
        mount = part[0]['mount_point']
        if not mount or not os.path.ismount(mount):
            raise RuntimeError('Empty card volume must be mounted for content inspection')
        entries = {path.name for path in Path(mount).iterdir()}
        if entries - EMPTY_ENTRIES:
            raise RuntimeError('Spare is no longer empty: ' + repr(sorted(entries - EMPTY_ENTRIES)))
    elif part[0]['mounted']:
        raise RuntimeError('Target remounted during raw verification')
    return row


def attach_source(args):
    image = Path(args.image)
    volume = Path(args.source_volume)
    if image.is_symlink() or not image.is_dir() or volume.is_symlink():
        raise RuntimeError('Image and source volume must be real directories')
    image, volume = image.resolve(strict=True), volume.resolve(strict=True)
    if volume not in image.parents:
        raise RuntimeError('Image must be inside the selected backup volume')
    volume_info = disk_info(volume)
    if (volume_info.get('VolumeUUID') != args.source_volume_uuid or
            volume_info.get('MountPoint') != str(volume) or
            volume_info.get('ParentWholeDisk') == args.disk):
        raise RuntimeError('Backup volume is absent, changed, or is the target card')
    report = json.loads((image.parent / 'firstboot-image-report.json').read_text())
    if (report.get('state') != 'virtual_firstboot_image_verified' or
            report.get('image') != str(image) or
            report.get('logical_bytes') != args.card_bytes or
            report.get('boot_sha256') != args.boot_sha256 or
            report.get('root_sha256') != args.root_sha256):
        raise RuntimeError('Virtual image report does not match the selected source')
    attached = plistlib.loads(command('/usr/bin/hdiutil', 'attach', '-nomount',
                                      '-noautofsck', '-readonly', '-plist', str(image)).stdout)
    candidates = []
    for entity in attached.get('system-entities', []):
        if entity.get('dev-entry'):
            detail = disk_info(entity['dev-entry'])
            if detail.get('WholeDisk') is True:
                candidates.append(detail)
    if (len(candidates) != 1 or candidates[0].get('VirtualOrPhysical') != 'Virtual' or
            candidates[0].get('WritableMedia') is not False or
            candidates[0].get('TotalSize') != args.card_bytes or
            candidates[0].get('DeviceBlockSize') != 512):
        raise RuntimeError('Source did not attach as one read-only virtual disk')
    return candidates[0]['DeviceNode'], report


def read_hash(device, offset, length):
    digest = hashlib.sha256()
    with open(device, 'rb', buffering=0) as stream:
        stream.seek(offset)
        while length:
            block = stream.read(min(CHUNK, length))
            if not block:
                raise RuntimeError('Short source or card read')
            digest.update(block)
            length -= len(block)
    return digest.hexdigest()


class MountGuard:
    def __init__(self, executable, disk, log):
        executable = Path(executable)
        if executable.is_symlink() or not executable.is_file():
            raise RuntimeError('Disk-specific mount guard executable is missing')
        self.log = open(log, 'a')
        self.process = subprocess.Popen([str(executable), disk], stdout=subprocess.PIPE,
                                        stderr=self.log, text=True)
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            if not selector.select(10) or self.process.stdout.readline().strip() != 'READY ' + disk:
                self.close()
                raise RuntimeError('Disk-specific mount guard did not become ready')
        self.check()

    def check(self):
        if self.process.poll() is not None:
            raise RuntimeError('Disk-specific mount guard exited')

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            self.process.wait(timeout=10)
        self.process.stdout.close()
        self.log.close()


def require_unmounted(disk):
    listing = plistlib.loads(command('/usr/sbin/diskutil', 'list', '-plist', disk).stdout)
    wholes = listing.get('AllDisksAndPartitions', [])
    if len(wholes) != 1 or wholes[0].get('DeviceIdentifier') != disk:
        raise RuntimeError('Target disk identity changed after unmount')
    for part in wholes[0].get('Partitions', []):
        if disk_info(part['DeviceIdentifier']).get('MountPoint'):
            raise RuntimeError('Target partition is mounted during raw I/O')


def copy_card(source, destination, size, report_path, guard):
    digest = hashlib.sha256()
    written = 0
    next_report = 8 * 1024**3
    with open(source, 'rb', buffering=0) as src, open(destination, 'r+b', buffering=0) as dst:
        while written < size:
            block = src.read(min(CHUNK, size - written))
            if not block:
                raise RuntimeError('Virtual image ended during card write')
            view = memoryview(block)
            while view:
                count = dst.write(view)
                if not count:
                    raise RuntimeError('Short card write')
                view = view[count:]
            digest.update(block)
            written += len(block)
            if written >= next_report:
                guard.check()
                write_manifest(report_path, {'state': 'writing_card', 'bytes_written': written,
                                             'card_bytes': size})
                print(f'Wrote {written / 1e9:.1f} of {size / 1e9:.1f} GB', flush=True)
                next_report += 8 * 1024**3
        dst.flush()
        os.fsync(dst.fileno())
    return digest.hexdigest()


def preflight(args):
    target = check_target(args)
    source = None
    try:
        source, image_report = attach_source(args)
        root_bytes = args.root_bytes
        print('Checking BOOT and Cartridge root in read-only virtual source...', flush=True)
        if (read_hash(source, 0, ROOT_OFFSET) != args.boot_sha256 or
                read_hash(source, ROOT_OFFSET, root_bytes) != args.root_sha256):
            raise RuntimeError('Virtual source BOOT or Cartridge root hash changed')
        roms = disk_info(source + 's3')
        if (roms.get('ParentWholeDisk') != source.removeprefix('/dev/') or
                roms.get('FilesystemType') != 'exfat' or
                roms.get('VolumeName') != 'EASYROMS'):
            raise RuntimeError('Virtual source lacks the expected empty ROMS filesystem')
        result = {'state': 'preflight_passed', 'target_disk': args.disk,
                  'target_empty_volume_uuid': args.empty_uuid,
                  'target_fingerprint': target['inventory_fingerprint'],
                  'card_bytes': args.card_bytes, 'source_image': str(Path(args.image).resolve()),
                  'source_boot_sha256': args.boot_sha256,
                  'source_root_sha256': args.root_sha256,
                  'build_revision': image_report['build_revision'], 'writes_performed': False}
        write_manifest(args.report_dir / 'spare-preflight.json', result)
        return result
    finally:
        if source is not None:
            command('/usr/bin/hdiutil', 'detach', source)


def flash(args):
    if os.geteuid() != 0:
        raise RuntimeError('Administrator access is required for --write')
    prior = json.loads((args.report_dir / 'spare-preflight.json').read_text())
    if (prior.get('state') != 'preflight_passed' or
            prior.get('target_disk') != args.disk or
            prior.get('target_fingerprint') != args.fingerprint or
            prior.get('target_empty_volume_uuid') != args.empty_uuid or
            prior.get('source_image') != str(Path(args.image).resolve())):
        raise RuntimeError('A matching read-only preflight is required')
    check_target(args)
    source = None
    guard = None
    report = args.report_dir / 'spare-write-report.json'
    try:
        source, _ = attach_source(args)
        if (read_hash(source, 0, ROOT_OFFSET) != args.boot_sha256 or
                read_hash(source, ROOT_OFFSET, args.root_bytes) != args.root_sha256):
            raise RuntimeError('Virtual source changed after preflight')
        guard = MountGuard(args.mount_guard, args.disk,
                           args.report_dir / 'mount-guard.log')
        command('/usr/sbin/diskutil', 'unmountDisk', 'force', args.disk)
        check_target(args, mounted=False)
        require_unmounted(args.disk)
        raw = Path('/dev/r' + args.disk)
        if not stat.S_ISCHR(raw.stat().st_mode):
            raise RuntimeError('Target raw path is not a character device')
        with raw.open('rb', buffering=0) as stream:
            first = stream.read(65536)
            stream.seek(args.card_bytes - 65536)
            last = stream.read(65536)
        for name, payload in (('spare-before-first64k.bin', first),
                              ('spare-before-last64k.bin', last)):
            with (args.report_dir / name).open('xb') as backup:
                backup.write(payload)
                backup.flush()
                os.fsync(backup.fileno())
        write_manifest(report, {'state': 'write_started', 'target_disk': args.disk,
                                'card_bytes': args.card_bytes})
        print('Writing only the identified empty spare card...', flush=True)
        expected_full = copy_card(source, raw, args.card_bytes, report, guard)
        guard.check()
        require_unmounted(args.disk)
        write_manifest(report, {'state': 'reading_back_card', 'target_disk': args.disk,
                                'expected_sha256': expected_full})
        print('Reading back the entire card for SHA-256 verification...', flush=True)
        actual_full = read_hash(raw, 0, args.card_bytes)
        if actual_full != expected_full:
            raise RuntimeError('Complete card readback checksum differs; do not boot')
        guard.check()
        require_unmounted(args.disk)
        command('/bin/sync')
        command('/usr/sbin/diskutil', 'eject', args.disk)
        result = {'state': 'verified_and_ejected', 'target_disk': args.disk,
                  'card_bytes': args.card_bytes, 'full_sha256': actual_full,
                  'boot_sha256': args.boot_sha256, 'root_sha256': args.root_sha256,
                  'build_revision': prior['build_revision']}
        write_manifest(report, result)
        return result
    except BaseException as exc:
        write_manifest(report, {'state': 'incomplete_do_not_boot', 'target_disk': args.disk,
                                'error': str(exc)})
        raise
    finally:
        if guard is not None:
            guard.close()
        if source is not None:
            command('/usr/bin/hdiutil', 'detach', source)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    phase = parser.add_mutually_exclusive_group(required=True)
    phase.add_argument('--preflight', action='store_true')
    phase.add_argument('--write', action='store_true')
    parser.add_argument('--disk', required=True)
    parser.add_argument('--fingerprint', required=True)
    parser.add_argument('--empty-uuid', required=True)
    parser.add_argument('--image', type=Path, required=True)
    parser.add_argument('--source-volume', type=Path, required=True)
    parser.add_argument('--source-volume-uuid', required=True)
    parser.add_argument('--boot-sha256', required=True)
    parser.add_argument('--root-sha256', required=True)
    parser.add_argument('--root-bytes', type=int, required=True)
    parser.add_argument('--card-bytes', type=int, required=True)
    parser.add_argument('--report-dir', type=Path, required=True)
    parser.add_argument('--mount-guard', type=Path,
                        help='macOS disk-specific mount veto executable, required for --write')
    args = parser.parse_args()
    try:
        args.report_dir = args.report_dir.resolve(strict=True)
        if (args.report_dir.is_symlink() or args.report_dir.stat().st_dev ==
                Path('/Volumes/Untitled').stat().st_dev):
            raise RuntimeError('Reports must be outside the spare card')
        if args.write and args.mount_guard is None:
            raise RuntimeError('--write requires --mount-guard')
        result = preflight(args) if args.preflight else flash(args)
        print(json.dumps(result, indent=2))
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
        parser.exit(2, f'Spare-card trial stopped: {exc}\n')


if __name__ == '__main__':
    main()
