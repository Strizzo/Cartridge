#!/usr/bin/env python3
"""Exercise offline Cartridge installation on disposable ext4 + exFAT images."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
BASE = Path('/tmp/cartridge-vm/offline-image-check')
BUNDLE = ROOT/'bundle/Cartridge'


def run(*args, **kwargs):
    return subprocess.run(args, check=True, text=True, **kwargs)


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(chunk)
    return value.hexdigest()


def main():
    if platform.machine() != 'aarch64' or os.geteuid() != 0 or not Path('/etc/cartridge-compat-vm').exists():
        raise RuntimeError('This check is restricted to the disposable ARM VM')
    if BASE.exists():
        shutil.rmtree(BASE)
    BASE.mkdir()
    root_image, roms_image = BASE/'root.ext4', BASE/'roms.exfat'
    for image, size in [(root_image, 128*1024*1024), (roms_image, 96*1024*1024)]:
        with image.open('wb') as stream:
            stream.truncate(size)
    run('mkfs.ext4', '-F', '-q', str(root_image))
    run('mkfs.exfat', str(roms_image), stdout=subprocess.DEVNULL)
    mounted_root = BASE/'mounted-root'
    mounted_root.mkdir()
    root_active = roms_active = False
    try:
        run('mount', '-t', 'ext4', '-o', 'loop', str(root_image), str(mounted_root))
        root_active = True
        mounted_roms = mounted_root/'roms'
        mounted_roms.mkdir()
        run('mount', '-t', 'exfat-fuse', '-o', 'loop', str(roms_image), str(mounted_roms))
        roms_active = True
        service = mounted_root/'etc/systemd/system/emulationstation.service'
        service.parent.mkdir(parents=True)
        stock = '[Service]\nUser=ark\nExecStart=/usr/bin/emulationstation/emulationstation.sh\n'
        service.write_text(stock)
        wants = service.parent/'multi-user.target.wants'
        wants.mkdir()
        (wants/'emulationstation.service').symlink_to('../emulationstation.service')
        script = mounted_root/'usr/bin/emulationstation/emulationstation.sh'
        script.parent.mkdir(parents=True)
        script.write_text('#!/bin/sh\nexit 0\n')
        game = mounted_roms/'psx/fixture-game.chd'
        game.parent.mkdir()
        game.write_bytes(b'Game data stays on the ROMS partition')
        save = mounted_roms/'psx/fixture-game.srm'
        save.write_bytes(b'Existing save stays on the ROMS partition')
        gamelist = mounted_roms/'psx/gamelist.xml'
        gamelist.write_text('<gameList><game/></gameList>')
        previous = {'game': digest(game), 'save': digest(save), 'gamelist': digest(gamelist)}
        app = mounted_roms/'Cartridge'
        (app/'assets/fonts').mkdir(parents=True)
        (app/'cartridge').write_bytes(b'previous application')
        (app/'ssh').mkdir()
        key = app/'ssh/id_ed25519'
        key.write_bytes(b'private-key-fixture')
        (mounted_roms/'tools').mkdir()
        # Capture the untouched, unmounted stock root before conversion. The
        # transaction below writes only a separate disposable target copy.
        run('umount', str(mounted_roms))
        roms_active = False
        run('umount', str(mounted_root))
        root_active = False
        original_backup = BASE/'original-backup'
        original_backup.mkdir()
        shutil.copyfile(root_image, original_backup/'root.ext4')
        disposable_target = BASE/'disposable-target.ext4'
        shutil.copyfile(root_image, disposable_target)
        original_hash = digest(original_backup/'root.ext4')
        run('mount', '-t', 'ext4', '-o', 'loop', str(root_image), str(mounted_root))
        root_active = True
        run('mount', '-t', 'exfat-fuse', '-o', 'loop', str(roms_image), str(mounted_roms))
        roms_active = True
        spec = importlib.util.spec_from_file_location('offline_prepare', ROOT/'installer/offline_prepare.py')
        converter = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(converter)
        result = converter.prepare(mounted_root, mounted_roms, BUNDLE, BASE/'previous-installation')
        if result['state'] != 'prepared_and_verified' or not result['cartridge_default_on_next_boot']:
            raise RuntimeError('Offline conversion did not enable Cartridge')
        if digest(app/'cartridge') != digest(BUNDLE/'cartridge'):
            raise RuntimeError('The installed ARM executable differs from the bundle')
        if {name: digest(path) for name, path in [('game',game),('save',save),('gamelist',gamelist)]} != previous:
            raise RuntimeError('Offline conversion modified a game, save or gamelist')
        if key.read_bytes() != b'private-key-fixture':
            raise RuntimeError('Offline conversion modified a user key')
        if service.read_text() != stock:
            raise RuntimeError('Stock ES service was replaced')
        # A failed s2 write must be able to undo the already staged exFAT
        # Cartridge files without touching games, saves, or the user's key.
        rollback = converter.restore_roms(mounted_roms, BASE/'previous-installation')
        if rollback['state'] != 'roms_rollback_verified':
            raise RuntimeError('Cartridge ROMS rollback was not verified')
        if (app/'cartridge').read_bytes() != b'previous application':
            raise RuntimeError('ROMS rollback did not restore the old executable')
        if {name: digest(path) for name, path in [('game',game),('save',save),('gamelist',gamelist)]} != previous:
            raise RuntimeError('ROMS rollback modified a game, save or gamelist')
        if key.read_bytes() != b'private-key-fixture':
            raise RuntimeError('ROMS rollback modified a user key')
        result = converter.prepare(mounted_root, mounted_roms, BUNDLE, BASE/'reprepared-installation')
        if result['state'] != 'prepared_and_verified' or digest(app/'cartridge') != digest(BUNDLE/'cartridge'):
            raise RuntimeError('Offline conversion failed after a verified ROMS rollback')
        report = {'state': result['state'], 'build_revision': result['build_revision'],
                  'root_fstype': 'ext4', 'roms_fstype': 'exfat-fuse',
                  'cartridge_default_on_next_boot': True,
                  'stock_es_service_preserved': True, 'game_save_and_gamelist_sha256': previous,
                  'user_key_preserved': True, 'roms_rollback_rehearsed': True,
                  'source_card_or_host_mount_used': False}
    finally:
        if roms_active:
            run('umount', str(mounted_root/'roms'))
        if root_active:
            run('umount', str(mounted_root))
    run('e2fsck', '-fn', str(root_image), stdout=subprocess.DEVNULL)
    sys.path.insert(0, str(ROOT/'installer'))
    from install_transaction import verify_prepared_root
    verify_prepared_root(root_image, result)
    run('fsck.exfat', '-n', str(roms_image), stdout=subprocess.DEVNULL)
    report['prepared_root_debugfs_verified'] = True
    # Complete the same two-partition handoff used by the future Mac app, but
    # route s2 writes into a regular file and s3 into this disposable exFAT.
    mounted_transaction_roms = BASE/'transaction-roms'
    mounted_transaction_roms.mkdir()
    run('mount', '-t', 'exfat-fuse', '-o', 'loop', str(roms_image), str(mounted_transaction_roms))
    try:
        sys.path.insert(0, str(ROOT/'installer'))
        from install_transaction import finish_install
        fingerprint = 'disposable-arm-vm-card'
        size = disposable_target.stat().st_size
        (original_backup/'manifest.json').write_text(json.dumps({
            'state': 'verified', 'selected_disk': 'disk6',
            'inventory_fingerprint': fingerprint, 'source_partition': 'disk6s2',
            'source_bytes': size, 'sha256': original_hash, 'source_read_passes': 2,
            'destination_readback_verified': True, 'ext4_check': 'clean',
        }))
        record = {'identifier': 'disk6', 'inventory_fingerprint': fingerprint,
                  'status': 'preserve_candidate', 'partitions': [
                      {'identifier': 'disk6s1', 'content': 'DOS_FAT_32', 'size_bytes': 100,
                       'device_size_bytes': 100, 'mounted': False, 'mount_point': ''},
                      {'identifier': 'disk6s2', 'content': 'Linux', 'size_bytes': size,
                       'device_size_bytes': size, 'mounted': False, 'mount_point': ''},
                      {'identifier': 'disk6s3', 'content': 'Windows_NTFS', 'size_bytes': roms_image.stat().st_size,
                       'device_size_bytes': roms_image.stat().st_size, 'mounted': True,
                       'mount_point': str(mounted_transaction_roms)},
                  ]}
        prepared_hash = digest(root_image)
        transaction = finish_install('disk6', fingerprint, original_backup, root_image,
                                     prepared_hash, BASE/'reprepared-installation',
                                     mounted_transaction_roms, BASE/'root-write-journal',
                                     inventory_fn=lambda: [record],
                                     target_override=disposable_target)
        if transaction['state'] != 'installed_and_verified' or digest(disposable_target) != prepared_hash:
            raise RuntimeError('Disposable two-partition install did not verify')
        for name, expected in previous.items():
            if digest(mounted_transaction_roms/'psx'/('fixture-game.chd' if name == 'game' else
                                                     'fixture-game.srm' if name == 'save' else
                                                     'gamelist.xml')) != expected:
                raise RuntimeError('Two-partition transaction modified '+name)
        if (mounted_transaction_roms/'Cartridge/ssh/id_ed25519').read_bytes() != b'private-key-fixture':
            raise RuntimeError('Two-partition transaction modified the user key')
    finally:
        run('umount', str(mounted_transaction_roms))
    run('e2fsck', '-fn', str(disposable_target), stdout=subprocess.DEVNULL)
    run('fsck.exfat', '-n', str(roms_image), stdout=subprocess.DEVNULL)
    report['disposable_two_partition_transaction_verified'] = True
    report['original_root_sha256'] = original_hash
    report['installed_root_sha256'] = prepared_hash
    (ROOT/'results/offline-image.json').write_text(json.dumps(report, indent=2)+'\n')
    print('OFFLINE IMAGE CHECK PASSED: Cartridge default, exFAT games/saves preserved, both filesystems clean')


if __name__ == '__main__':
    main()
