#!/usr/bin/env python3
"""Build a disposable Cartridge-first R36S Plus card image from verified inputs.

This is a hardware-test fixture, not a redistributable operating-system image.
It writes only a new sparsebundle on the selected workspace volume; it never
opens a physical card. The ROMS partition contains Cartridge and no games.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import stat
import struct
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'installer'))
from card_writeback import default_fsck  # noqa: E402
from install_transaction import verify_prepared_root  # noqa: E402
from offline_prepare import bundle_digest, sha256, stage_roms, write_manifest  # noqa: E402

CHUNK = 4 * 1024 * 1024
BOOT_BYTES = 128 * 1024 * 1024


def command(*args, capture=True):
    return subprocess.run(args, check=True, capture_output=capture, text=False, timeout=300)


def info(device):
    return plistlib.loads(command('/usr/sbin/diskutil', 'info', '-plist', str(device)).stdout)


def checked_file(path, expected):
    path = Path(path)
    if path.is_symlink() or not path.is_file() or not stat.S_ISREG(path.stat().st_mode):
        raise RuntimeError('Input must be an ordinary file: ' + str(path))
    path = path.resolve(strict=True)
    if sha256(path) != expected:
        raise RuntimeError('Input checksum differs: ' + str(path))
    return path


def checked_boot_logo(path):
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise RuntimeError('Boot logo must be an ordinary file')
    with path.open('rb') as stream:
        header = stream.read(54)
    if (len(header) != 54 or header[:2] != b'BM' or
            struct.unpack_from('<I', header, 2)[0] != path.stat().st_size or
            struct.unpack_from('<I', header, 10)[0] != 54 or
            struct.unpack_from('<I', header, 14)[0] != 40 or
            struct.unpack_from('<ii', header, 18) != (720, 720) or
            struct.unpack_from('<HHI', header, 26) != (1, 24, 0)):
        raise RuntimeError('Boot logo must be an uncompressed 720x720 24-bit BMP')
    return path.resolve(strict=True)


def partition(prefix, index):
    off = 446 + 16 * (index - 1)
    return prefix[off + 4], *struct.unpack_from('<II', prefix, off + 8)


def validate_layout(prefix, root_bytes, card_bytes):
    if (len(prefix) != BOOT_BYTES or prefix[510:512] != b'\x55\xaa' or
            root_bytes % 512 or card_bytes % 512):
        raise RuntimeError('Boot prefix, root, or card sector geometry is invalid')
    p1, p2, p3 = (partition(prefix, index) for index in (1, 2, 3))
    if (p1[0] not in (0x0b, 0x0c) or p2 != (0x83, BOOT_BYTES // 512, root_bytes // 512) or
            p3[0] != 0x07 or p3[1] * 512 < BOOT_BYTES + root_bytes or
            p3[1] % 2048 or (p3[1] + p3[2]) * 512 != card_bytes):
        raise RuntimeError('MBR does not describe the expected BOOT/root/ROMS layout')
    return p3[1] * 512, p3[2] * 512


def attach(image, card_bytes):
    result = command('/usr/bin/hdiutil', 'attach', '-nomount', '-noautofsck',
                     '-plist', str(image))
    entities = plistlib.loads(result.stdout).get('system-entities', [])
    whole = []
    for row in entities:
        if row.get('dev-entry'):
            detail = info(row['dev-entry'])
            if detail.get('WholeDisk') is True:
                whole.append(detail)
    if len(whole) != 1 or whole[0].get('VirtualOrPhysical') != 'Virtual' or \
            whole[0].get('TotalSize') != card_bytes or whole[0].get('DeviceBlockSize') != 512:
        raise RuntimeError('Created image did not attach as one virtual disk')
    return whole[0]['DeviceNode']


def copy_to_device(source, device, position, length):
    count = 0
    with source.open('rb') as src, open(device, 'r+b', buffering=0) as dst:
        dst.seek(position)
        while count < length:
            block = src.read(min(CHUNK, length - count))
            if not block:
                raise RuntimeError('Source ended during virtual-image write')
            view = memoryview(block)
            while view:
                n = dst.write(view)
                if not n:
                    raise RuntimeError('Short virtual-image write')
                view = view[n:]
            count += len(block)
            if count % (1024**3) < CHUNK:
                print(f'Wrote {count / 1024**3:.1f} GiB of {length / 1024**3:.1f} GiB',
                      flush=True)
        dst.flush()
        os.fsync(dst.fileno())
    if count != length:
        raise RuntimeError('Virtual-image write length differs')


def device_hash(device, position, length):
    digest = hashlib.sha256()
    with open(device, 'rb', buffering=0) as src:
        src.seek(position)
        while length:
            block = src.read(min(CHUNK, length))
            if not block:
                raise RuntimeError('Virtual-image readback ended early')
            digest.update(block)
            length -= len(block)
    return digest.hexdigest()


def install_boot_logo(device, logo):
    """Replace only the stock splash file inside the virtual BOOT partition."""
    part = device + 's1'
    with tempfile.TemporaryDirectory(prefix='cartridge-spare-boot-') as mount:
        command('/usr/sbin/diskutil', 'mount', '-mountPoint', mount, part)
        try:
            boot = Path(mount)
            current = boot / 'logo.bmp'
            if not current.is_file() or current.is_symlink():
                raise RuntimeError('Stock BOOT partition has no ordinary logo.bmp')
            stock_hash = sha256(current)
            saved = boot / 'logo.bmp.stock'
            if saved.exists():
                raise RuntimeError('Stock BOOT already has a logo backup')
            shutil.copyfile(current, saved)
            if sha256(saved) != stock_hash:
                raise RuntimeError('Stock boot logo backup did not verify')
            shutil.copyfile(logo, current)
            if sha256(current) != sha256(logo):
                raise RuntimeError('Cartridge boot logo did not verify on virtual BOOT')
            return stock_hash
        finally:
            command('/usr/sbin/diskutil', 'unmount', part)


def build(prefix, prefix_hash, prepared_root, root_hash, backup, bundle,
          output, card_bytes, boot_logo):
    prefix = checked_file(prefix, prefix_hash)
    boot_logo = checked_boot_logo(boot_logo)
    prepared_root = checked_file(prepared_root, root_hash)
    backup, bundle, output = Path(backup), Path(bundle), Path(output)
    if backup.is_symlink() or bundle.is_symlink() or output.exists() or output.is_symlink():
        raise RuntimeError('Backup/bundle must be real directories and output must be new')
    backup, bundle = backup.resolve(strict=True), bundle.resolve(strict=True)
    output_parent = output.parent.resolve(strict=True)
    if not backup.is_dir() or not bundle.is_dir() or not output.is_absolute():
        raise RuntimeError('Use existing backup/bundle and an absolute output path')
    if not str(output).endswith('.sparsebundle'):
        raise RuntimeError('Output must end in .sparsebundle')
    if prepared_root.stat().st_dev != output_parent.stat().st_dev:
        raise RuntimeError('Prepared root and virtual image must use the same workspace volume')
    if shutil.disk_usage(output_parent).free < prepared_root.stat().st_size + 2 * 1024**3:
        raise RuntimeError('Workspace needs root-image size plus 2 GiB free')
    prefix_data = prefix.read_bytes()
    games_offset, games_bytes = validate_layout(prefix_data, prepared_root.stat().st_size,
                                                 card_bytes)
    default_fsck(prepared_root)
    manifest = json.loads((backup / 'manifest.json').read_text())
    if (manifest.get('state') != 'root_prepared_and_verified' or
            manifest.get('bundle_sha256') != bundle_digest(bundle) or
            manifest.get('app_path') != '/roms/Cartridge'):
        raise RuntimeError('Root preparation and CI bundle do not match')
    verify_prepared_root(prepared_root, manifest)
    command('/usr/bin/hdiutil', 'create', '-sectors', str(card_bytes // 512),
            '-type', 'SPARSEBUNDLE', '-layout', 'NONE', str(output))
    device = None
    try:
        device = attach(output, card_bytes)
        print('Writing verified BOOT and Cartridge root to virtual disk...', flush=True)
        copy_to_device(prefix, device, 0, BOOT_BYTES)
        copy_to_device(prepared_root, device, BOOT_BYTES, prepared_root.stat().st_size)
        if device_hash(device, 0, BOOT_BYTES) != prefix_hash or \
                device_hash(device, BOOT_BYTES, prepared_root.stat().st_size) != root_hash:
            raise RuntimeError('Virtual BOOT or Linux root readback differs')
        stock_logo_hash = install_boot_logo(device, boot_logo)
        boot_hash = device_hash(device, 0, BOOT_BYTES)
        part = device + 's3'
        for _ in range(20):
            try:
                detail = info(part)
            except subprocess.CalledProcessError:
                time.sleep(0.5)
                continue
            if (detail.get('ParentWholeDisk') == device.removeprefix('/dev/') and
                    detail.get('PartitionMapPartitionOffset') == games_offset and
                    detail.get('IOKitSize') == games_bytes):
                break
            time.sleep(0.5)
        else:
            raise RuntimeError('Virtual ROMS partition geometry was not recognized')
        print('Formatting the virtual empty ROMS partition...', flush=True)
        command('/sbin/newfs_exfat', '-b', '65536', '-v', 'EASYROMS', part)
        with tempfile.TemporaryDirectory(prefix='cartridge-spare-roms-') as mount:
            command('/usr/sbin/diskutil', 'mount', '-mountPoint', mount, part)
            try:
                roms = Path(mount)
                (roms / 'Cartridge').mkdir()
                (roms / 'tools').mkdir()
                print('Staging and verifying the CI bundle on virtual ROMS...', flush=True)
                staged = stage_roms(roms, bundle, backup)
                if staged.get('state') != 'prepared_and_verified':
                    raise RuntimeError('Virtual ROMS staging was incomplete')
            finally:
                command('/usr/sbin/diskutil', 'unmount', part)
        command('/sbin/fsck_exfat', '-n', part)
        if device_hash(device, 0, BOOT_BYTES) != boot_hash or \
                device_hash(device, BOOT_BYTES, prepared_root.stat().st_size) != root_hash:
            raise RuntimeError('Virtual system partitions changed during ROMS staging')
        report = {'state': 'virtual_firstboot_image_verified', 'image': str(output),
                  'logical_bytes': card_bytes, 'boot_sha256': boot_hash,
                  'root_bytes': prepared_root.stat().st_size,
                  'stock_boot_sha256': prefix_hash,
                  'stock_boot_logo_sha256': stock_logo_hash,
                  'cartridge_boot_logo_sha256': sha256(boot_logo),
                  'root_sha256': root_hash, 'games_offset': games_offset,
                  'games_bytes': games_bytes, 'bundle_sha256': bundle_digest(bundle),
                  'build_revision': manifest['build_revision'], 'physical_card_written': False}
        write_manifest(output_parent / 'firstboot-image-report.json', report)
        return report
    finally:
        if device is not None:
            command('/usr/bin/hdiutil', 'detach', device)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--prefix-sha256', required=True)
    parser.add_argument('--prepared-root', type=Path, required=True)
    parser.add_argument('--root-sha256', required=True)
    parser.add_argument('--preparation-backup', type=Path, required=True)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--card-bytes', type=int, required=True)
    parser.add_argument('--boot-logo', type=Path, default=ROOT / 'assets/logo.bmp')
    args = parser.parse_args()
    try:
        print(json.dumps(build(args.prefix, args.prefix_sha256, args.prepared_root,
                               args.root_sha256, args.preparation_backup, args.bundle,
                               args.output, args.card_bytes, args.boot_logo), indent=2))
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
        parser.exit(2, f'First-boot image build stopped: {exc}\n')


if __name__ == '__main__':
    main()
