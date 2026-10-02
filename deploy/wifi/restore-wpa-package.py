#!/usr/bin/env python3
"""Restore the two known damaged WPA binaries on this ArkOS firmware."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

FILES = {
    'wpa_supplicant': {
        'damaged': 'a8352f9830fe588fd36363db66b215c48c457f1db7981e5507a1f3339189cb0a',
        'clean': '5cf389efbcd8dbab03522d3b916852f3e5efc94e049a8878ec62632dc90c0a9c',
        'md5': '318ca3f3fb2ae215c80777c24d55c521',
    },
    'wpa_cli': {
        'damaged': '4e218046e63994cb7c6ea128a2d52c7845a2b01371304bc4939b0a31068459bf',
        'clean': 'f90c690ada84e973f2b7e632fdafcf62e952a9050d234867011c4097f0e6d6cf',
        'md5': '81b943238986db79383ff8ab4e98d546',
    },
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def atomic_write(path, data, mode=0o755, owner=None):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=str(path.parent), delete=False) as f:
            temporary = Path(f.name)
            f.write(data)
            os.fchmod(f.fileno(), mode)
            if owner is not None:
                os.fchown(f.fileno(), *owner)
            f.flush()
            os.fsync(f.fileno())
        os.replace(str(temporary), str(path))
        temporary = None
        fd = os.open(str(path.parent), os.O_RDONLY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    finally:
        if temporary is not None:
            try:
                temporary.unlink()
            except FileNotFoundError:
                pass


def request_network_restart():
    result = {}
    try:
        subprocess.run(['systemctl', 'reset-failed', 'wpa_supplicant.service', 'NetworkManager.service'],
                       capture_output=True, timeout=5)
        service = subprocess.run(['systemctl', 'restart', '--no-block',
                                  'wpa_supplicant.service', 'NetworkManager.service'],
                                 capture_output=True, text=True, timeout=5)
        result['network_restart_requested'] = service.returncode == 0
        if service.returncode:
            result['network_restart_error'] = service.stderr.strip()
    except (OSError, subprocess.TimeoutExpired) as exc:
        result['network_restart_requested'] = False
        result['network_restart_error'] = str(exc)
    return result


def repair(root, payload, *, verify_only=False, restart=False):
    root, payload = root.resolve(), payload.resolve()
    live = root == Path('/')
    if live and not verify_only and os.geteuid() != 0:
        raise RuntimeError('Run with sudo on the handheld; no files changed')
    if restart and not live:
        raise RuntimeError('Offline rehearsal never controls host services')
    checksums = (root/'var/lib/dpkg/info/wpasupplicant.md5sums').read_text()
    originals, replacements, paths, stats = {}, {}, {}, {}
    for name, hashes in FILES.items():
        if not re.search(r'^'+hashes['md5']+r'\s+sbin/'+name+r'$', checksums, re.M):
            raise RuntimeError('Original firmware package does not match: '+name)
        staged = payload/name
        if staged.is_symlink() or not staged.is_file():
            raise RuntimeError('Expected regular payload file: '+name)
        replacements[name] = staged.read_bytes()
        if digest(replacements[name]) != hashes['clean']:
            raise RuntimeError('Clean payload checksum mismatch: '+name)
        path = root/'sbin'/name
        if path.is_symlink() or not path.is_file() or root not in path.resolve().parents:
            raise RuntimeError('Unexpected installed executable path: '+name)
        paths[name] = path
        originals[name] = path.read_bytes()
        if digest(originals[name]) not in (hashes['damaged'], hashes['clean']):
            raise RuntimeError('Installed binary is not the recorded damaged or clean copy: '+name)
        stats[name] = path.stat()
    changed = [name for name in FILES if digest(originals[name]) != FILES[name]['clean']]
    result = {'state': 'verification_only' if verify_only else 'already_repaired',
              'files_needing_repair': changed, 'hardware_verified': False}
    if verify_only or not changed:
        if restart and not verify_only:
            result.update(request_network_restart())
        return result
    if live:
        # The loader can execute an exFAT payload without changing its mode.
        version = subprocess.run([str(root/'lib/ld-linux-aarch64.so.1'),
                                  str(payload/'wpa_supplicant'), '-v'],
                                 capture_output=True, text=True, timeout=5)
        if version.returncode or not version.stdout.startswith('wpa_supplicant v2.9\n'):
            raise RuntimeError('Clean WPA cannot load with the device libraries; no files changed')
    base = root/'var/lib/cartridge/wpa-package-repair'
    base.mkdir(parents=True, exist_ok=True)
    backup = Path(tempfile.mkdtemp(prefix='backup-', dir=str(base)))
    manifest = {}
    for name in changed:
        atomic_write(backup/name, originals[name], 0o600)
        if digest((backup/name).read_bytes()) != FILES[name]['damaged']:
            raise RuntimeError('Backup verification failed; installed binaries unchanged')
        st = stats[name]
        manifest[name] = {'sha256': digest(originals[name]), 'uid': st.st_uid,
                          'gid': st.st_gid, 'mode': st.st_mode & 0o7777}
    atomic_write(backup/'manifest.json', (json.dumps(manifest, indent=2)+'\n').encode(), 0o600)
    attempted = []
    try:
        for name in changed:
            # Recheck immediately before replacement. Never overwrite an
            # independently updated executable between preflight and commit.
            if digest(paths[name].read_bytes()) != FILES[name]['damaged']:
                raise RuntimeError('Installed binary changed during repair: '+name)
            attempted.append(name)
            atomic_write(paths[name], replacements[name], owner=(0, 0) if live else None)
            if digest(paths[name].read_bytes()) != FILES[name]['clean']:
                raise RuntimeError('Installed repair checksum mismatch: '+name)
    except Exception:
        for name in reversed(attempted):
            st = stats[name]
            atomic_write(paths[name], originals[name], st.st_mode & 0o7777,
                         (st.st_uid, st.st_gid) if live else None)
        raise
    result.update(state='original_package_files_restored', backup=str(backup),
                  sha256={name: digest(paths[name].read_bytes()) for name in FILES})
    if restart:
        result.update(request_network_restart())
    atomic_write(base/'last-repair.json', (json.dumps(result, indent=2)+'\n').encode(), 0o644)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--payload', type=Path, default=Path(__file__).with_name('payload'))
    parser.add_argument('--root', type=Path, default=Path('/'), help='Offline rehearsal directory; never controls host services')
    parser.add_argument('--verify-only', action='store_true')
    parser.add_argument('--restart-network', action='store_true')
    args = parser.parse_args()
    print(json.dumps(repair(args.root, args.payload, verify_only=args.verify_only,
                            restart=args.restart_network), indent=2))


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        raise SystemExit('WPA repair stopped: '+str(exc))
