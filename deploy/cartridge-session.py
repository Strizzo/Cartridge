#!/usr/bin/env python3
"""Primary handheld session: Cartridge first, stock ES on exit or startup failure."""
import argparse
from contextlib import contextmanager
import fcntl
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import sys
import tempfile
import time

ES_EXIT = 20
UPDATE_EXIT = 40
HEALTH_GRACE = 5.0
VERIFY_TIMEOUT = 60.0
MAX_UPDATE_RESTARTS = 3
UPDATE_ENV = ('CARTRIDGE_UPDATE_ROOT', 'CARTRIDGE_UPDATE_STATE',
              'CARTRIDGE_UPDATE_TARGET', 'CARTRIDGE_UPDATE_SUPERVISOR')
STATE_KEYS = {'schema', 'active', 'previous', 'pending', 'trial', 'last_result'}
NUMBER = r'(?:0|[1-9][0-9]*)'
RELEASE_ID = re.compile(
    '(' + NUMBER + r')\.(' + NUMBER + r')\.(' + NUMBER + r')-[0-9a-f]{12}')


def atomic_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', dir=str(path.parent), delete=False) as f:
            temporary = f.name
            json.dump(value, f, ensure_ascii=False)
            f.write('\n')
            f.flush()
            os.fsync(f.fileno())
        os.replace(temporary, path)
        temporary = None
        directory = os.open(str(path.parent), os.O_RDONLY | getattr(os, 'O_DIRECTORY', 0))
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if temporary is not None:
            os.unlink(temporary)


class UpdateStateError(ValueError):
    pass


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise UpdateStateError('Duplicate system update state field: ' + key)
        result[key] = value
    return result


def validate_state(value):
    if not isinstance(value, dict) or set(value) != STATE_KEYS:
        raise UpdateStateError('Invalid system update state fields')
    if type(value['schema']) is not int or value['schema'] != 1:
        raise UpdateStateError('Unsupported system update state schema; leaving it unchanged')
    for key in ('active', 'previous', 'pending', 'trial'):
        release = value[key]
        if release is None:
            continue
        match = RELEASE_ID.fullmatch(release) if isinstance(release, str) and len(release) <= 100 else None
        if match is None or any(int(part) > 2**64 - 1 for part in match.groups()):
            raise UpdateStateError('Invalid system update release ID in ' + key)
    if not isinstance(value['last_result'], str) or len(value['last_result'].encode('utf-8')) > 8000:
        raise UpdateStateError('Invalid system update result')
    return value


def release_name(release):
    return release or 'the original installation'


class SystemUpdates:
    """All state mutations reread under the updater's shared advisory lock."""
    def __init__(self, app, state, log):
        self.app, self.state, self.log = app, state, log

    @contextmanager
    def locked(self):
        fd = os.open(str(self.state / 'system-update.lock'),
                     os.O_CREAT | os.O_RDWR | getattr(os, 'O_NOFOLLOW', 0), 0o600)
        try:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as exc:
                raise UpdateStateError('Another system update is in progress; using the original installation') from exc
            path = self.state / 'system-update.json'
            if path.is_symlink():
                raise UpdateStateError('System update state must not be a symlink')
            try:
                with path.open('rb') as source:
                    raw = source.read(16385)
            except FileNotFoundError:
                value = dict(schema=1, active=None, previous=None, pending=None,
                             trial=None, last_result='')
            else:
                if len(raw) > 16384:
                    raise UpdateStateError('System update state is too large')
                try:
                    value = validate_state(json.loads(raw.decode('utf-8'), object_pairs_hook=unique_object))
                except (ValueError, UnicodeError) as exc:
                    raise UpdateStateError('Cannot read system update state: ' + str(exc)) from exc
            yield value
        finally:
            os.close(fd)  # close releases flock, including on validation/write failure

    def save(self, value):
        atomic_json(self.state / 'system-update.json', validate_state(value))

    def select(self):
        with self.locked() as value:
            if value['trial'] is not None:
                interrupted = value['trial']
                value['trial'] = None
                if value['pending'] == interrupted:
                    value['pending'] = None
                value['last_result'] = ('Interrupted update trial ' + interrupted +
                                        '; rolled back to ' + release_name(value['active']) + '.')
                self.save(value)
                return value['active'], False, True
            if value['pending'] is not None:
                release = value['pending']
                if release == value['active']:
                    value['pending'] = None
                    value['last_result'] = 'Ignored update request for the already active release ' + release + '.'
                    self.save(value)
                    return release, False, False
                value['pending'], value['trial'] = None, release
                value['last_result'] = 'Trying update ' + release + '; waiting for a healthy first frame.'
                self.save(value)  # durable before verification or candidate execution
                return release, True, False
            return value['active'], False, False

    def promote(self, release):
        with self.locked() as value:
            if value['trial'] != release or value['active'] == release:
                raise UpdateStateError('Update trial changed before promotion')
            value['previous'], value['active'], value['trial'] = value['active'], release, None
            value['last_result'] = 'Update ' + release + ' is healthy and active.'
            self.save(value)  # preserve any concurrently staged pending release

    def rollback(self, release, reason):
        with self.locked() as value:
            if value['trial'] == release:
                value['trial'] = None
            elif value['active'] == release:
                value['active'], value['previous'] = value['previous'], None
            else:
                raise UpdateStateError('Running release changed before rollback')
            if value['pending'] == release:
                value['pending'] = None
            value['last_result'] = ('Rolled back ' + release + ' to ' +
                                    release_name(value['active']) + ': ' + reason + '.')
            self.save(value)
            return value['active']

    def finish_trial(self, release, reason):
        with self.locked() as value:
            if value['trial'] == release:
                value['trial'] = None
                value['last_result'] = ('Update trial ' + release + ' ended before its health check (' +
                                        reason + '); kept ' + release_name(value['active']) + '.')
                self.save(value)

    def result(self, message):
        with self.locked() as value:
            value['last_result'] = message
            self.save(value)

    def directory(self, release):
        if release is None:
            return self.app
        directory = self.app / 'releases' / release
        if directory.is_symlink() or directory.parent.is_symlink() or not directory.is_dir():
            raise UpdateStateError('Update release directory is missing or is a symlink')
        return directory

    def verify(self, directory, env):
        # Only the original installation is trusted to authenticate staged files.
        # Never ask the candidate executable to verify itself.
        child = subprocess.Popen([str(self.app / 'cartridge'), 'system-verify', '--path', str(directory)],
                                 cwd=str(self.app), env=env, stdout=self.log, stderr=subprocess.STDOUT,
                                 start_new_session=True)
        try:
            try:
                code = child.wait(timeout=VERIFY_TIMEOUT)
            except subprocess.TimeoutExpired as exc:
                raise UpdateStateError('Trusted release verification timed out') from exc
            if code != 0:
                raise UpdateStateError('Trusted release verification failed (exit ' + str(code) + ')')
        finally:
            stop(child)


@contextmanager
def shutdown_signals():
    old = {}
    def shutting_down(signum, _frame):
        # Never enter Popen.wait from a signal handler: its waitpid lock can deadlock.
        raise SystemExit(128 + signum)
    try:
        for sig in (signal.SIGTERM, signal.SIGINT):
            old[sig] = signal.signal(sig, shutting_down)
        yield
    finally:
        for sig, handler in old.items():
            signal.signal(sig, handler)


def stop(child):
    # The frontend owns a separate process group including launch helpers and
    # emulators. Clean descendants even when the frontend itself already exited.
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        child.wait()
        return
    try:
        child.wait(timeout=3)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


def supervise(command, cwd, env, ready, timeout, log, on_healthy=None, health_grace=HEALTH_GRACE):
    """First-frame deadline; promotion requires readiness and a full alive grace."""
    with shutdown_signals():
        child = subprocess.Popen(command, cwd=str(cwd), env=env, stdout=log,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        try:
            deadline = time.monotonic() + timeout
            while child.poll() is None and not ready.is_file():
                if time.monotonic() >= deadline:
                    stop(child)
                    return 'startup_timeout', child.returncode
                time.sleep(0.05)
            appeared = ready.is_file()
            if appeared and on_healthy is not None:
                healthy_at = time.monotonic() + health_grace
                while child.poll() is None and time.monotonic() < healthy_at:
                    time.sleep(0.05)
                if child.poll() is None:
                    on_healthy()
            status = child.wait()
            if status == 30:
                return 'power_requested', status
            if status == ES_EXIT:
                return 'emulationstation_requested', status
            if status == UPDATE_EXIT:
                return 'update_restart_requested', status
            if not appeared:
                return 'startup_failed', status
            return ('closed' if status == 0 else 'crashed'), status
        finally:
            stop(child)


def beneath(path, root):
    return path == root or root in path.parents


def linux_filesystem(path):
    # mountinfo describes mounts without touching an SD/ROM mount itself.
    selected, depth = None, -1
    for line in Path('/proc/self/mountinfo').read_text().splitlines():
        before, after = line.split(' - ', 1)
        encoded = before.split()[4]
        mount = Path(re.sub(r'\\([0-7]{3})', lambda match: chr(int(match[1], 8)), encoded))
        if beneath(path, mount) and len(mount.parts) >= depth:
            selected, depth = after.split()[0], len(mount.parts)
    if selected is None:
        raise RuntimeError('Cannot determine the Linux session state filesystem')
    return selected


def check_state_location(state, desktop):
    # Production state belongs to Linux home, never the ROM/SD app root. Desktop
    # rehearsal also permits the OS temporary directory, on its own filesystem.
    if sys.platform != 'linux' and not desktop:
        return
    roots = [Path.home().resolve()]
    if desktop:
        roots.append(Path(tempfile.gettempdir()).resolve())
    ancestor = state
    while not ancestor.exists():
        ancestor = ancestor.parent
    if not any(beneath(state, root) and ancestor.stat().st_dev == root.stat().st_dev for root in roots):
        raise RuntimeError('Session state must stay on the home filesystem (or desktop temporary filesystem)')
    if sys.platform == 'linux' and linux_filesystem(state) in ('vfat', 'exfat', 'msdos', 'fuseblk'):
        raise RuntimeError('Session state must use a Linux filesystem, not FAT/exFAT')


def run_cartridge(args, app, state, env, log):
    supported = args.desktop or (sys.platform == 'linux' and platform.machine() in ('aarch64', 'arm64'))
    updates = SystemUpdates(app, state, log) if supported else None
    release, trial, recovered = None, False, False
    restarts = 0
    update_error = None
    for key in UPDATE_ENV:
        env.pop(key, None)

    def disable_updates(exc):
        nonlocal updates, update_error
        update_error = str(exc)
        print('System updates disabled: ' + update_error, file=log)
        updates = None
        for key in UPDATE_ENV:
            env.pop(key, None)

    def select():
        nonlocal recovered
        if updates is None:
            return None, False
        try:
            chosen, is_trial, interrupted = updates.select()
            recovered = recovered or interrupted
            return chosen, is_trial
        except (OSError, ValueError) as exc:
            disable_updates(exc)
            return None, False

    if updates is not None and (beneath(state, app) or beneath(app, state)):
        disable_updates(UpdateStateError('System update state must be separate from the installation'))
    release, trial = select()
    if updates is not None:
        env.update(CARTRIDGE_UPDATE_ROOT=str(app), CARTRIDGE_UPDATE_STATE=str(state),
                   CARTRIDGE_UPDATE_TARGET='r36s-plus-aarch64', CARTRIDGE_UPDATE_SUPERVISOR='1')
    while True:
        directory = app
        promoted = False
        attempt_error = None
        def healthy():
            nonlocal promoted
            updates.promote(release)
            promoted = True
        try:
            directory = updates.directory(release) if updates is not None else app
            if release is not None:
                updates.verify(directory, env)
            binary = directory / 'cartridge'
            if not binary.is_file() or not (directory / 'assets/fonts').is_dir():
                reason, status = 'installation_missing', None
            else:
                # exFAT cannot preserve archive mode bits. Only repair executable
                # permissions after the trusted verifier accepted all file bytes.
                if not os.access(binary, os.X_OK):
                    binary.chmod(binary.stat().st_mode | 0o111)
                with tempfile.TemporaryDirectory(prefix='cartridge-session-') as tmp:
                    ready = Path(tmp) / 'ready'
                    child_env = dict(env, CARTRIDGE_READY_FILE=str(ready),
                                     CARTRIDGE_ASSETS=str(directory / 'assets'))
                    reason, status = supervise([str(binary)], directory, child_env, ready,
                                               args.startup_timeout, log, healthy if trial else None)
        except (OSError, ValueError) as exc:
            print('Cannot run Cartridge: ' + str(exc), file=log)
            reason, status = 'startup_failed', None
            update_error = str(exc)
            attempt_error = str(exc)

        intentional = reason in ('power_requested', 'emulationstation_requested', 'update_restart_requested')
        if reason == 'power_requested':
            args._stop_requested = True
        if trial and not promoted and intentional and updates is not None:
            try:
                updates.finish_trial(release, reason.replace('_', ' '))
            except (OSError, ValueError) as exc:
                disable_updates(exc)
        if reason == 'update_restart_requested':
            restarts += 1
            if restarts > MAX_UPDATE_RESTARTS:
                reason = 'update_restart_limit'
                args._stop_requested = True
                if updates is not None:
                    try:
                        updates.result('Stopped after repeated update restart requests; no environment handoff was started.')
                    except (OSError, ValueError) as exc:
                        disable_updates(exc)
                return reason, status, directory, update_error
            # Each explicit restart begins a new update transaction. A prior
            # recovery must not prevent a newly staged trial from rolling back.
            recovered = False
            release, trial = select()
            continue

        failed = reason in ('startup_timeout', 'startup_failed', 'crashed', 'installation_missing')
        failed = failed or (trial and not promoted and not intentional)
        if release is not None and failed:
            detail = attempt_error or reason.replace('_', ' ')
            if trial and not promoted and reason == 'closed':
                detail = 'exited before completing the health check'
            elif status is not None:
                detail += ' (exit ' + str(status) + ')'
            if not recovered:
                recovered = True
                try:
                    release = updates.rollback(release, detail)
                except (OSError, ValueError) as exc:
                    disable_updates(exc)
                    release = None
                trial = False
                continue
            if updates is not None:
                try:
                    updates.result('Recovery release ' + release_name(release) + ' also failed: ' + detail + '.')
                except (OSError, ValueError) as exc:
                    disable_updates(exc)
            # Even a clean exit before trial health is a failed recovery.
            if reason == 'closed':
                reason = 'startup_failed'
        elif recovered and failed and updates is not None:
            try:
                updates.result('Recovery of the original installation also failed: ' + reason.replace('_', ' ') + '.')
            except (OSError, ValueError) as exc:
                disable_updates(exc)
        return reason, status, directory, update_error


def run(args):
    # Cover verification and the gaps between children as well as frontend waits.
    args._stop_requested = False
    with shutdown_signals():
        try:
            run_session(args)
        except Exception:
            # Even log writes/close can fail after a power request. Remember the
            # handoff before any diagnostics so the outer handler cannot run ES.
            if not args._stop_requested:
                raise


def run_session(args):
    app = args.cartridge_dir.resolve()
    state = args.state_dir.resolve()
    check_state_location(state, args.desktop)
    state.mkdir(parents=True, exist_ok=True)
    report_path = state / 'last-session.json'
    failure = state / 'fallback.json'
    log_path = state / 'session.log'
    if log_path.exists() and log_path.stat().st_size > 1024 * 1024:
        os.replace(log_path, state / 'session.previous.log')
    env = os.environ.copy()
    if not args.desktop:
        env.setdefault('SDL_VIDEODRIVER', 'kmsdrm')
        env.setdefault('SDL_AUDIODRIVER', 'alsa')
    env.setdefault('RUST_LOG', 'cartridge=info,cartridge_launcher=info,cartridge_core=info')
    reason, status = 'recovery_requested', None
    directory, update_error = app, None
    with log_path.open('a', buffering=1) as log:
        print('\nSession started: ' + time.strftime('%Y-%m-%d %H:%M:%S'), file=log)
        if failure.exists():
            reason = 'previous_failure'
        elif not (app / 'boot-emulationstation').exists():
            reason, status, directory, update_error = run_cartridge(args, app, state, env, log)
        report = {'reason': reason, 'exit_code': status, 'time': time.time(),
                  'cartridge_dir': str(directory), 'update_root': str(app), 'fallback': str(args.es_script)}
        if update_error:
            report['update_error'] = update_error
        if reason in ('power_requested', 'update_restart_limit'):
            args._stop_requested = True
            # A failed diagnostic write must not turn an intentional shutdown
            # into the outer exception handler's EmulationStation handoff.
            try:
                atomic_json(report_path, report)
            except OSError as exc:
                print('Cannot save session report: ' + str(exc), file=log)
            print('Session stopped: ' + reason, file=log)
            return
        atomic_json(report_path, report)
        if reason in ('startup_timeout', 'startup_failed', 'crashed') or (reason == 'installation_missing' and directory != app):
            # Only latch after recovery has also failed.
            atomic_json(failure, report)
        print('Starting stock EmulationStation: ' + reason, file=log)
    if not args.es_script.is_file():
        raise RuntimeError('Stock EmulationStation script missing: ' + str(args.es_script))
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
        if 'args' in globals() and args.es_script.is_file():
            os.execv('/bin/bash', ['bash', str(args.es_script)])
        sys.exit(1)
