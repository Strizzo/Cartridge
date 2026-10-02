"""OTA session transactions, using local executables and a trusted verifier stub."""
import argparse
import fcntl
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SESSION = ROOT / 'deploy/cartridge-session.py'
spec = importlib.util.spec_from_file_location('system_session', SESSION)
session = importlib.util.module_from_spec(spec)
spec.loader.exec_module(session)
OLD = '0.6.0-111111111111'
NEW = '0.6.1-222222222222'
NEXT = '0.6.2-333333333333'
READY = "Path(os.environ['CARTRIDGE_READY_FILE']).touch()\n"


class SystemSessionTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='cartridge-system-test-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        self.app = self.root / 'Cartridge'
        self.state = self.root / 'state'
        self.state.mkdir()
        self.path = self.state / 'system-update.json'
        self.events_path = self.root / 'events.jsonl'
        self.policy = self.root / 'verify-policy.json'
        self.policy.write_text('{}')
        self.es_seen = self.root / 'es-seen'
        self.es = self.root / 'es.sh'
        self.es.write_text('#!/bin/bash\ntouch "$ES_SEEN"\n')
        self.env = dict(os.environ, ES_SEEN=str(self.es_seen), PYTHONDONTWRITEBYTECODE='1')
        self.binary(None, READY + 'raise SystemExit(30)')

    def binary(self, release, body):
        directory = self.app if release is None else self.app / 'releases' / release
        (directory / 'assets/fonts').mkdir(parents=True, exist_ok=True)
        # Verifier policy lives outside the staged tree. Candidates deliberately
        # reject verification arguments: only the base executable may verify.
        prelude = ('#!' + sys.executable + '\nimport fcntl,json,os,sys,time\nfrom pathlib import Path\n'
                   'state_path=Path(' + repr(str(self.path)) + ')\n'
                   'events_path=Path(' + repr(str(self.events_path)) + ')\n'
                   'def record(kind):\n'
                   '    try: state=json.loads(state_path.read_text())\n'
                   '    except (ValueError,OSError): state=None\n'
                   '    lock=os.open(str(state_path.parent/"system-update.lock"),os.O_CREAT|os.O_RDWR,0o600)\n'
                   '    try: fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)\n'
                   '    except BlockingIOError:\n'
                   '        if "CARTRIDGE_UPDATE_SUPERVISOR" in os.environ: raise SystemExit("supervisor held lock while running child")\n'
                   '    finally: os.close(lock)\n'
                   '    event=dict(kind=kind,release=' + repr(release) + ',pid=os.getpid(),time=time.monotonic(),'
                   'cwd=os.getcwd(),args=sys.argv[1:],state=state,env={k:v for k,v in os.environ.items() '
                   'if k.startswith("CARTRIDGE_UPDATE_") or k=="CARTRIDGE_ASSETS"})\n'
                   '    with events_path.open("a") as f: f.write(json.dumps(event)+"\\n")\n')
        if release is None:
            prelude += ('if sys.argv[1:2]==["system-verify"]:\n'
                        '    record("verify")\n'
                        '    assert sys.argv[2]=="--path"\n'
                        '    policy=json.loads(Path(' + repr(str(self.policy)) + ').read_text()).get(Path(sys.argv[3]).name,0)\n'
                        '    if policy=="hang": time.sleep(30)\n'
                        '    raise SystemExit(policy)\n')
        prelude += 'assert not sys.argv[1:], "candidate must never verify itself"\nrecord("run")\n'
        path = directory / 'cartridge'
        path.write_text(prelude + body + '\n')
        path.chmod(0o755)
        return directory

    def write_state(self, **changes):
        value = dict(schema=1, active=None, previous=None, pending=None, trial=None, last_result='')
        value.update(changes)
        session.atomic_json(self.path, value)
        return value

    def state_value(self):
        return json.loads(self.path.read_text())

    def events(self, kind=None):
        if not self.events_path.exists():
            return []
        events = [json.loads(line) for line in self.events_path.read_text().splitlines()]
        return [event for event in events if kind is None or event['kind'] == kind]

    def start(self, timeout='0.5', verify_timeout=None):
        command = [sys.executable, str(SESSION)]
        if verify_timeout is not None:
            # Shorten only the test's verifier wait; production uses 60 seconds.
            command = [sys.executable, '-c',
                       'import importlib.util; s=importlib.util.spec_from_file_location("session",' + repr(str(SESSION)) + '); '
                       'm=importlib.util.module_from_spec(s); s.loader.exec_module(m); '
                       'm.VERIFY_TIMEOUT=' + repr(verify_timeout) + '; m.run(m.parse_args())']
        command += ['--cartridge-dir', str(self.app), '--state-dir', str(self.state),
                    '--es-script', str(self.es), '--startup-timeout', timeout, '--desktop']
        proc = subprocess.Popen(command, env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(self.cleanup_process, proc)
        return proc

    def cleanup_process(self, proc):
        if proc.poll() is None:
            proc.terminate()
        try:
            proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.communicate()
        for event in self.events():
            try:
                os.killpg(event['pid'], signal.SIGKILL)
            except ProcessLookupError:
                pass

    def launch(self, **kwargs):
        proc = self.start(**kwargs)
        _, stderr = proc.communicate(timeout=12)
        self.assertEqual(proc.returncode, 0, stderr)

    def wait_for(self, predicate, timeout=3):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return
            time.sleep(0.02)
        self.fail('Timed out waiting for session state/events: ' + repr(self.events()))

    def assert_recovered(self, expected=None):
        self.assertEqual([event['release'] for event in self.events('run')][-1], expected)
        self.assertFalse(self.es_seen.exists())
        self.assertFalse((self.state / 'fallback.json').exists())
        self.assertIsNone(self.state_value()['trial'])

    def test_missing_state_uses_base_and_exports_update_contract(self):
        self.launch()
        self.assertFalse(self.path.exists())
        event = self.events('run')[0]
        self.assertEqual(event['cwd'], str(self.app))
        self.assertEqual(event['env'], dict(CARTRIDGE_UPDATE_ROOT=str(self.app),
                                          CARTRIDGE_UPDATE_STATE=str(self.state),
                                          CARTRIDGE_UPDATE_TARGET='r36s-plus-aarch64',
                                          CARTRIDGE_UPDATE_SUPERVISOR='1',
                                          CARTRIDGE_ASSETS=str(self.app / 'assets')))
        self.assertFalse(self.es_seen.exists())

    def test_trial_is_durable_before_trusted_verify_and_early_crash_recovers(self):
        self.binary(OLD, READY + 'raise SystemExit(30)')
        self.binary(NEW, READY + 'time.sleep(0.1)\nraise SystemExit(9)')
        self.write_state(active=OLD, pending=NEW)
        self.launch()
        self.assertEqual([(e['kind'], e['release']) for e in self.events()],
                         [('verify', None), ('run', NEW), ('verify', None), ('run', OLD)])
        verification = self.events()[0]
        self.assertEqual(verification['state']['active'], OLD)
        self.assertEqual(verification['state']['trial'], NEW)
        self.assertIsNone(verification['state']['pending'])
        self.assertEqual(verification['cwd'], str(self.app))
        self.assertEqual(verification['args'], ['system-verify', '--path', str(self.app / 'releases' / NEW)])
        self.assertEqual(self.state_value()['active'], OLD)
        self.assertIn('crashed', self.state_value()['last_result'])
        self.assertTrue((self.app / 'releases' / OLD / 'cartridge').is_file())
        self.assertTrue((self.app / 'releases' / NEW / 'cartridge').is_file())
        self.assert_recovered(OLD)

    def test_first_frame_timeout_recovers_base(self):
        self.binary(NEW, 'time.sleep(30)')
        self.write_state(pending=NEW)
        self.launch(timeout='0.15')
        self.assertIn('startup timeout', self.state_value()['last_result'])
        self.assert_recovered()

    def test_exit_before_first_frame_recovers_base(self):
        self.binary(NEW, 'raise SystemExit(7)')
        self.write_state(pending=NEW)
        self.launch()
        self.assertIn('startup failed', self.state_value()['last_result'])
        self.assert_recovered()

    def test_clean_early_exit_cannot_promote_trial(self):
        self.binary(NEW, READY + 'raise SystemExit(0)')
        self.write_state(pending=NEW)
        self.launch()
        self.assertIsNone(self.state_value()['active'])
        self.assert_recovered()

    def test_promotion_waits_five_seconds_and_preserves_concurrent_pending(self):
        directory = self.binary(NEW, READY + 'time.sleep(30)')
        self.write_state(active=OLD, pending=NEW)
        proc = self.start()
        self.wait_for(lambda: len(self.events('run')) == 1)
        launch = self.events('run')[0]
        self.assertEqual(launch['cwd'], str(directory))
        self.assertEqual(launch['env']['CARTRIDGE_ASSETS'], str(directory / 'assets'))
        self.assertEqual(launch['env']['CARTRIDGE_UPDATE_ROOT'], str(self.app))
        self.assertEqual(launch['env']['CARTRIDGE_UPDATE_STATE'], str(self.state))
        # Simulate another state writer. The actual updater rejects busy trials,
        # but promotion must still reread and preserve unrelated pending work.
        with session.SystemUpdates(self.app, self.state, io.StringIO()).locked() as value:
            self.assertEqual(value['active'], OLD)
            self.assertEqual(value['trial'], NEW)
            value['pending'] = NEXT
            session.atomic_json(self.path, value)
        time.sleep(0.2)
        self.assertEqual(self.state_value()['active'], OLD)
        self.wait_for(lambda: self.state_value()['active'] == NEW, timeout=7)
        self.assertGreaterEqual(time.monotonic() - launch['time'], 5.0)
        value = self.state_value()
        self.assertEqual(value['previous'], OLD)
        self.assertEqual(value['pending'], NEXT)
        self.assertIsNone(value['trial'])
        proc.terminate()
        proc.communicate(timeout=5)
        self.assertFalse(self.es_seen.exists())

    def test_promotion_then_later_crash_rolls_back_base(self):
        self.binary(NEW, READY + 'time.sleep(5.4)\nraise SystemExit(9)')
        self.write_state(pending=NEW)
        self.launch()
        self.assertIsNone(self.state_value()['active'])
        self.assert_recovered()

    def test_interrupted_trial_does_not_reattempt(self):
        self.binary(NEW, READY + 'time.sleep(30)')
        self.write_state(pending=NEW)
        proc = self.start()
        self.wait_for(lambda: len(self.events('run')) == 1)
        self.assertEqual(self.state_value()['trial'], NEW)
        proc.kill()
        proc.communicate(timeout=5)
        os.killpg(self.events('run')[0]['pid'], signal.SIGKILL)
        self.launch()
        self.assertEqual([e['release'] for e in self.events('run')], [NEW, None])
        self.assertIn('Interrupted', self.state_value()['last_result'])
        self.assert_recovered()

    def test_preexisting_trial_returns_to_active_not_previous(self):
        self.binary(OLD, READY + 'raise SystemExit(30)')
        self.write_state(active=OLD, previous=NEXT, trial=NEW)
        self.launch()
        self.assertEqual([e['release'] for e in self.events('run')], [OLD])
        self.assertEqual(self.state_value()['previous'], NEXT)
        self.assert_recovered(OLD)

    def test_rejected_signature_never_executes_candidate(self):
        self.binary(NEW, 'raise RuntimeError("must not execute")')
        self.policy.write_text(json.dumps({NEW: 1}))
        self.write_state(pending=NEW)
        self.launch()
        self.assertEqual([e['release'] for e in self.events('run')], [None])
        self.assertIn('verification failed', self.state_value()['last_result'])
        self.assert_recovered()

    def test_verifier_timeout_is_bounded_and_recovers(self):
        self.binary(NEW, 'raise RuntimeError("must not execute")')
        self.policy.write_text(json.dumps({NEW: 'hang'}))
        self.write_state(pending=NEW)
        before = time.monotonic()
        self.launch(verify_timeout=0.15)
        self.assertLess(time.monotonic() - before, 3)
        self.assertIn('verification timed out', self.state_value()['last_result'])
        self.assert_recovered()

    def test_sigterm_during_verification_preserves_shutdown(self):
        self.binary(NEW, READY + 'time.sleep(30)')
        self.policy.write_text(json.dumps({NEW: 'hang'}))
        self.write_state(pending=NEW)
        proc = self.start()
        self.wait_for(lambda: len(self.events('verify')) == 1)
        proc.terminate()
        proc.communicate(timeout=5)
        self.assertEqual(proc.returncode, 128 + signal.SIGTERM)
        self.assertEqual(self.events('run'), [])
        self.assertFalse(self.es_seen.exists())
        self.assertEqual(self.state_value()['trial'], NEW)

    def test_explicit_power_and_es_handoffs_do_not_launch_recovery(self):
        for status in (30, 20):
            with self.subTest(status=status):
                self.events_path.unlink(missing_ok=True)
                self.es_seen.unlink(missing_ok=True)
                self.binary(NEW, 'raise SystemExit(' + str(status) + ')')
                self.write_state(pending=NEW)
                self.launch()
                self.assertEqual([e['release'] for e in self.events('run')], [NEW])
                self.assertEqual(self.es_seen.exists(), status == 20)
                self.assertIsNone(self.state_value()['active'])
                self.assertIsNone(self.state_value()['trial'])
                self.assertFalse((self.state / 'fallback.json').exists())

    def test_active_release_is_reverified_and_rolls_back_once(self):
        self.binary(NEW, READY + 'raise SystemExit(9)')
        self.binary(OLD, READY + 'raise SystemExit(30)')
        self.write_state(active=NEW, previous=OLD)
        self.launch()
        self.assertEqual([e['release'] for e in self.events('run')], [NEW, OLD])
        self.assertEqual(len(self.events('verify')), 2)
        self.assertEqual(self.state_value()['active'], OLD)
        self.assertIsNone(self.state_value()['previous'])
        self.assert_recovered(OLD)

    def test_recovery_failure_latches_es_without_third_launch(self):
        self.binary(NEW, READY + 'raise SystemExit(9)')
        self.binary(OLD, READY + 'raise SystemExit(8)')
        self.write_state(active=NEW, previous=OLD)
        self.launch()
        self.assertEqual([e['release'] for e in self.events('run')], [NEW, OLD])
        self.assertTrue(self.es_seen.exists())
        self.assertTrue((self.state / 'fallback.json').exists())
        self.assertIn('also failed', self.state_value()['last_result'])

    def test_update_exit_reselects_staged_state_in_same_supervisor(self):
        self.binary(None, 'value=json.loads(state_path.read_text())\n'
                    'value["pending"]=' + repr(NEW) + '\n'
                    'state_path.write_text(json.dumps(value))\nraise SystemExit(40)')
        self.binary(NEW, READY + 'raise SystemExit(30)')
        self.write_state()
        self.launch()
        self.assertEqual([e['release'] for e in self.events('run')], [None, NEW])
        self.assertFalse(self.es_seen.exists())
        self.assertFalse((self.state / 'fallback.json').exists())

    def test_repeated_update_exits_are_bounded_without_es(self):
        self.binary(None, 'raise SystemExit(40)')
        self.launch()
        self.assertEqual(len(self.events('run')), session.MAX_UPDATE_RESTARTS + 1)
        self.assertFalse(self.es_seen.exists())
        self.assertFalse((self.state / 'fallback.json').exists())
        self.assertIn('repeated update restart', self.state_value()['last_result'])

    def test_new_transaction_after_recovery_can_still_roll_back(self):
        # The recovered base deliberately stages a different update and restarts.
        self.binary(None, 'marker=state_path.parent/"already-staged"\n'
                    'if not marker.exists():\n'
                    '    marker.touch()\n'
                    '    value=json.loads(state_path.read_text())\n'
                    '    value["pending"]=' + repr(NEXT) + '\n'
                    '    state_path.write_text(json.dumps(value))\n'
                    '    raise SystemExit(40)\nraise SystemExit(30)')
        self.binary(NEW, 'raise SystemExit(9)')
        self.binary(NEXT, 'raise SystemExit(8)')
        self.write_state(pending=NEW)
        self.launch()
        self.assertEqual([e['release'] for e in self.events('run')], [NEW, None, NEXT, None])
        self.assert_recovered()

    def test_sigint_during_trial_does_not_launch_recovery_or_es(self):
        self.binary(NEW, READY + 'time.sleep(30)')
        self.write_state(pending=NEW)
        proc = self.start()
        self.wait_for(lambda: len(self.events('run')) == 1)
        proc.send_signal(signal.SIGINT)
        proc.communicate(timeout=5)
        self.assertEqual(proc.returncode, 128 + signal.SIGINT)
        self.assertEqual([e['release'] for e in self.events('run')], [NEW])
        self.assertFalse(self.es_seen.exists())
        self.assertEqual(self.state_value()['trial'], NEW)

    def test_invalid_state_remains_untouched_and_disables_update_ui(self):
        for raw in ('{"schema":2,"active":"unknown"}', '{',
                    json.dumps(dict(schema=1, active='../escape', previous=None, pending=None, trial=None, last_result=''))):
            with self.subTest(raw=raw):
                self.events_path.unlink(missing_ok=True)
                self.path.write_text(raw)
                self.env.update({key: 'inherited-unsafe' for key in session.UPDATE_ENV})
                self.launch()
                self.assertEqual(self.path.read_text(), raw)
                self.assertEqual(self.events('run')[0]['env'], {'CARTRIDGE_ASSETS': str(self.app / 'assets')})
                self.assertFalse(self.es_seen.exists())

    def test_symlink_release_is_not_verified_or_executed(self):
        (self.app / 'releases').mkdir()
        (self.app / 'releases' / NEW).symlink_to(self.app, target_is_directory=True)
        self.write_state(pending=NEW)
        self.launch()
        self.assertEqual(self.events('verify'), [])
        self.assert_recovered()

    def test_non_executable_candidate_gets_execute_bit_only_after_verification(self):
        directory = self.binary(NEW, READY + 'raise SystemExit(30)')
        binary = directory / 'cartridge'
        content = binary.read_bytes()
        binary.chmod(0o644)
        self.write_state(pending=NEW)
        self.launch()
        self.assertEqual(binary.stat().st_mode & 0o777, 0o755)
        self.assertEqual(binary.read_bytes(), content)
        self.assertEqual([e['kind'] for e in self.events()], ['verify', 'run'])
        self.assertFalse(self.es_seen.exists())

    def test_bad_verification_does_not_chmod_candidate(self):
        directory = self.binary(NEW, 'raise SystemExit(30)')
        binary = directory / 'cartridge'
        binary.chmod(0o644)
        self.policy.write_text(json.dumps({NEW: 1}))
        self.write_state(pending=NEW)
        self.launch()
        self.assertEqual(binary.stat().st_mode & 0o777, 0o644)
        self.assert_recovered()

    def test_busy_updater_lock_falls_back_base_without_waiting_or_state_writes(self):
        value = self.write_state(active=OLD, pending=NEW)
        with (self.state / 'system-update.lock').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            before = time.monotonic()
            self.launch()
            self.assertLess(time.monotonic() - before, 3)
        self.assertEqual(self.state_value(), value)
        self.assertEqual([e['release'] for e in self.events('run')], [None])
        self.assertEqual(self.events('run')[0]['env'], {'CARTRIDGE_ASSETS': str(self.app / 'assets')})
        self.assertFalse(self.es_seen.exists())

    def test_state_size_and_utf8_result_limits_leave_invalid_state_untouched(self):
        for raw in (' ' * 16385,
                    json.dumps(dict(schema=1, active=None, previous=None, pending=None, trial=None,
                                    last_result='é' * 4001), ensure_ascii=False)):
            with self.subTest(size=len(raw.encode('utf-8'))):
                self.events_path.unlink(missing_ok=True)
                self.path.write_text(raw)
                self.launch()
                self.assertEqual(self.path.read_text(), raw)
                self.assertNotIn('CARTRIDGE_UPDATE_SUPERVISOR', self.events('run')[0]['env'])

    def test_trial_state_write_failure_does_not_execute_candidate(self):
        before = self.write_state(pending=NEW)
        args = argparse.Namespace(desktop=True, startup_timeout=0.5)
        with (self.state / 'session.log').open('w') as log, patch.object(session, 'atomic_json', side_effect=OSError('disk full')):
            reason, _, _, _ = session.run_cartridge(args, self.app, self.state, self.env, log)
        self.assertEqual(reason, 'power_requested')
        self.assertEqual(self.state_value(), before)
        self.assertEqual([e['release'] for e in self.events('run')], [None])
        self.assertFalse(self.es_seen.exists())

    def test_state_inside_app_disables_updates_without_consuming_pending(self):
        state = self.app / 'state'
        state.mkdir()
        value = dict(schema=1, active=None, previous=None, pending=NEW, trial=None, last_result='')
        session.atomic_json(state / 'system-update.json', value)
        args = argparse.Namespace(desktop=True, startup_timeout=0.5)
        with (self.state / 'session.log').open('w') as log:
            reason, _, _, _ = session.run_cartridge(args, self.app, state, self.env, log)
        self.assertEqual(reason, 'power_requested')
        self.assertEqual(json.loads((state / 'system-update.json').read_text()), value)
        self.assertNotIn('CARTRIDGE_UPDATE_SUPERVISOR', self.events('run')[0]['env'])


class StateValidationTest(unittest.TestCase):
    def test_strict_state_schema_and_ids(self):
        valid = dict(schema=1, active=NEW, previous=None, pending=None, trial=None, last_result='')
        for key, bad in [('schema', 2), ('schema', True), ('schema', 1.0), ('last_result', None),
                         ('active', '.'), ('previous', '..'), ('pending', '../' + NEW),
                         ('trial', '0.6.1-' + 'a' * 40), ('active', '01.6.1-' + 'a' * 12),
                         ('active', '0.6.1-01-' + 'a' * 12), ('active', '0.6.1-α-' + 'a' * 12),
                         ('active', '0.6.1+' + 'a' * 100 + '-' + 'b' * 12), ('active', 42),
                         ('active', '0.6.1-' + 'A' * 12), ('active', '0.6.1-rc.1-' + 'a' * 12),
                         ('active', '0.6.1+build.1-' + 'a' * 12), ('active', '18446744073709551616.0.0-' + 'a' * 12),
                         ('last_result', 'é' * 4001)]:
            with self.subTest(key=key, bad=bad), self.assertRaises(session.UpdateStateError):
                session.validate_state(dict(valid, **{key: bad}))
        with self.assertRaises(session.UpdateStateError):
            session.validate_state(dict(valid, extra=True))
        with self.assertRaises(session.UpdateStateError):
            session.validate_state({'schema': 1})
        for version in ('0.6.1', '1.0.0', '18446744073709551615.0.0'):
            session.validate_state(dict(valid, active=version + '-abcdef012345'))

    def test_duplicate_fields_are_rejected(self):
        with self.assertRaises(session.UpdateStateError):
            json.loads('{"schema":2,"schema":1}', object_pairs_hook=session.unique_object)

    def test_atomic_replace_syncs_file_then_parent_and_cleans_failed_temp(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'state.json'
            calls = []
            original_sync, original_replace = os.fsync, os.replace
            def sync(fd):
                calls.append('sync')
                original_sync(fd)
            def replace(source, target):
                calls.append('replace')
                original_replace(source, target)
            with patch.object(session.os, 'fsync', side_effect=sync), patch.object(session.os, 'replace', side_effect=replace):
                session.atomic_json(path, {'schema': 1})
            self.assertEqual(calls, ['sync', 'replace', 'sync'])
            with patch.object(session.os, 'replace', side_effect=OSError('interrupted')), self.assertRaises(OSError):
                session.atomic_json(path, {'schema': 2})
            self.assertEqual(json.loads(path.read_text()), {'schema': 1})
            self.assertEqual(list(Path(tmp).iterdir()), [path])

    def test_device_state_must_be_home_filesystem_and_desktop_can_use_temp(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(session.sys, 'platform', 'linux'), \
                patch.object(session, 'linux_filesystem', return_value='ext4'):
            directory = Path(tmp).resolve() / 'new-state'
            with self.assertRaisesRegex(RuntimeError, 'home filesystem'):
                session.check_state_location(directory, False)
            session.check_state_location(directory, True)
            self.assertFalse(directory.exists())

    def test_linux_fat_and_exfat_state_are_rejected_before_directory_creation(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(session.sys, 'platform', 'linux'):
            directory = Path(tmp).resolve() / 'new-state'
            for filesystem in ('vfat', 'exfat', 'fuseblk'):
                with patch.object(session, 'linux_filesystem', return_value=filesystem), self.assertRaisesRegex(RuntimeError, 'FAT/exFAT'):
                    session.check_state_location(directory, True)
            self.assertFalse(directory.exists())

    def test_power_handoff_survives_report_storage_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            args = argparse.Namespace(desktop=True, cartridge_dir=Path(tmp), state_dir=Path(tmp), es_script=Path(tmp) / 'es.sh')
            with patch.object(session, 'run_cartridge', return_value=('power_requested', 30, Path(tmp), None)), \
                    patch.object(session, 'atomic_json', side_effect=OSError('disk full')), \
                    patch.object(session.os, 'execv') as handoff:
                session.run(args)
            handoff.assert_not_called()

    def test_power_handoff_survives_log_failure(self):
        args = argparse.Namespace()
        def fail_after_power(_args):
            _args._stop_requested = True
            raise OSError('logging failed after shutdown request')
        with patch.object(session, 'run_session', side_effect=fail_after_power):
            session.run(args)

    def test_state_writer_round_trips_maximum_utf8_result(self):
        with tempfile.TemporaryDirectory() as tmp:
            updates = session.SystemUpdates(Path(tmp), Path(tmp), io.StringIO())
            with updates.locked() as value:
                value['last_result'] = 'é' * 4000
                updates.save(value)
            with updates.locked() as value:
                self.assertEqual(value['last_result'], 'é' * 4000)

    def test_mountinfo_uses_deepest_mount_and_decodes_spaces(self):
        mounts = ('1 0 8:1 / / rw - ext4 /dev/root rw\n'
                  '2 1 8:2 / /home/test\\040user/state rw - exfat /dev/example rw\n')
        with patch.object(Path, 'read_text', return_value=mounts):
            self.assertEqual(session.linux_filesystem(Path('/home/test user/state/new')), 'exfat')
            self.assertEqual(session.linux_filesystem(Path('/home/test user/other')), 'ext4')

    def test_unsupported_mode_removes_inherited_update_capability(self):
        with tempfile.TemporaryDirectory() as tmp:
            app = Path(tmp)
            env = {key: 'unsafe' for key in session.UPDATE_ENV}
            args = argparse.Namespace(desktop=False, startup_timeout=1)
            with patch.object(session.sys, 'platform', 'darwin'):
                session.run_cartridge(args, app, app, env, io.StringIO())
            self.assertFalse(any(key in env for key in session.UPDATE_ENV))


if __name__ == '__main__':
    unittest.main()
