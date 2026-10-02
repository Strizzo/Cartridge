#!/usr/bin/env python3
"""Fetch the original firmware's pinned Ubuntu WPA binaries without installing."""
import argparse
import hashlib
import io
from pathlib import Path
import tarfile
import urllib.request

URL = 'https://old-releases.ubuntu.com/ubuntu/pool/main/w/wpa/wpasupplicant_2.9-1ubuntu2_arm64.deb'
PACKAGE_SHA256 = 'fc1aa77412b3deb9fbe3cfd8f47b1e267a81bb0f38f6cc5bdb41371ea630c508'
EXPECTED = {
    'wpa_supplicant': '5cf389efbcd8dbab03522d3b916852f3e5efc94e049a8878ec62632dc90c0a9c',
    'wpa_cli': 'f90c690ada84e973f2b7e632fdafcf62e952a9050d234867011c4097f0e6d6cf',
}


def extract(package):
    if hashlib.sha256(package).hexdigest() != PACKAGE_SHA256:
        raise RuntimeError('Ubuntu package checksum mismatch')
    if not package.startswith(b'!<arch>\n'):
        raise RuntimeError('Invalid Debian package')
    offset = 8
    while offset < len(package):
        header = package[offset:offset+60]
        size = int(header[48:58])
        name = header[:16].decode().strip().rstrip('/')
        if name.startswith('data.tar'):
            with tarfile.open(fileobj=io.BytesIO(package[offset+60:offset+60+size])) as archive:
                data = {name: archive.extractfile('./sbin/'+name).read() for name in EXPECTED}
            for name, content in data.items():
                if hashlib.sha256(content).hexdigest() != EXPECTED[name]:
                    raise RuntimeError('Extracted binary checksum mismatch: '+name)
            return data
        offset += 60+size+(size%2)
    raise RuntimeError('Package has no binary data archive')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--package', type=Path, help='Use an already downloaded package; still verifies all checksums')
    args = parser.parse_args()
    try:
        if args.package:
            package = args.package.read_bytes()
        else:
            with urllib.request.urlopen(URL, timeout=30) as response:
                package = response.read(2*1024*1024)
        data = extract(package)
        args.output.mkdir(parents=True, exist_ok=True)
        for name, content in data.items():
            target = args.output/name
            if target.is_symlink():
                raise RuntimeError('Refusing symlink output: '+str(target))
            target.write_bytes(content)
        print('Verified original WPA binaries staged:', args.output)
    except Exception as exc:
        parser.exit(1, 'Fetch stopped: '+str(exc)+'\n')
