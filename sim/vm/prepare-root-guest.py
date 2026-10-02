#!/usr/bin/env python3
"""Mount and prepare one disposable ext4 clone inside the dedicated ARM VM."""
import hashlib
import json
import os
from pathlib import Path
import platform
import stat
import subprocess
import sys


def run(*argv):
    return subprocess.run(argv, check=True, capture_output=True, text=True)


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(4*1024*1024), b''):
            h.update(block)
    return h.hexdigest()


def main():
    if len(sys.argv) != 4:
        raise SystemExit('Usage: prepare-root-guest.py STAGING BUNDLE APP_PATH')
    staging, bundle, app_path = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
    if (os.geteuid() != 0 or platform.machine() != 'aarch64' or
            not Path('/etc/cartridge-prep-vm').is_file()):
        raise RuntimeError('This tool runs only as root in the dedicated preparation VM')
    if staging.is_symlink() or not staging.is_dir():
        raise RuntimeError('Staging mount is absent or symlinked')
    staging = staging.resolve(strict=True)
    if run('findmnt', '-n', '-o', 'FSTYPE', '-T', str(staging)).stdout.strip() != 'virtiofs':
        raise RuntimeError('Expected exactly the selected host staging directory as a virtiofs mount')
    image = staging/'prepared-root.ext4'
    if image.is_symlink() or not image.is_file() or not stat.S_ISREG(image.stat().st_mode):
        raise RuntimeError('Prepared root must be an ordinary image file, never a device')
    handoff = staging/'host-report.json'
    if handoff.is_symlink() or not handoff.is_file():
        raise RuntimeError('Validated host preparation handoff is missing')
    host = json.loads(handoff.read_text())
    if (host.get('state') != 'running_vm' or host.get('writes_to_card') is not False or
            host.get('root_bytes') != image.stat().st_size or
            host.get('app_path') != app_path or
            not isinstance(host.get('original_backup'), str) or
            (Path(host['original_backup'])/'root.ext4').resolve() == image.resolve()):
        raise RuntimeError('Host preparation handoff does not describe this disposable image')
    backup = staging/'preparation-backup'
    if backup.exists() or backup.is_symlink():
        raise RuntimeError('Preparation backup already exists')
    root = Path('/tmp/cartridge-prep-mounted-root')
    root.mkdir(mode=0o700, exist_ok=True)
    if os.path.ismount(root):
        raise RuntimeError('Preparation mount is already busy')
    try:
        run('mount', '-t', 'ext4', '-o', 'loop', str(image), str(root))
        try:
            run('python3', '/tmp/cartridge-prep-offline_prepare.py', '--phase', 'root',
                '--root', str(root), '--bundle', str(bundle), '--backup', str(backup),
                '--app-path', app_path)
        finally:
            run('sync')
            run('umount', str(root))
    finally:
        if os.path.ismount(root):
            raise RuntimeError('Offline root remained mounted; preparation is incomplete')
    run('e2fsck', '-fn', str(image))
    report = {'state': 'root_prepared_and_verified', 'image_sha256': digest(image),
              'image_bytes': image.stat().st_size, 'app_path': app_path}
    (staging/'guest-report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
