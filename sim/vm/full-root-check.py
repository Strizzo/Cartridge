#!/usr/bin/env python3
"""Rehearse VM preparation on a verified full-size stock root image copy.

The source image is opened read-only. Only a temporary copy on the chosen
workspace volume is modified. No physical SD partition is opened or written.
"""
import argparse
from contextlib import nullcontext
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'installer'))
from card_writeback import default_fsck  # noqa: E402
from install_transaction import verify_prepared_root  # noqa: E402
from offline_prepare import bundle_digest, sha256, write_manifest  # noqa: E402


def checked_source(path, expected_sha256):
    path = Path(path)
    if path.is_symlink() or not path.is_file() or not stat.S_ISREG(path.stat().st_mode):
        raise RuntimeError('Source must be a regular image file, not a device or symlink')
    source = path.resolve(strict=True)
    if not isinstance(expected_sha256, str) or len(expected_sha256) != 64:
        raise RuntimeError('A recorded 64-character source SHA-256 is required')
    print('Checking recorded source image hash...', flush=True)
    if sha256(source) != expected_sha256:
        raise RuntimeError('Source image differs from its recorded SHA-256')
    default_fsck(source)
    return source


def copy_verified(source, destination, expected_sha256):
    count = 0
    next_report = 1024**3
    with source.open('rb') as old, destination.open('xb') as new:
        while block := old.read(4*1024*1024):
            new.write(block)
            count += len(block)
            if count >= next_report:
                print(f'Copied {count/1024**3:.1f} GiB of {source.stat().st_size/1024**3:.1f} GiB',
                      flush=True)
                next_report += 1024**3
        new.flush()
        os.fsync(new.fileno())
    if count != source.stat().st_size or sha256(destination) != expected_sha256:
        raise RuntimeError('Temporary root copy failed size or independent readback verification')


def rehearse(source, expected_sha256, bundle, workspace_parent, *,
            app_path='/roms/Cartridge', keep_work_dir=None):
    source = checked_source(source, expected_sha256)
    bundle, workspace_parent = Path(bundle), Path(workspace_parent)
    if bundle.is_symlink() or workspace_parent.is_symlink():
        raise RuntimeError('Bundle and workspace parent must not be symlinks')
    bundle = bundle.resolve(strict=True)
    workspace_parent = workspace_parent.resolve(strict=True)
    if not bundle.is_dir() or not (bundle/'dev/build-revision').is_file():
        raise RuntimeError('Expected an extracted CI device bundle')
    if not workspace_parent.is_dir() or workspace_parent == source.parent:
        raise RuntimeError('Use a separate temporary workspace directory')
    if workspace_parent.stat().st_dev != source.stat().st_dev:
        raise RuntimeError('Source and temporary workspace must be on the same verified volume')
    if shutil.disk_usage(workspace_parent).free < source.stat().st_size + 1024**3:
        raise RuntimeError('Temporary workspace needs image size plus 1 GiB free')
    source_bundle_hash = bundle_digest(bundle)
    if keep_work_dir is None:
        workspace = tempfile.TemporaryDirectory(prefix='cartridge-full-root-check-',
                                                dir=workspace_parent)
    else:
        kept = Path(keep_work_dir)
        if (not kept.is_absolute() or kept.parent.resolve(strict=True) != workspace_parent or
                kept.exists() or kept.is_symlink()):
            raise RuntimeError('Retained work directory must be a new direct child of the workspace')
        kept.mkdir(mode=0o700)
        workspace = nullcontext(str(kept))
    with workspace as temporary:
        work = Path(temporary).resolve()
        image = work/'prepared-root.ext4'
        print('Making an independently verified temporary root copy...', flush=True)
        copy_verified(source, image, expected_sha256)
        host_report = {'state': 'running_vm', 'writes_to_card': False,
                       'root_bytes': source.stat().st_size, 'app_path': app_path,
                       'original_backup': str(source.parent),
                       'source_image': str(source), 'source_sha256': expected_sha256,
                       'bundle_sha256': source_bundle_hash}
        write_manifest(work/'host-report.json', host_report)
        print('Preparing the copy inside the isolated ARM VM...', flush=True)
        subprocess.run(['bash', str(ROOT/'sim/vm/prepare-root.sh'), str(work), str(bundle), app_path],
                       check=True)
        guest = json.loads((work/'guest-report.json').read_text())
        manifest = json.loads((work/'preparation-backup/manifest.json').read_text())
        prepared_hash = sha256(image)
        if (guest.get('state') != 'root_prepared_and_verified' or
                guest.get('image_sha256') != prepared_hash or
                guest.get('image_bytes') != source.stat().st_size or
                manifest.get('state') != 'root_prepared_and_verified' or
                manifest.get('bundle_sha256') != source_bundle_hash or
                manifest.get('app_path') != app_path):
            raise RuntimeError('VM result does not match the prepared root and CI bundle')
        default_fsck(image)
        verify_prepared_root(image, manifest)
        if sha256(image) != prepared_hash or sha256(source) != expected_sha256:
            raise RuntimeError('Prepared or original image changed during final verification')
        report = {'state': 'full_stock_root_preparation_verified',
                  'build_revision': manifest['build_revision'],
                  'source_image': str(source), 'source_sha256': expected_sha256,
                  'prepared_sha256': prepared_hash,
                  'root_bytes': image.stat().st_size,
                  'stock_es_service_preserved': True,
                  'raw_card_device_opened': False,
                  'temporary_copy_removed': keep_work_dir is None}
        if keep_work_dir is not None:
            report['prepared_image'] = str(image)
            report['preparation_backup'] = str(work/'preparation-backup')
    results = ROOT/'.sim/vm/results'
    results.mkdir(parents=True, exist_ok=True)
    write_manifest(results/'full-root-check.json', report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--workspace-parent', type=Path, required=True)
    parser.add_argument('--keep-work-dir', type=Path,
                        help='Retain the verified prepared image in a new direct child of the workspace')
    parser.add_argument('--app-path', choices=('/roms/Cartridge', '/roms2/Cartridge'),
                        default='/roms/Cartridge')
    args = parser.parse_args()
    try:
        result = rehearse(args.source, args.sha256, args.bundle,
                          args.workspace_parent, app_path=args.app_path,
                          keep_work_dir=args.keep_work_dir)
    except (RuntimeError, OSError, ValueError, subprocess.CalledProcessError) as exc:
        parser.exit(2, f'Full-size root rehearsal stopped: {exc}\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
