#!/usr/bin/env python3
"""Exact ARM payload + stable verifier + real SDL first frame and rollback rehearsal.

Run only in the disposable ARM VM; fixture must carry a production-key-signed
manifest for the actual CI payload. No test key or verification bypass exists.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
BUNDLE = ROOT / 'bundle/Cartridge'
FIXTURE = ROOT / 'system-update-fixture'


def wait_for(predicate, message, timeout=20):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.05)
    raise RuntimeError(message)


def main():
    if platform.machine() != 'aarch64' or not Path('/etc/cartridge-compat-vm').is_file():
        raise RuntimeError('Only run inside the disposable ARM compatibility VM')
    envelope = json.loads((FIXTURE / 'system-update.json').read_text())
    release = json.loads(envelope['payload'])
    archive = FIXTURE / release['archive']['url'].rsplit('/', 1)[-1]
    if archive.stat().st_size != release['archive']['size'] or hashlib.sha256(archive.read_bytes()).hexdigest() != release['archive']['sha256']:
        raise RuntimeError('Fixture archive mismatch')
    current_sha = hashlib.sha256((BUNDLE / 'cartridge').read_bytes()).hexdigest()
    if next(f['sha256'] for f in release['files'] if f['path'] == 'cartridge') != current_sha:
        raise RuntimeError('OTA fixture is not from this exact ARM build')
    release_id = release['version'] + '-' + release['revision'][:12]
    report = dict(release=release_id, cartridge_sha256=current_sha, checks=[])
    with tempfile.TemporaryDirectory(prefix='cartridge-real-ota-') as temporary:
        temporary = Path(temporary)
        app = temporary / 'Cartridge'
        # Only release-owned content. Never copy any host/user home or card data.
        app.mkdir()
        shutil.copy2(BUNDLE / 'cartridge', app / 'cartridge')
        shutil.copytree(BUNDLE / 'assets', app / 'assets')
        shutil.copytree(BUNDLE / 'lua_cartridges', app / 'lua_cartridges')
        shutil.copy2(BUNDLE / 'registry.json', app / 'registry.json')
        shutil.copy2(BUNDLE / 'game-library.py', app / 'game-library.py')
        state = temporary / 'state'
        state.mkdir()
        home = temporary / 'home'
        home.mkdir()
        candidate = app / 'releases' / release_id
        candidate.mkdir(parents=True)
        expected = {f['path'] for f in release['files']}
        seen = set()
        with tarfile.open(archive, 'r:gz') as source:
            for member in source:
                name = member.name
                if (not member.isfile() or name not in expected or name in seen or name.startswith('/')
                        or any(p in ('', '.', '..') for p in name.split('/'))):
                    raise RuntimeError('Unsafe fixture archive member')
                seen.add(name)
                dest = candidate / name
                dest.parent.mkdir(parents=True, exist_ok=True)
                with source.extractfile(member) as src, dest.open('xb') as dst:
                    shutil.copyfileobj(src, dst)
        if seen != expected:
            raise RuntimeError('Incomplete fixture archive')
        (candidate / 'system-release.json').write_text((FIXTURE / 'system-update.json').read_text())
        (candidate / 'cartridge').chmod(0o755)
        verify = [str(app / 'cartridge'), 'system-verify', '--path', str(candidate)]
        subprocess.run(verify, check=True)
        report['checks'].append('Production signature and every file verified by the exact ARM binary')
        saved = temporary / 'games/example.srm'
        saved.parent.mkdir()
        saved.write_bytes(b'fixture save must remain unchanged')
        fallback = temporary / 'es.sh'
        fallback.write_text('#!/bin/sh\ntouch "' + str(temporary / 'es-called') + '"\n')
        env = dict(os.environ, CARTRIDGE_SIM='1', CARTRIDGE_HOME=str(home),
                   CARTRIDGE_HIDDEN='1', CARTRIDGE_SOFTWARE='1', SDL_VIDEODRIVER='dummy',
                   CARTRIDGE_SIM_PROFILE=str(ROOT / 'sim/profiles/r36s-plus.json'))
        # Supervisor must set the candidate-specific assets; inherited VM paths cannot mask a bad release.
        env.pop('CARTRIDGE_ASSETS', None)
        env.pop('CARTRIDGE_READY_FILE', None)
        command = [sys.executable, str(ROOT / 'deploy/cartridge-session.py'), '--desktop',
                   '--cartridge-dir', str(app), '--state-dir', str(state), '--es-script', str(fallback)]
        processes = []

        def load_state():
            try:
                return json.loads((state / 'system-update.json').read_text())
            except FileNotFoundError:
                return {}

        def stage():
            value = dict(schema=1, active=None, previous=None, pending=release_id, trial=None, last_result='VM fixture staged')
            (state / 'system-update.json').write_text(json.dumps(value))

        def start():
            log = (temporary / 'harness.log').open('a')
            proc = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT)
            log.close()
            processes.append(proc)
            return proc

        def child(proc, directory):
            if proc.poll() is not None:
                raise RuntimeError('Supervisor exited unexpectedly; ' + (state / 'session.log').read_text()[-4000:])
            for path in Path('/proc').iterdir():
                if not path.name.isdigit():
                    continue
                try:
                    status = (path / 'status').read_text()
                    parent = next(line.split()[1] for line in status.splitlines() if line.startswith('PPid:'))
                    executable = (path / 'exe').resolve()
                    arguments = (path / 'cmdline').read_bytes().split(b'\0')
                    if b'system-verify' in arguments:
                        continue  # the trusted verifier is not a recovered launcher
                    if parent == str(proc.pid) and executable == directory / 'cartridge':
                        return int(path.name)
                except (OSError, StopIteration):
                    pass
            return None

        def stop(proc):
            proc.terminate()
            proc.wait(timeout=8)

        try:
            stage()
            proc = start()
            wait_for(lambda: child(proc, candidate), 'Candidate did not start')
            wait_for(lambda: load_state().get('active') == release_id, 'Real first frame did not promote after health grace')
            report['checks'].append('Real ARM/SDL first frame and five-second health grace promote the candidate')
            pid = child(proc, candidate)
            os.kill(pid, signal.SIGKILL)
            wait_for(lambda: load_state().get('active') is None and 'Rolled back' in load_state().get('last_result', ''), 'Promoted release crash did not roll back')
            wait_for(lambda: child(proc, app), 'Base binary did not recover')
            stop(proc)
            report['checks'].append('Crash after promotion restores the original working ARM release without ES')

            stage()
            proc = start()
            wait_for(lambda: child(proc, candidate), 'Trial did not start')
            stop(proc)  # shutdown before the five-second trial completes
            if load_state().get('trial') != release_id:
                raise RuntimeError('Interrupted trial was not durably recorded')
            proc = start()
            wait_for(lambda: child(proc, app), 'Interrupted trial did not recover original release')
            if load_state().get('trial') is not None or 'Interrupted' not in load_state().get('last_result', ''):
                raise RuntimeError('Interrupted trial state was not recovered')
            stop(proc)
            report['checks'].append('Shutdown during trial recovers the original release on the next boot')

            stage()
            damaged = candidate / 'registry.json'
            original = damaged.read_bytes()
            damaged.write_bytes(original + b' ')
            result = subprocess.run(verify, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            if result.returncode == 0:
                raise RuntimeError('Trusted verifier accepted altered release bytes')
            proc = start()
            wait_for(lambda: load_state().get('trial') is None and 'Rolled back' in load_state().get('last_result', ''), 'Damaged release did not record rollback')
            wait_for(lambda: child(proc, app), 'Damaged release did not recover original')
            stop(proc)
            report['checks'].append('Changed card payload is refused before execution and the original release starts')
            if (temporary / 'es-called').exists() or (state / 'fallback.json').exists():
                raise RuntimeError('A successful rollback incorrectly fell back to ES')
            if saved.read_bytes() != b'fixture save must remain unchanged':
                raise RuntimeError('Game save changed')
            report['checks'].append('Saves preserved; no ES fallback or failure latch after successful recovery')
        finally:
            for proc in processes:
                if proc.poll() is None:
                    stop(proc)
    report['hardware_performance_validated'] = False
    (ROOT / 'results/system-update-check.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
