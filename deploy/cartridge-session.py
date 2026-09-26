#!/usr/bin/env python3
"""Primary handheld session: Cartridge first, stock ES on exit or startup failure."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time

ES_EXIT = 20


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode='w', dir=str(path.parent), delete=False) as f:
        json.dump(value, f); f.write('\n'); f.flush(); os.fsync(f.fileno())
        temporary = f.name
    os.replace(temporary, path)


def stop(child):
    # The frontend owns a separate process group including Python launch helpers
    # and emulators. Clean the group even if the frontend itself already exited.
    # Otherwise a crash could leave an emulator holding DRM while ES starts.
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        child.wait()
        return
    try:
        child.wait(timeout=3)
    except subprocess.TimeoutExpired:
        pass
    # Also terminate descendants which ignored SIGTERM after their parent died.
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


def supervise(command, cwd, env, ready, timeout, log):
    """First-frame deadline; shutdown signals never start another environment."""
    child = subprocess.Popen(command, cwd=str(cwd), env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    old = {}
    def shutting_down(signum, _frame):
        # Unwind child.wait() before calling stop() in finally; re-entering
        # Popen.wait from a signal handler can deadlock its waitpid lock.
        raise SystemExit(128 + signum)
    try:
        for sig in (signal.SIGTERM, signal.SIGINT):
            old[sig] = signal.signal(sig, shutting_down)
        deadline = time.monotonic() + timeout
        while child.poll() is None and not ready.is_file():
            if time.monotonic() >= deadline:
                stop(child)
                return 'startup_timeout', child.returncode
            time.sleep(0.05)
        appeared = ready.is_file()
        status = child.wait()
        if status == 30:
            return 'power_requested', status
        if status == ES_EXIT:
            return 'emulationstation_requested', status
        if not appeared:
            return 'startup_failed', status
        return ('closed' if status == 0 else 'crashed'), status
    finally:
        stop(child)
        for sig, handler in old.items():
            signal.signal(sig, handler)


def run(args):
    app = args.cartridge_dir.resolve()
    state = args.state_dir.resolve()
    state.mkdir(parents=True, exist_ok=True)
    report_path = state / 'last-session.json'
    failure = state / 'fallback.json'
    log_path = state / 'session.log'
    if log_path.exists() and log_path.stat().st_size > 1024 * 1024:
        os.replace(log_path, state / 'session.previous.log')
    env = os.environ.copy()
    # SDL settings apply only to Cartridge. Stock ES inherits its normal env.
    if not args.desktop:
        env.setdefault('SDL_VIDEODRIVER', 'kmsdrm')
        env.setdefault('SDL_AUDIODRIVER', 'alsa')
    env['CARTRIDGE_ASSETS'] = str(app / 'assets')
    env.setdefault('RUST_LOG', 'cartridge=info,cartridge_launcher=info,cartridge_core=info')
    reason, status = 'recovery_requested', None
    with log_path.open('a', buffering=1) as log:
        print('\nSession started: ' + time.strftime('%Y-%m-%d %H:%M:%S'), file=log)
        if failure.exists():
            reason = 'previous_failure'
        elif not (app / 'boot-emulationstation').exists():
            binary = app / 'cartridge'
            if not binary.is_file() or not (app / 'assets/fonts').is_dir():
                reason = 'installation_missing'
            else:
                try:
                    if not os.access(binary, os.X_OK):
                        binary.chmod(binary.stat().st_mode | 0o111)
                    with tempfile.TemporaryDirectory(prefix='cartridge-session-') as tmp:
                        ready = Path(tmp) / 'ready'
                        env['CARTRIDGE_READY_FILE'] = str(ready)
                        reason, status = supervise([str(binary)], app, env, ready, args.startup_timeout, log)
                except OSError as exc:
                    print('Cannot start Cartridge: ' + str(exc), file=log)
                    reason = 'startup_failed'
        report = {'reason': reason, 'exit_code': status, 'time': time.time(),
                  'cartridge_dir': str(app), 'fallback': str(args.es_script)}
        atomic_json(report_path, report)
        if reason in ('startup_timeout', 'startup_failed', 'crashed'):
            # Stop an unhealthy installation retrying on every boot. A manual
            # launch still works; rerunning setup clears this latch explicitly.
            atomic_json(failure, report)
        if reason == 'power_requested':
            print('Power action requested; session stopped.', file=log)
            return
        print('Starting stock EmulationStation: ' + reason, file=log)
    if not args.es_script.is_file():
        raise RuntimeError('Stock EmulationStation script missing: ' + str(args.es_script))
    # No nested systemctl start: this is still the original stock ES service.
    os.execv('/bin/bash', ['bash', str(args.es_script)])


def parse_args():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cartridge-dir', type=Path, required=True)
    p.add_argument('--state-dir', type=Path, default=Path.home()/'.cartridges/session')
    p.add_argument('--es-script', type=Path, default=Path('/usr/bin/emulationstation/emulationstation.sh'))
    p.add_argument('--startup-timeout', type=float, default=20)
    p.add_argument('--desktop', action='store_true', help='Use native SDL drivers for simulator rehearsal')
    args = p.parse_args()
    if not 0 < args.startup_timeout <= 120:
        p.error('startup timeout must be between 0 and 120 seconds')
    return args


if __name__ == '__main__':
    try:
        args = parse_args()
        run(args)
    except Exception as exc:
        print('Cartridge session failed: ' + str(exc), file=sys.stderr)
        # Logging/state storage failures must not strand the handheld either.
        if 'args' in globals() and args.es_script.is_file():
            os.execv('/bin/bash', ['bash', str(args.es_script)])
        sys.exit(1)
