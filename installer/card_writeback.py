#!/usr/bin/env python3
"""Guarded Linux-partition writeback for a previously cloned, compatible card.

This does not touch BOOT, the partition map, or EASYROMS. The prepared ext4
image must be offline and the verified original clone must remain available.
An interrupted write leaves a durable journal for explicit restoration.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile

from card_clone import CloneError, read_exact_hash, select_root
from card_inventory import DISK_ID, InventoryError, inventory


class WritebackError(RuntimeError):
    pass


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def save_journal(path, value):
    """Replace and sync both the journal file and its directory."""
    with tempfile.NamedTemporaryFile(mode='w', dir=path.parent, prefix='.journal-',
                                     encoding='utf-8', delete=False) as stream:
        temporary = Path(stream.name)
        try:
            json.dump(value, stream, indent=2)
            stream.write('\n')
            stream.flush()
            os.fsync(stream.fileno())
        except BaseException:
            temporary.unlink(missing_ok=True)
            raise
    try:
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)
    sync_directory(path.parent)


def outside_card(path, record, label, *, require_file=False):
    path = Path(path)
    if path.is_symlink():
        raise WritebackError(f'{label} must not be a symlink')
    resolved = path.resolve(strict=True) if require_file else path.parent.resolve(strict=True) / path.name
    for part in record['partitions']:
        mount = part.get('mount_point')
        if mount:
            root = Path(mount).resolve(strict=True)
            if resolved == root or root in resolved.parents:
                raise WritebackError(f'{label} must be outside the selected card')
    if require_file and not resolved.is_file():
        raise WritebackError(f'{label} is not a regular file')
    return resolved


def checked_file(path, expected_bytes, expected_hash, label):
    if path.stat().st_size != expected_bytes:
        raise WritebackError(f'{label} has the wrong size')
    with path.open('rb', buffering=0) as stream:
        actual = read_exact_hash(stream, expected_bytes)
    if actual != expected_hash:
        raise WritebackError(f'{label} checksum differs from its manifest')


def default_fsck(path):
    command = shutil.which('e2fsck') or '/opt/homebrew/opt/e2fsprogs/sbin/e2fsck'
    if not Path(command).is_file():
        raise WritebackError('e2fsck is required')
    subprocess.run([command, '-fn', str(path)], check=True, capture_output=True, text=True)


def load_backup(directory, disk_id, fingerprint, size, record):
    directory = outside_card(directory, record, 'Original backup')
    if not directory.is_dir() or directory.is_symlink():
        raise WritebackError('Original backup directory is missing or symlinked')
    manifest_path = outside_card(directory/'manifest.json', record, 'Backup manifest', require_file=True)
    manifest = json.loads(manifest_path.read_text())
    if (manifest.get('state') != 'verified' or
            manifest.get('selected_disk') != disk_id or
            manifest.get('inventory_fingerprint') != fingerprint or
            manifest.get('source_partition') != disk_id+'s2' or
            manifest.get('source_bytes') != size or
            manifest.get('source_read_passes') != 2 or
            manifest.get('destination_readback_verified') is not True or
            manifest.get('ext4_check') != 'clean'):
        raise WritebackError('Original backup is not a verified clone of this partition')
    digest = manifest.get('sha256')
    if not isinstance(digest, str) or len(digest) != 64:
        raise WritebackError('Original backup checksum is invalid')
    image = outside_card(directory/'root.ext4', record, 'Original image', require_file=True)
    checked_file(image, size, digest, 'Original image')
    return image, digest


def open_target(path, size, *, override):
    if path.is_symlink():
        raise WritebackError('Target partition is symlinked')
    flags = os.O_RDWR | getattr(os, 'O_NOFOLLOW', 0)
    descriptor = os.open(path, flags)
    mode = os.fstat(descriptor).st_mode
    if not (stat.S_ISREG(mode) if override else stat.S_ISCHR(mode)):
        os.close(descriptor)
        raise WritebackError('Target is not the expected raw partition device')
    if override and os.fstat(descriptor).st_size != size:
        os.close(descriptor)
        raise WritebackError('Disposable target has the wrong size')
    return descriptor


def hash_target(descriptor, size):
    os.lseek(descriptor, 0, os.SEEK_SET)
    with os.fdopen(os.dup(descriptor), 'rb', buffering=0) as stream:
        return read_exact_hash(stream, size)


def copy_to_target(source, descriptor, size, *, chunk_hook=None):
    os.lseek(descriptor, 0, os.SEEK_SET)
    written = 0
    digest = hashlib.sha256()
    with source.open('rb', buffering=0) as stream:
        while written < size:
            block = stream.read(min(4 * 1024 * 1024, size - written))
            if not block:
                raise WritebackError('Image ended during write')
            digest.update(block)
            view = memoryview(block)
            while view:
                count = os.write(descriptor, view)
                if count <= 0:
                    raise WritebackError('Target accepted only a partial write')
                view = view[count:]
                written += count
            if chunk_hook:
                chunk_hook(written)
    os.fsync(descriptor)
    return digest.hexdigest()


def check_identity(inventory_fn, disk_id, fingerprint):
    return select_root(inventory_fn(), disk_id, fingerprint)


def check_fsck(fsck, image):
    try:
        fsck(image)
    except Exception as exc:
        raise WritebackError(f'ext4 check failed for {image.name}') from exc


def writeback(disk_id, fingerprint, backup_dir, prepared, prepared_sha256, journal_dir,
              *, inventory_fn=inventory, target_override=None, filesystem_check=None,
              chunk_hook=None):
    """Write only s2; target_override exists solely for disposable-file tests."""
    if not DISK_ID.fullmatch(disk_id):
        raise WritebackError('Select a whole disk identifier')
    record, partition = check_identity(inventory_fn, disk_id, fingerprint)
    size = partition['size_bytes']
    original, original_hash = load_backup(backup_dir, disk_id, fingerprint, size, record)
    prepared = outside_card(prepared, record, 'Prepared image', require_file=True)
    if prepared == original:
        raise WritebackError('Prepared image must be separate from the original backup')
    if not isinstance(prepared_sha256, str) or len(prepared_sha256) != 64:
        raise WritebackError('Prepared image SHA-256 is required')
    checked_file(prepared, size, prepared_sha256, 'Prepared image')
    fsck = filesystem_check or default_fsck
    check_fsck(fsck, prepared)
    checked_file(prepared, size, prepared_sha256, 'Prepared image')
    journal_dir = Path(journal_dir)
    journal_dir = outside_card(journal_dir, record, 'Recovery journal')
    if journal_dir.exists() or journal_dir.is_symlink():
        raise WritebackError('Recovery journal must be a new directory')
    if journal_dir.parent.stat().st_dev != original.parent.stat().st_dev:
        raise WritebackError('Recovery journal must be on the original backup volume')
    if journal_dir == prepared or journal_dir == original:
        raise WritebackError('Recovery journal overlaps an image')
    target = Path(target_override) if target_override is not None else Path('/dev/r'+partition['identifier'])
    if target_override is None and str(target) != '/dev/r'+disk_id+'s2':
        raise WritebackError('Only the selected Linux partition may be written')
    check_identity(inventory_fn, disk_id, fingerprint)
    descriptor = open_target(target, size, override=target_override is not None)
    journal = {
        'state': 'checking_target', 'disk': disk_id, 'fingerprint': fingerprint,
        'partition': partition['identifier'], 'bytes': size,
        'original_backup': str(original), 'original_sha256': original_hash,
        'prepared_image': str(prepared), 'prepared_sha256': prepared_sha256,
        'target': str(target), 'writes_to_boot_or_games': False,
    }
    try:
        if hash_target(descriptor, size) != original_hash:
            raise WritebackError('Card Linux partition changed since the verified clone; no write made')
        check_identity(inventory_fn, disk_id, fingerprint)
        journal_dir.mkdir(mode=0o700)
        sync_directory(journal_dir.parent)
        save_journal(journal_dir/'manifest.json', journal)
        journal['state'] = 'write_started'
        save_journal(journal_dir/'manifest.json', journal)
        try:
            if copy_to_target(prepared, descriptor, size, chunk_hook=chunk_hook) != prepared_sha256:
                raise WritebackError('Prepared image changed during write')
            if hash_target(descriptor, size) != prepared_sha256:
                raise WritebackError('Physical readback does not match prepared image')
            check_identity(inventory_fn, disk_id, fingerprint)
            check_fsck(fsck, target)
            if hash_target(descriptor, size) != prepared_sha256:
                raise WritebackError('Physical data changed during ext4 verification')
            journal['state'] = 'verified'
            save_journal(journal_dir/'manifest.json', journal)
        except BaseException as exc:
            journal.update(state='rollback_started', error=str(exc))
            try:
                save_journal(journal_dir/'manifest.json', journal)
            except OSError as journal_exc:
                # The durable write_started record already exists. Restore
                # the original even when updating the journal is impossible.
                journal['journal_error'] = str(journal_exc)
            try:
                check_identity(inventory_fn, disk_id, fingerprint)
                checked_file(original, size, original_hash, 'Original image')
                if copy_to_target(original, descriptor, size) != original_hash:
                    raise WritebackError('Original image changed during rollback')
                if hash_target(descriptor, size) != original_hash:
                    raise WritebackError('Original image rollback readback differs')
                journal['state'] = 'rolled_back_and_verified'
            except BaseException as rollback_exc:
                journal.update(state='rollback_incomplete', rollback_error=str(rollback_exc))
            try:
                save_journal(journal_dir/'manifest.json', journal)
            except OSError as journal_exc:
                journal['journal_error'] = str(journal_exc)
            raise WritebackError(f'Writeback failed; recovery state: {journal["state"]}; journal: {journal_dir}') from exc
        return journal
    finally:
        os.close(descriptor)


def restore(journal_dir, *, inventory_fn=inventory, target_override=None):
    """Explicitly restore a failed/interrupted write from its original image."""
    journal_dir = Path(journal_dir)
    if journal_dir.is_symlink():
        raise WritebackError('Recovery journal must not be symlinked')
    manifest_path = journal_dir/'manifest.json'
    if manifest_path.is_symlink() or not manifest_path.is_file():
        raise WritebackError('Recovery journal is missing')
    journal = json.loads(manifest_path.read_text())
    if journal.get('state') not in {'write_started', 'rollback_started', 'rollback_incomplete'}:
        raise WritebackError('Journal is not in a restorable state')
    disk_id, fingerprint = journal['disk'], journal['fingerprint']
    record, partition = check_identity(inventory_fn, disk_id, fingerprint)
    size = partition['size_bytes']
    if size != journal['bytes'] or partition['identifier'] != journal['partition']:
        raise WritebackError('Partition changed since writeback')
    original = outside_card(journal['original_backup'], record, 'Original image', require_file=True)
    outside_card(journal_dir, record, 'Recovery journal')
    if target_override is None and journal['target'] != '/dev/r'+disk_id+'s2':
        raise WritebackError('Journal target is not the selected Linux partition')
    target = Path(target_override) if target_override is not None else Path(journal['target'])
    checked_file(original, size, journal['original_sha256'], 'Original image')
    descriptor = open_target(target, size, override=target_override is not None)
    try:
        # An interrupted write or interrupted rollback can contain a mix of
        # both images. Identity and the original clone are checked above.
        journal['recovery_target_sha256'] = hash_target(descriptor, size)
        journal['state'] = 'rollback_started'
        save_journal(manifest_path, journal)
        if copy_to_target(original, descriptor, size) != journal['original_sha256']:
            raise WritebackError('Original image changed during restoration')
        if hash_target(descriptor, size) != journal['original_sha256']:
            raise WritebackError('Restored partition readback differs from backup')
        check_identity(inventory_fn, disk_id, fingerprint)
        journal['state'] = 'rolled_back_and_verified'
        save_journal(manifest_path, journal)
        return journal
    except BaseException as exc:
        journal.update(state='rollback_incomplete', rollback_error=str(exc))
        save_journal(manifest_path, journal)
        raise
    finally:
        os.close(descriptor)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    apply = sub.add_parser('apply', help='Write and verify only the selected Linux partition')
    apply.add_argument('--disk', required=True)
    apply.add_argument('--fingerprint', required=True)
    apply.add_argument('--original-backup', type=Path, required=True)
    apply.add_argument('--prepared-image', type=Path, required=True)
    apply.add_argument('--prepared-sha256', required=True)
    apply.add_argument('--journal', type=Path, required=True)
    recovery = sub.add_parser('restore', help='Restore the original Linux partition after interruption')
    recovery.add_argument('--journal', type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.action == 'apply':
            result = writeback(args.disk, args.fingerprint, args.original_backup,
                               args.prepared_image, args.prepared_sha256, args.journal)
        else:
            result = restore(args.journal)
    except (WritebackError, CloneError, InventoryError, OSError, ValueError,
            KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as exc:
        parser.exit(2, f'Card writeback stopped: {exc}\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
