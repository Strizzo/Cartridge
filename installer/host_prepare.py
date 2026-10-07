#!/usr/bin/env python3
"""Mac-to-ARM-VM handoff for a verified clone of a selected games card.

The root phase writes only a new file outside the card. The separate ROMS
phase stages Cartridge-owned files on the selected card after root validation.
Neither phase writes BOOT, the card's Linux partition, games or saves.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

from card_clone import select_root
from card_inventory import inventory
from card_writeback import default_fsck, load_backup, outside_card
from install_transaction import verify_prepared_root
from offline_prepare import bundle_digest, restore_roms, sha256, stage_roms, write_manifest


class HostPrepareError(RuntimeError):
    pass


def prepare_clone(disk_id, fingerprint, original_backup, work_dir, bundle,
                  app_path='/roms/Cartridge', *, inventory_fn=inventory,
                  vm_runner=None, filesystem_check=default_fsck,
                  root_validator=verify_prepared_root):
    """Create and verify a mutable copy, then prepare it in the isolated VM."""
    if app_path not in {'/roms/Cartridge', '/roms2/Cartridge'}:
        raise HostPrepareError('Unsupported Cartridge installation path')
    record, part = select_root(inventory_fn(), disk_id, fingerprint)
    original, original_hash = load_backup(original_backup, disk_id, fingerprint,
                                          part['size_bytes'], record)
    bundle = outside_card(bundle, record, 'CI device bundle', require_file=False)
    if not bundle.is_dir() or bundle.is_symlink() or not (bundle/'dev/build-revision').is_file():
        raise HostPrepareError('Expected an extracted CI device bundle')
    source_bundle_sha = bundle_digest(bundle)
    work_dir = outside_card(work_dir, record, 'Preparation workspace')
    if work_dir.exists() or work_dir.is_symlink():
        raise HostPrepareError('Preparation workspace must be a new directory')
    if work_dir == original.parent or original.parent in work_dir.parents:
        raise HostPrepareError('Workspace must be separate from the immutable original backup')
    if shutil.disk_usage(work_dir.parent).free < part['size_bytes'] + 1024**3:
        raise HostPrepareError('Preparation workspace needs root image size plus 1 GiB free')
    work_dir.mkdir(mode=0o700)
    report_path = work_dir/'host-report.json'
    report = {'state': 'copying_root', 'selected_disk': disk_id,
              'inventory_fingerprint': fingerprint, 'original_sha256': original_hash,
              'root_bytes': part['size_bytes'], 'app_path': app_path,
              'bundle_revision': (bundle/'dev/build-revision').read_text().strip(),
              'bundle_cartridge_sha256': sha256(bundle/'cartridge'),
              'bundle_session_sha256': sha256(bundle/'cartridge-session.py'),
              'bundle_sha256': source_bundle_sha,
              'original_backup': str(Path(original_backup).resolve()),
              'bundle': str(bundle), 'writes_to_card': False}
    write_manifest(report_path, report)
    image = work_dir/'prepared-root.ext4'
    try:
        with original.open('rb') as source, image.open('xb') as destination:
            shutil.copyfileobj(source, destination, 4*1024*1024)
            destination.flush()
            os.fsync(destination.fileno())
        if image.stat().st_size != part['size_bytes'] or sha256(image) != original_hash:
            raise HostPrepareError('Mutable clone differs from the verified original')
        report['state'] = 'running_vm'
        write_manifest(report_path, report)
        if vm_runner is None:
            script = Path(__file__).resolve().parents[1]/'sim/vm/prepare-root.sh'
            subprocess.run(['bash', str(script), str(work_dir), str(bundle), app_path], check=True)
        else:
            vm_runner(work_dir, bundle, app_path)
        guest_report_path = work_dir/'guest-report.json'
        manifest_path = work_dir/'preparation-backup/manifest.json'
        if guest_report_path.is_symlink() or manifest_path.is_symlink():
            raise HostPrepareError('VM handoff report must not be symlinked')
        guest = json.loads(guest_report_path.read_text())
        preparation = json.loads(manifest_path.read_text())
        prepared_hash = sha256(image)
        if (guest.get('state') != 'root_prepared_and_verified' or
                guest.get('image_sha256') != prepared_hash or
                guest.get('image_bytes') != part['size_bytes'] or
                guest.get('app_path') != app_path or
                preparation.get('state') != 'root_prepared_and_verified' or
                preparation.get('app_path') != app_path or
                preparation.get('build_revision') != report['bundle_revision'] or
                preparation.get('cartridge_sha256') != report['bundle_cartridge_sha256'] or
                preparation.get('session_sha256') != report['bundle_session_sha256'] or
                preparation.get('bundle_sha256') != report['bundle_sha256']):
            raise HostPrepareError('VM preparation report disagrees with the selected clone or bundle')
        filesystem_check(image)
        root_validator(image, preparation)
        if sha256(image) != prepared_hash:
            raise HostPrepareError('Prepared root changed during validation')
        report.update(state='root_prepared_and_verified', prepared_sha256=prepared_hash,
                      writes_to_card=False)
        write_manifest(report_path, report)
        return report
    except BaseException as exc:
        report.update(state='incomplete', error=str(exc))
        write_manifest(report_path, report)
        raise


def stage_selected_roms(disk_id, fingerprint, work_dir, roms, bundle,
                        *, inventory_fn=inventory, require_mount=True,
                        filesystem_check=default_fsck,
                        root_validator=verify_prepared_root):
    """Stage only Cartridge-owned s3 files after root clone verification."""
    record, part = select_root(inventory_fn(), disk_id, fingerprint)
    work_dir = outside_card(work_dir, record, 'Preparation workspace')
    bundle = outside_card(bundle, record, 'CI device bundle', require_file=False)
    roms = Path(roms)
    parts = record['partitions']
    if (len(parts) != 3 or not parts[2]['mounted'] or not parts[2]['mount_point'] or
            roms.is_symlink() or roms.resolve(strict=True) != Path(parts[2]['mount_point']).resolve(strict=True)):
        raise HostPrepareError('Selected games partition is not mounted at the supplied ROMS path')
    if require_mount and not os.path.ismount(roms):
        raise HostPrepareError('Selected games partition is not a mount point')
    path = work_dir/'host-report.json'
    if path.is_symlink() or not path.is_file():
        raise HostPrepareError('Host preparation report is missing')
    host = json.loads(path.read_text())
    if (host.get('state') != 'root_prepared_and_verified' or
            host.get('selected_disk') != disk_id or
            host.get('inventory_fingerprint') != fingerprint or
            host.get('root_bytes') != part['size_bytes'] or
            host.get('bundle') != str(bundle) or
            host.get('bundle_revision') != (bundle/'dev/build-revision').read_text().strip() or
            host.get('bundle_cartridge_sha256') != sha256(bundle/'cartridge') or
            host.get('bundle_session_sha256') != sha256(bundle/'cartridge-session.py') or
            host.get('bundle_sha256') != bundle_digest(bundle)):
        raise HostPrepareError('Prepared clone no longer matches the selected card or bundle')
    image = work_dir/'prepared-root.ext4'
    if image.is_symlink() or not image.is_file() or sha256(image) != host.get('prepared_sha256'):
        raise HostPrepareError('Prepared Linux image changed before ROMS staging')
    filesystem_check(image)
    preparation_backup = work_dir/'preparation-backup'
    preparation = json.loads((preparation_backup/'manifest.json').read_text())
    if preparation.get('state') != 'root_prepared_and_verified':
        raise HostPrepareError('Root preparation is not at the staging handoff')
    root_validator(image, preparation)
    if sha256(image) != host['prepared_sha256']:
        raise HostPrepareError('Prepared Linux image changed during validation')
    try:
        staged = stage_roms(roms, bundle, preparation_backup, require_mount=require_mount)
        host['state'] = 'roms_staged_and_verified'
        host['writes_to_card'] = True
        write_manifest(path, host)
        return staged
    except BaseException:
        state = json.loads((preparation_backup/'manifest.json').read_text()).get('state')
        if state in {'roms_backed_up', 'prepared_and_verified', 'roms_rollback_incomplete'}:
            restore_roms(roms, preparation_backup, require_mount=require_mount)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='phase', required=True)
    root = sub.add_parser('root', help='Prepare only an immutable-backup-derived root copy in the VM')
    root.add_argument('--disk', required=True)
    root.add_argument('--fingerprint', required=True)
    root.add_argument('--original-backup', type=Path, required=True)
    root.add_argument('--work-dir', type=Path, required=True)
    root.add_argument('--bundle', type=Path, required=True)
    root.add_argument('--app-path', choices=('/roms/Cartridge', '/roms2/Cartridge'),
                      default='/roms/Cartridge')
    roms = sub.add_parser('roms', help='Stage only Cartridge-owned files on selected s3')
    roms.add_argument('--disk', required=True)
    roms.add_argument('--fingerprint', required=True)
    roms.add_argument('--work-dir', type=Path, required=True)
    roms.add_argument('--roms', type=Path, required=True)
    roms.add_argument('--bundle', type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.phase == 'root':
            result = prepare_clone(args.disk, args.fingerprint, args.original_backup,
                                   args.work_dir, args.bundle, args.app_path)
        else:
            result = stage_selected_roms(args.disk, args.fingerprint, args.work_dir,
                                         args.roms, args.bundle)
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
        parser.exit(2, f'Host preparation stopped: {exc}\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
