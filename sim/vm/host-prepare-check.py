#!/usr/bin/env python3
"""Repeat the full Mac->ARM VM->disposable s3/s2 installer transaction.

Usage: python3 sim/vm/host-prepare-check.py /path/to/device-bundle/Cartridge
Only temporary ordinary files are used. No diskutil, raw device or SD access.
"""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO/'installer'))
from host_prepare import prepare_clone, stage_selected_roms  # noqa: E402
from install_transaction import finish_install  # noqa: E402
from offline_prepare import sha256  # noqa: E402


def mkfs_tool():
    command = shutil.which('mke2fs') or '/opt/homebrew/opt/e2fsprogs/sbin/mke2fs'
    if not Path(command).is_file():
        raise RuntimeError('mke2fs is required for the disposable ext4 fixture')
    return command


def main():
    if len(sys.argv) != 2:
        raise SystemExit('Supply an extracted CI device bundle /Cartridge directory')
    bundle = Path(sys.argv[1]).resolve(strict=True)
    if not (bundle/'dev/build-revision').is_file():
        raise RuntimeError('Expected an extracted CI device bundle')
    with tempfile.TemporaryDirectory(prefix='cartridge-host-prep-check-') as name:
        base = Path(name).resolve()
        stock = base/'stock-tree'
        service = stock/'etc/systemd/system/emulationstation.service'
        service.parent.mkdir(parents=True)
        service.write_text('[Service]\nUser=ark\nExecStart=/usr/bin/emulationstation/emulationstation.sh\n')
        wants = service.parent/'multi-user.target.wants'
        wants.mkdir()
        (wants/'emulationstation.service').symlink_to('../emulationstation.service')
        es = stock/'usr/bin/emulationstation/emulationstation.sh'
        es.parent.mkdir(parents=True)
        es.write_text('#!/bin/sh\nexit 0\n')
        (stock/'roms').mkdir()
        original = base/'original'
        original.mkdir()
        original_image = original/'root.ext4'
        with original_image.open('wb') as stream:
            stream.truncate(128*1024*1024)
        subprocess.run([mkfs_tool(), '-F', '-q', '-t', 'ext4', '-d', str(stock),
                        str(original_image)], check=True)
        original_hash = sha256(original_image)
        fingerprint = 'disposable-host-prep-check'
        (original/'manifest.json').write_text(json.dumps({
            'state': 'verified', 'selected_disk': 'disk6',
            'inventory_fingerprint': fingerprint, 'source_partition': 'disk6s2',
            'source_bytes': original_image.stat().st_size, 'sha256': original_hash,
            'source_read_passes': 2, 'destination_readback_verified': True,
            'ext4_check': 'clean'}))
        roms = base/'synthetic-roms'
        (roms/'Cartridge/ssh').mkdir(parents=True)
        (roms/'Cartridge/cartridge').write_bytes(b'previous Cartridge executable')
        (roms/'Cartridge/ssh/id_ed25519').write_bytes(b'user key fixture')
        (roms/'tools').mkdir()
        (roms/'psx').mkdir()
        (roms/'psx/game.chd').write_bytes(b'game fixture')
        (roms/'psx/game.srm').write_bytes(b'save fixture')
        protected = {str(path.relative_to(roms)): sha256(path) for path in
                     (roms/'psx/game.chd', roms/'psx/game.srm', roms/'Cartridge/ssh/id_ed25519')}
        size = original_image.stat().st_size
        record = {'identifier': 'disk6', 'inventory_fingerprint': fingerprint,
                  'status': 'preserve_candidate', 'partitions': [
                      {'identifier': 'disk6s1', 'mount_point': ''},
                      {'identifier': 'disk6s2', 'content': 'Linux', 'mounted': False,
                       'mount_point': '', 'size_bytes': size, 'device_size_bytes': size},
                      {'identifier': 'disk6s3', 'mounted': True, 'mount_point': str(roms)}]}
        inventory_fn = lambda: [record]
        work = base/'work'
        root_report = prepare_clone('disk6', fingerprint, original, work, bundle,
                                    inventory_fn=inventory_fn)
        if sha256(original_image) != original_hash:
            raise RuntimeError('Immutable root backup changed during VM preparation')
        if (roms/'Cartridge/cartridge').read_bytes() != b'previous Cartridge executable':
            raise RuntimeError('Root-only VM preparation touched the games partition')
        staged = stage_selected_roms('disk6', fingerprint, work, roms, bundle,
                                     inventory_fn=inventory_fn, require_mount=False)
        target = base/'disposable-s2.ext4'
        shutil.copyfile(original_image, target)
        prepared = work/'prepared-root.ext4'
        result = finish_install('disk6', fingerprint, original, prepared,
                                root_report['prepared_sha256'], work/'preparation-backup',
                                roms, base/'write-journal', inventory_fn=inventory_fn,
                                target_override=target, require_mount=False)
        if (staged['state'] != 'prepared_and_verified' or
                result['state'] != 'installed_and_verified' or
                sha256(target) != sha256(prepared) or
                sha256(original_image) != original_hash or
                {rel: sha256(roms/rel) for rel in protected} != protected):
            raise RuntimeError('Disposable transaction or personal-data verification failed')
        report = {'state': 'disposable_host_vm_transaction_verified',
                  'build_revision': root_report['bundle_revision'],
                  'original_root_sha256': original_hash,
                  'installed_root_sha256': sha256(prepared),
                  'game_save_and_key_sha256': protected,
                  'physical_card_accessed': False}
    results = REPO/'.sim/vm/results'
    results.mkdir(parents=True, exist_ok=True)
    (results/'host-prepare-check.json').write_text(json.dumps(report, indent=2)+'\n')
    print('HOST->VM DISPOSABLE TRANSACTION PASSED. Report: '+str(results/'host-prepare-check.json'))


if __name__ == '__main__':
    main()
