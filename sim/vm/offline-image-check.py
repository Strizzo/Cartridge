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
        report = {'state': result['state'], 'build_revision': result['build_revision'],
                  'root_fstype': 'ext4', 'roms_fstype': 'exfat-fuse',
                  'cartridge_default_on_next_boot': True,
                  'stock_es_service_preserved': True, 'game_save_and_gamelist_sha256': previous,
                  'user_key_preserved': True, 'source_card_or_host_mount_used': False}
    finally:
        if roms_active:
            run('umount', str(mounted_root/'roms'))
        if root_active:
            run('umount', str(mounted_root))
    run('e2fsck', '-fn', str(root_image), stdout=subprocess.DEVNULL)
    run('fsck.exfat', '-n', str(roms_image), stdout=subprocess.DEVNULL)
    (ROOT/'results/offline-image.json').write_text(json.dumps(report, indent=2)+'\n')
    print('OFFLINE IMAGE CHECK PASSED: Cartridge default, exFAT games/saves preserved, both filesystems clean')


if __name__ == '__main__':
    main()
