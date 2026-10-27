#!/usr/bin/env python3
"""Finalize a trusted, freshly built private engine tree before packaging.

This is not an untrusted archive extractor. Debian APT has already verified the
packages. Rebase chroot-absolute links so the resulting archive cannot place a
symlink outside its extraction directory, remove setuid/setgid, retain notices,
and record complete file hashes for verification at installation time.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import posixpath
import stat
import shutil
import struct
import subprocess


def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for data in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(data)
    return digest.hexdigest()


def static_arm64(path):
    with path.open('rb') as source:
        header = source.read(64)
        if header[:7] != b'\x7fELF\x02\x01\x01' or struct.unpack_from('<H', header, 18)[0] != 183:
            raise ValueError('Sandbox helper is not an ARM64 Linux ELF')
        offset = struct.unpack_from('<Q', header, 32)[0]
        size, count = struct.unpack_from('<HH', header, 54)
        if size != 56 or count > 256:
            raise ValueError('Invalid helper program headers')
        source.seek(offset)
        programs = source.read(size * count)
        if any(struct.unpack_from('<I', programs, i * size)[0] == 3 for i in range(count)):
            raise ValueError('The helper requires a dynamic loader; build it statically')


def clear_runtime_devices(root):
    """mmdebstrap creates /dev nodes; bubblewrap supplies a fresh /dev at run time."""
    device_dir = root / "dev"
    if device_dir.is_symlink():
        raise ValueError("The private rootfs /dev must be a directory")
    for directory, dirs, files in os.walk(device_dir, topdown=False, followlinks=False):
        for name in files + dirs:
            path = Path(directory) / name
            if path.is_symlink() or not path.is_dir():
                path.unlink()
            else:
                path.rmdir()


def normalize(root):
    root = root.resolve()
    entries = []
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in dirs + files:
            entries.append(Path(directory) / name)
    for path in entries:
        info = path.lstat()
        relative = path.relative_to(root)
        if stat.S_ISLNK(info.st_mode):
            old = os.readlink(path)
            virtual_parent = '/' + relative.parent.as_posix()
            virtual_target = posixpath.normpath(posixpath.join(virtual_parent, old))
            rebased = posixpath.relpath(virtual_target, virtual_parent)
            path.unlink()
            path.symlink_to(rebased)
        elif stat.S_ISREG(info.st_mode):
            path.chmod(stat.S_IMODE(info.st_mode) & ~0o6000)
        elif not stat.S_ISDIR(info.st_mode):
            raise ValueError('Special filesystem object in component: ' + str(relative))
    for path in entries:
        if path.is_symlink():
            target = path.resolve(strict=False)
            if os.path.commonpath([str(root), str(target)]) != str(root):
                raise ValueError('Symlink escapes component: ' + str(path.relative_to(root)))
    # USTAR has a fixed 100-byte link field. A few Debian CA aliases exceed
    # it; retain their exact certificate bytes as small regular files instead.
    # This avoids archive extensions and preserves all certificate trust data.
    for path in entries:
        if path.is_symlink() and len(os.readlink(path).encode('utf-8')) > 100:
            target = path.resolve(strict=True)
            if not target.is_file() or target.stat().st_size > 16 * 1024 * 1024:
                raise ValueError('Long link is not a small regular file: ' + str(path.relative_to(root)))
            mode = stat.S_IMODE(target.stat().st_mode) & ~0o6000
            path.unlink()
            with target.open('rb') as source, path.open('xb') as output:
                shutil.copyfileobj(source, output)
            path.chmod(mode)


def inventory(root):
    result = []
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in sorted(dirs + files):
            path = Path(directory) / name
            relative = path.relative_to(root).as_posix()
            if relative == 'component.json':
                continue
            info = path.lstat()
            if stat.S_ISLNK(info.st_mode):
                result.append(dict(path=relative, type='symlink', target=os.readlink(path)))
            elif stat.S_ISREG(info.st_mode):
                result.append(dict(path=relative, type='file', size=info.st_size,
                                   sha256=sha256(path), mode=stat.S_IMODE(info.st_mode)))
    return sorted(result, key=lambda entry: entry['path'])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('component', type=Path)
    args = parser.parse_args()
    component = args.component.resolve()
    if platform.machine() not in ('aarch64', 'arm64') or os.geteuid() != 0:
        raise ValueError('Finalize inside an ARM64 Linux build environment as root')
    if not (component / '.cartridge-browser-build').is_file():
        raise ValueError('Not a new engine build directory')
    root = component / 'rootfs'
    static_arm64(component / 'bubblewrap')
    for path in ('profile', 'etc', 'tmp', 'run', 'proc', 'dev'):
        (root / path).mkdir(exist_ok=True, parents=True)
    for path in ('etc/hosts', 'etc/resolv.conf'):
        target = root / path
        if target.is_symlink():
            target.unlink()
        target.write_text('')
    packages = subprocess.check_output(['chroot', str(root), 'dpkg-query', '-W',
        '-f=${binary:Package}\t${Version}\n'], text=True)
    sources = subprocess.check_output(['chroot', str(root), 'dpkg-query', '-W',
        '-f=${source:Package}\t${source:Version}\n'], text=True)
    (component / 'source-packages.tsv').write_text(''.join(line + '\n' for line in sorted(set(sources.splitlines()))))
    clear_runtime_devices(root)
    normalize(root)
    (component / 'packages.tsv').write_text(packages)
    version = next(line.split('\t')[1] for line in packages.splitlines()
                   if line.split('\t')[0] == 'chromium')
    files = inventory(component)
    document = dict(schema=1, target='r36s-plus-aarch64', min_runtime='0.8.0',
                    chromium_version=version, debian_suite='bookworm',
                    bubblewrap_version='0.13.1', files=files,
                    unpacked_bytes=sum(f.get('size', 0) for f in files),
                    sandbox='user namespaces and Chromium namespace/seccomp; no setuid helper',
                    hardware_validated=False)
    (component / 'component.json').write_text(json.dumps(document, indent=2) + '\n')
    print('Component inventory:', len(files), 'entries;', document['unpacked_bytes'], 'bytes')


if __name__ == '__main__':
    main()
