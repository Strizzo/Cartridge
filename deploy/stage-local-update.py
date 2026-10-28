#!/usr/bin/env python3
"""Prepare/stage a local signed runtime using the installed bootstrap verifier.

For authenticated wireless administration, not an alternative download trust path.
The install kit must retain deploy/ and scripts/ from the same reviewed checkout.
Run as the owning Cartridge user on ARM64 Linux. The reviewed --root/cartridge
is the trust anchor, never a candidate executable. Keep the kit outside releases.
Preparing never selects a boot release and permits absent VM power telemetry
(but not a known low battery). --stage always requires charging or >=30%.
It only records a pending trial; the existing supervisor owns activation, health
checks and rollback.
"""
import argparse
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
from contextlib import contextmanager
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, str(path))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


package = module('cartridge_local_package', ROOT / 'scripts/package_system_update.py')
session = module('cartridge_local_session', Path(__file__).with_name('cartridge-session.py'))


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def regular(path):
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1,
            'Expected a single-link regular file (no symlink/hardlink): ' + str(path))


def directory(path):
    require(stat.S_ISDIR(path.lstat().st_mode) and not path.is_symlink(),
            'Expected a real directory: ' + str(path))


def check_platform():
    require(sys.platform == 'linux' and os.uname().machine in ('aarch64', 'arm64'),
            'Run on the ARM64 Linux handheld/VM')
    require(os.geteuid() != 0, 'Run as the normal Cartridge user')


@contextmanager
def locked(state):
    path = state / 'system-update.lock'
    fd = os.open(str(path), os.O_CREAT | os.O_RDWR | getattr(os, 'O_NOFOLLOW', 0)
                 | getattr(os, 'O_NONBLOCK', 0), 0o600)
    try:
        info = os.fstat(fd)
        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1
                and info.st_uid == os.geteuid() and not info.st_mode & 0o022,
                'Unsafe system update lock')
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('Another system update holds the lock; leave it unchanged')
        yield
    finally:
        os.close(fd)


def version(text):
    package.semver(text)
    return tuple(int(part) for part in text.split('.'))


def read_power():
    capacity, charging = None, False
    for supply in Path('/sys/class/power_supply').glob('*'):
        try:
            kind = (supply / 'type').read_text().strip()
            if kind == 'Battery':
                value = int((supply / 'capacity').read_text().strip())
                if 0 <= value <= 100:
                    capacity = value
                charging = charging or (supply / 'status').read_text().strip() in ('Charging', 'Full')
            elif kind in ('Mains', 'USB', 'USB_C', 'USB_PD'):
                charging = charging or (supply / 'online').read_text().strip() == '1'
        except (OSError, ValueError):
            continue
    return capacity, charging


def check_power(prepare_only=False):
    capacity, charging = read_power()
    if prepare_only and capacity is None and not charging:
        return  # ARM VMs may have no supplies; preparation cannot select a boot release.
    require(charging or (capacity is not None and capacity >= 30),
            'Connect the charger or charge to at least 30% before preparing/staging an update')


def trusted_command(root, *arguments):
    regular(root / 'cartridge')
    result = subprocess.run([str(root / 'cartridge')] + list(arguments), cwd=str(root),
                            capture_output=True, universal_newlines=True, timeout=60)
    require(result.returncode == 0,
            'Installed bootstrap refused the release: ' + result.stderr.strip()[:500])
    return result.stdout.strip()


def envelope(path):
    regular(path)
    text = package.read_metadata(path)
    outer = package.strict_json(text)
    package.exact_fields(outer, 'key_id payload signature')
    require(outer['key_id'] == package.KEY_ID and isinstance(outer['payload'], str),
            'Not a system-update signed envelope')
    require(isinstance(outer['signature'], str) and re.fullmatch(r'[0-9a-fA-F]{128}', outer['signature']),
            'Invalid Ed25519 signature shape')
    payload = package.strict_json(outer['payload'])
    package.validate_payload(payload)
    return text, payload


def read_state(state):
    path = state / 'system-update.json'
    regular(path)
    with package.regular_open(path) as source:
        raw = source.read(16385)
    require(len(raw) <= 16384, 'System update state is too large')
    return session.validate_state(package.strict_json(raw.decode('utf-8')))


def copy_archive(source, target, expected):
    regular(source)
    with package.regular_open(source) as incoming, target.open('xb') as output:
        remaining = expected['size']
        while remaining:
            block = incoming.read(min(1024 * 1024, remaining))
            require(block, 'Truncated local archive')
            output.write(block)
            remaining -= len(block)
        require(not incoming.read(1), 'Local archive exceeds its signed size')
        output.flush()
        os.fsync(output.fileno())


def unpack(archive, destination, payload):
    # This raw USTAR pass rejects PAX/GNU pseudo entries, links, special files,
    # duplicate/escaping paths, noncanonical modes, extra data and bad hashes.
    package.verify_archive(archive, payload)
    with tarfile.open(str(archive), 'r:gz', encoding='ascii', errors='strict') as source:
        for expected in payload['files']:
            item = source.next()
            require(item is not None and item.type == tarfile.REGTYPE
                    and not item.linkname and item.name == expected['path']
                    and item.size == expected['size'], 'Archive changed after validation')
            target = destination / item.name
            target.parent.mkdir(parents=True, exist_ok=True)
            with source.extractfile(item) as incoming, target.open('xb') as output:
                shutil.copyfileobj(incoming, output, 1024 * 1024)
                os.fchmod(output.fileno(), 0o755 if item.name == 'cartridge' else 0o644)
                output.flush()
                os.fsync(output.fileno())


def verify_candidate(root, candidate, text):
    directory(candidate)
    # The installed verifier checks signature, identity, exact file hashes and
    # tree scope. Additionally reject regular hardlinks, including the receipt.
    for parent, dirs, files in os.walk(str(candidate), followlinks=False):
        for name in dirs:
            directory(Path(parent) / name)
        for name in files:
            regular(Path(parent) / name)
    regular(candidate / 'system-release.json')
    require(package.read_metadata(candidate / 'system-release.json') == text,
            'A different immutable release already uses this identifier')
    trusted_command(root, 'system-verify', '--path', str(candidate))


def receipt_state(path):
    regular(path)
    with package.regular_open(path) as source:
        raw = source.read(16385)
    require(len(raw) <= 16384, 'Backup receipt is too large')
    return session.validate_state(package.strict_json(raw.decode('utf-8')))


def sync_metadata(path):
    # Reused receipts/pending files may come from a prior interrupted rename.
    # Reestablish the durability barrier before reporting success or committing.
    regular(path)
    with package.regular_open(path) as source:
        os.fsync(source.fileno())
    package.sync_directory(path.parent)


def pending_state(before, release):
    return dict(before, pending=release,
                last_result='Locally authenticated runtime ' + release + ' is ready for a supervised trial.')


def prepare(root, state, manifest, archive, *, stage=False, expected_active=None):
    check_platform()
    # Resolve a reviewed installation root, not an artifact-supplied path. Leaf
    # symlinks are refused before resolution so they cannot authorize writes.
    root, state = Path(root).absolute(), Path(state).absolute()
    directory(root)
    directory(state)
    root, state = root.resolve(), state.resolve()
    require(not session.beneath(root, state) and not session.beneath(state, root),
            'State and installation must be separate')
    session.check_state_location(state, False)
    info = state.stat()
    require(info.st_uid == os.geteuid() and not info.st_mode & 0o022,
            'State directory must be owned by the Cartridge user and not writable by others')
    text, payload = envelope(Path(manifest))
    release_id = package.release_id(payload['version'], payload['revision'])
    receipt = state / ('local-stage-before-' + release_id + '.json')
    regular(root / 'cartridge')
    with locked(state):
        before = read_state(state)
        require(before['trial'] is None and before['pending'] in (None, release_id),
                'Another update is pending or in trial; leave it unchanged')
        if expected_active is not None:
            require(before['active'] == expected_active, 'The active release changed; inspect before retrying')
        # A pending release from another writer is never adopted as our own.
        # Only an exact receipt/state pair authorizes an idempotent stage retry.
        already_pending = before['pending'] == release_id
        if already_pending:
            require(stage, 'This release is already pending; preparation does not change pending state')
            backup = receipt_state(receipt)
            require(backup['pending'] is None and backup['trial'] is None
                    and pending_state(backup, release_id) == before,
                    'Pending state does not match the original local-stage receipt')
        bootstrap = trusted_command(root, '--version')
        require(bootstrap.startswith('cartridge '), 'Unrecognized bootstrap executable')
        bootstrap_version = bootstrap[len('cartridge '):]
        require(version(bootstrap_version) >= version(payload['min_runtime']), 'Bootstrap is too old for this release')
        current = before['active'].rsplit('-', 1)[0] if before['active'] else bootstrap_version
        require(version(payload['version']) > version(current), 'Only a newer runtime may be staged')
        check_power(prepare_only=not stage)
        releases = root / 'releases'
        if not os.path.lexists(str(releases)):
            releases.mkdir()
            package.sync_directory(root)
        directory(releases)
        destination = releases / release_id
        if os.path.lexists(str(destination)):
            verify_candidate(root, destination, text)
        else:
            require(not already_pending, 'Pending candidate is missing; leave state unchanged')
            required = payload['archive']['size'] + payload['archive']['unpacked_size'] + 32 * 1024 * 1024
            require(shutil.disk_usage(str(releases)).free >= required, 'Not enough room for verified runtime staging')
            with tempfile.TemporaryDirectory(prefix='.local-incoming-', dir=str(releases)) as temporary:
                incoming = Path(temporary)
                copied = incoming / 'archive.tar.gz'
                copy_archive(Path(archive), copied, payload['archive'])
                # The installed verifier requires this exact signed basename,
                # including during private preparation before the final rename.
                candidate = incoming / release_id
                candidate.mkdir(mode=0o700)
                unpack(copied, candidate, payload)
                with (candidate / 'system-release.json').open('x', encoding='utf-8') as output:
                    output.write(text)
                    output.flush()
                    os.fsync(output.fileno())
                verify_candidate(root, candidate, text)
                for parent, _, _ in os.walk(str(candidate), topdown=False):
                    package.sync_directory(Path(parent))
                os.rename(str(candidate), str(destination))
                package.sync_directory(releases)
            # Authenticate the final path too. A completed rename alone never
            # authorizes pending state or candidate execution.
            verify_candidate(root, destination, text)
        require(read_state(state) == before, 'State changed while preparing; inspect before staging')
        result = dict(status='prepared', release=release_id, candidate=str(destination), active=before['active'])
        if stage:
            check_power()
            if already_pending:
                sync_metadata(receipt)
                sync_metadata(state / 'system-update.json')
                return dict(result, status='pending', backup=str(receipt))
            require(shutil.disk_usage(str(state)).free >= 2 * 1024 * 1024, 'Not enough room for update state')
            if os.path.lexists(str(receipt)):
                require(receipt_state(receipt) == before,
                        'Existing backup receipt differs from current state; leave it unchanged')
            else:
                # Persist a complete recovery receipt before the pending write.
                # atomic_json fsyncs both the new file and its containing directory.
                session.atomic_json(receipt, before)
            require(receipt_state(receipt) == before, 'Backup receipt did not verify')
            sync_metadata(receipt)
            require(read_state(state) == before, 'State changed before staging; leave it unchanged')
            session.atomic_json(state / 'system-update.json', pending_state(before, release_id))
            result.update(status='pending', backup=str(receipt))
        return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--state', type=Path, required=True)
    p.add_argument('--manifest', type=Path, required=True)
    p.add_argument('--archive', type=Path, required=True)
    p.add_argument('--expected-active')
    p.add_argument('--stage', action='store_true', help='Record pending after authentication; otherwise only prepare')
    args = p.parse_args()
    print(json.dumps(prepare(args.root, args.state, args.manifest, args.archive,
                             stage=args.stage, expected_active=args.expected_active), indent=2))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, tarfile.TarError, EOFError) as error:
        sys.exit('Local update stopped: ' + str(error))
