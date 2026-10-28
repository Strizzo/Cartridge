"""Trusted-bootstrap staging with real packages and separate generated test keys.

Platform/power are substituted only in this test process. The fixture bootstrap
is actually executed and authenticates Ed25519 signatures with OpenSSL 3; neither
trusted_command nor the production archive verifier is mocked. Filesystem mocks
below inject resource shortages and failures at atomic-write boundaries only.
"""
import ast
import copy
import fcntl
import gzip
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / 'deploy/stage-local-update.py'
spec = importlib.util.spec_from_file_location('local_update_stage', SCRIPT)
stager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stager)
package = stager.package
OLD = '0.8.0-111111111111'
PREVIOUS = '0.7.0-222222222222'
VERSION = '0.8.1'
REVISION = 'abcdef0123456789' * 2 + 'abcdef01'
NEW = package.release_id(VERSION, REVISION)

# This is the only executable the stager may invoke. The fixture's trust key is
# external to all candidate trees. Keep basename validation: it catches the
# original draft's otherwise nonfunctional .../payload verifier invocation.
BOOTSTRAP = r'''#!@PYTHON@
import fcntl, importlib.util, json, os, sys
from pathlib import Path
root, state = Path(@APP@), Path(@STATE@)
policy_path, events = Path(@POLICY@), Path(@EVENTS@)
policy = json.loads(policy_path.read_text())
fd = os.open(str(state / 'system-update.lock'), os.O_RDWR)
try:
    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
except BlockingIOError:
    pass
else:
    sys.exit('stager must retain the supervisor lock during verification')
finally:
    os.close(fd)
with events.open('a') as output:
    output.write(json.dumps(dict(executable=sys.argv[0], args=sys.argv[1:], cwd=os.getcwd())) + '\n')
if sys.argv[1:] == ['--version']:
    if policy.get('fail_version'): sys.exit('version refused')
    print(policy.get('version', 'cartridge 0.8.0'))
    sys.exit(0)
assert sys.argv[1:3] == ['system-verify', '--path'] and len(sys.argv) == 4
candidate = Path(sys.argv[3])
if policy.get('fail_verify') or (policy.get('fail_final') and candidate.parent == root / 'releases'):
    sys.exit('verification refused by trusted bootstrap')
s = importlib.util.spec_from_file_location('trusted_package', @PACKAGE@)
p = importlib.util.module_from_spec(s); s.loader.exec_module(p)
envelope = p.strict_json(p.read_metadata(candidate / 'system-release.json'))
p.exact_fields(envelope, 'key_id payload signature')
assert envelope['key_id'] == p.KEY_ID
p.signature_operation(envelope['payload'], Path(@PUBLIC@), envelope['signature'])
payload = p.strict_json(envelope['payload']); p.validate_payload(payload)
assert candidate.name == p.release_id(payload['version'], payload['revision']), 'signed basename required'
expected = {f['path'] for f in payload['files']} | {'system-release.json'}
actual = set()
for directory, dirs, files in os.walk(str(candidate), followlinks=False):
    for name in dirs:
        assert not (Path(directory) / name).is_symlink()
    for name in files:
        path = Path(directory) / name
        assert path.is_file() and not path.is_symlink()
        actual.add(path.relative_to(candidate).as_posix())
assert actual == expected
for entry in payload['files']:
    path = candidate / entry['path']
    assert path.stat().st_size == entry['size'] and p.sha256_file(path) == entry['sha256']
with (candidate / 'cartridge').open('rb') as executable:
    p.check_elf(executable.read(64))
if policy.get('mutate_state'):
    state.joinpath('system-update.json').write_text(json.dumps(policy['mutate_state']))
print('Verified by installed test-key bootstrap')
'''


def arm_elf():
    data = bytearray(64)
    data[:7] = b'\x7fELF\x02\x01\x01'
    struct.pack_into('<HHI', data, 16, 3, 183, 1)
    struct.pack_into('<H', data, 52, 64)
    return bytes(data) + b'never execute this candidate\n'


class LocalUpdateStageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.keys = tempfile.TemporaryDirectory(prefix='local-update-test-keys-')
        cls.openssl = os.environ.get('OPENSSL') or '/opt/homebrew/opt/openssl@3/bin/openssl'
        if not Path(cls.openssl).is_file():
            cls.openssl = shutil.which('openssl')
        if not cls.openssl or not subprocess.check_output([cls.openssl, 'version']).startswith(b'OpenSSL 3.'):
            raise unittest.SkipTest('OpenSSL 3 required for real test-key signatures')
        cls.private = Path(cls.keys.name) / 'test-private.pem'
        cls.public = Path(cls.keys.name) / 'test-public.pem'
        subprocess.run([cls.openssl, 'genpkey', '-algorithm', 'ED25519', '-out', str(cls.private)], check=True, capture_output=True)
        subprocess.run([cls.openssl, 'pkey', '-in', str(cls.private), '-pubout', '-out', str(cls.public)], check=True, capture_output=True)

    @classmethod
    def tearDownClass(cls):
        cls.keys.cleanup()

    def setUp(self):
        # Production state must remain on the user's Linux home filesystem.
        self.tmp = tempfile.TemporaryDirectory(prefix='.cartridge-local-update-test-', dir=Path.home())
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        self.app, self.state, self.games = self.root / 'app', self.root / 'state', self.root / 'games'
        for directory in (self.app, self.state, self.games):
            directory.mkdir(mode=0o700)
        self.path = self.state / 'system-update.json'
        self.initial = dict(schema=1, active=OLD, previous=PREVIOUS, pending=None, trial=None,
                            last_result='Keep the active runtime and its recovery release.')
        stager.session.atomic_json(self.path, self.initial)
        for release in (OLD, PREVIOUS):
            directory = self.app / 'releases' / release
            directory.mkdir(parents=True)
            (directory / 'cartridge').write_text('untouched installed ' + release)
        (self.games / 'game.srm').write_bytes(b'precious save\x00\xff')
        (self.games / 'game.rom').write_bytes(b'precious ROM')
        self.events_path, self.policy = self.root / 'events.jsonl', self.root / 'policy.json'
        self.policy.write_text('{}')
        bootstrap = BOOTSTRAP.replace('@PYTHON@', sys.executable)
        for token, path in [('APP', self.app), ('STATE', self.state), ('POLICY', self.policy),
                            ('EVENTS', self.events_path), ('PUBLIC', self.public),
                            ('PACKAGE', ROOT / 'scripts/package_system_update.py')]:
            bootstrap = bootstrap.replace('@' + token + '@', repr(str(path)))
        (self.app / 'cartridge').write_text(bootstrap)
        (self.app / 'cartridge').chmod(0o755)
        self.bundle = self.root / 'bundle'
        self.bundle.mkdir()
        self.files = {'cartridge': arm_elf(), 'game-library.py': b'# reviewed helper\n',
                      'registry.json': b'{"apps":[]}\n', 'assets/fonts/font.ttf': b'fixture font',
                      'assets/icons/home.png': b'fixture icon', 'lua_cartridges/clock/main.lua': b'return {}\n'}
        for name, data in self.files.items():
            path = self.bundle / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        self.addCleanup(mock.patch.stopall)
        mock.patch.dict(os.environ, OPENSSL=self.openssl, PYTHONDONTWRITEBYTECODE='1').start()
        mock.patch.object(stager, 'check_platform').start()
        self.power = mock.patch.object(stager, 'read_power', return_value=(80, False)).start()
        self.artifacts = self.root / 'artifacts'
        self.payload = package.build(self.bundle, self.artifacts, VERSION, REVISION,
                                     build_version=VERSION, sign_key=self.private, public_key=self.public)
        self.manifest = self.artifacts / 'system-update.json'
        self.archive = self.artifacts / package.archive_name(VERSION)
        self.destination = self.app / 'releases' / NEW
        self.receipt = self.state / ('local-stage-before-' + NEW + '.json')
        self.protected = {p: p.read_bytes() for p in [self.app / 'cartridge',
            self.app / 'releases' / OLD / 'cartridge', self.app / 'releases' / PREVIOUS / 'cartridge',
            self.games / 'game.srm', self.games / 'game.rom']}
        self.original_state = self.path.read_bytes()

    def run_stage(self, **kwargs):
        return stager.prepare(self.app, self.state, self.manifest, self.archive, **kwargs)

    def events(self):
        return [json.loads(line) for line in self.events_path.read_text().splitlines()] if self.events_path.exists() else []

    def assert_protected(self, state=True):
        for path, data in self.protected.items():
            self.assertEqual(path.read_bytes(), data, str(path))
        if state:
            self.assertEqual(self.path.read_bytes(), self.original_state)
        self.assertFalse(list((self.app / 'releases').glob('.local-incoming-*')))
        for event in self.events():
            self.assertEqual(event['executable'], str(self.app / 'cartridge'))
            self.assertEqual(event['cwd'], str(self.app))
            self.assertTrue(event['args'] == ['--version'] or event['args'][:2] == ['system-verify', '--path'])

    def sign_payload(self, payload):
        text = json.dumps(payload, separators=(',', ':')) + '\n'
        envelope = dict(key_id=package.KEY_ID, payload=text,
                        signature=package.signature_operation(text, self.private))
        self.manifest.write_text(json.dumps(envelope) + '\n')

    def changed_archive(self, raw):
        self.archive.write_bytes(gzip.compress(raw, mtime=0))
        changed = copy.deepcopy(self.payload)
        changed['archive'].update(size=self.archive.stat().st_size, sha256=package.sha256_file(self.archive))
        self.sign_payload(changed)

    def test_prepare_verifies_exact_signed_package_without_boot_selection_or_candidate_execution(self):
        result = self.run_stage(expected_active=OLD)
        self.assertEqual(result, dict(status='prepared', release=NEW, candidate=str(self.destination), active=OLD))
        self.assertFalse(self.receipt.exists())
        self.assertEqual((self.destination / 'system-release.json').read_bytes(), self.manifest.read_bytes())
        for name, data in self.files.items():
            path = self.destination / name
            self.assertEqual(path.read_bytes(), data)
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o755 if name == 'cartridge' else 0o644)
        paths = [Path(e['args'][2]) for e in self.events() if e['args'][0] == 'system-verify']
        self.assertEqual(len(paths), 2)
        self.assertEqual([path.name for path in paths], [NEW, NEW])
        self.assertEqual(paths[-1], self.destination)
        self.assertIn('.local-incoming-', str(paths[0]))
        self.assert_protected()

    def test_stage_records_only_pending_and_backup_then_unchanged_supervisor_selects_trial(self):
        result = self.run_stage(stage=True)
        self.assertEqual(result['status'], 'pending')
        self.assertEqual(json.loads(self.receipt.read_text()), self.initial)
        expected = stager.pending_state(self.initial, NEW)
        self.assertEqual(json.loads(self.path.read_text()), expected)
        self.assert_protected(state=False)
        updates = stager.session.SystemUpdates(self.app, self.state, io.StringIO())
        self.assertEqual(updates.select(), (NEW, True, False))
        value = json.loads(self.path.read_text())
        self.assertEqual((value['active'], value['previous'], value['trial'], value['pending']), (OLD, PREVIOUS, NEW, None))
        self.assert_protected(state=False)

    def test_prepared_reuse_and_pending_retry_are_verified_idempotent_without_archive(self):
        self.run_stage()
        inode = (self.destination / 'cartridge').stat().st_ino
        self.archive.unlink()
        self.run_stage()
        self.assert_protected()
        self.run_stage(stage=True)
        before = [(p.read_bytes(), p.stat().st_ino) for p in (self.path, self.receipt)]
        count = len(self.events())
        result = self.run_stage(stage=True)
        self.assertEqual(result['status'], 'pending')
        self.assertEqual([(p.read_bytes(), p.stat().st_ino) for p in (self.path, self.receipt)], before)
        self.assertEqual(len(self.events()) - count, 2)  # version + trusted re-verification
        self.assertEqual((self.destination / 'cartridge').stat().st_ino, inode)
        self.assert_protected(state=False)

    def test_trusted_bootstrap_failure_and_forged_signature_leave_boot_state_unchanged(self):
        self.policy.write_text('{"fail_verify":true}')
        with self.assertRaisesRegex(RuntimeError, 'bootstrap refused'):
            self.run_stage(stage=True)
        self.assertFalse(self.destination.exists())
        self.assertFalse(self.receipt.exists())
        self.assert_protected()
        self.policy.write_text('{}')
        envelope = json.loads(self.manifest.read_text())
        envelope['signature'] = '00' * 64
        self.manifest.write_text(json.dumps(envelope))
        with self.assertRaisesRegex(RuntimeError, 'bootstrap refused'):
            self.run_stage(stage=True)
        self.assertFalse(self.destination.exists())
        self.assertFalse(self.receipt.exists())
        self.assert_protected()

    def test_wrong_signing_key_is_rejected_by_real_bootstrap_crypto(self):
        other = self.root / 'other.pem'
        subprocess.run([self.openssl, 'genpkey', '-algorithm', 'ED25519', '-out', str(other)], check=True, capture_output=True)
        envelope = json.loads(self.manifest.read_text())
        envelope['signature'] = package.signature_operation(envelope['payload'], other)
        self.manifest.write_text(json.dumps(envelope))
        with self.assertRaisesRegex(RuntimeError, 'bootstrap refused'):
            self.run_stage(stage=True)
        self.assert_protected()
        self.assertFalse(self.destination.exists())

    def test_final_path_verification_failure_keeps_an_unselected_recoverable_candidate(self):
        self.policy.write_text('{"fail_final":true}')
        with self.assertRaisesRegex(RuntimeError, 'bootstrap refused'):
            self.run_stage(stage=True)
        self.assertTrue(self.destination.is_dir())
        self.assertFalse(self.receipt.exists())
        self.assert_protected()
        self.policy.write_text('{}')
        self.assertEqual(self.run_stage(stage=True)['status'], 'pending')
        self.assert_protected(state=False)

    def test_truncated_oversized_and_hash_tampered_archive_never_reach_trusted_verify(self):
        original = self.archive.read_bytes()
        for body in (original[:-1], original + b'extra', bytes([original[0] ^ 1]) + original[1:]):
            with self.subTest(size=len(body)):
                self.archive.write_bytes(body)
                with self.assertRaises((RuntimeError, ValueError)):
                    self.run_stage(stage=True)
                self.assertFalse(self.destination.exists())
                self.assertFalse(self.receipt.exists())
                self.assert_protected()
        self.assertTrue(all(e['args'] == ['--version'] for e in self.events()))

    def test_signed_archive_link_escape_extension_duplicate_mode_and_hash_attacks_fail(self):
        original = gzip.decompress(self.archive.read_bytes())
        for attack in ('symlink', 'hardlink', 'fifo', 'pax', 'parent', 'absolute', 'duplicate', 'mode', 'hash', 'trailer'):
            raw = bytearray(original)
            if attack in ('symlink', 'hardlink', 'fifo', 'pax'):
                raw[156] = {'symlink': ord('2'), 'hardlink': ord('1'), 'fifo': ord('6'), 'pax': ord('x')}[attack]
                raw[157:257] = b'/etc/passwd'.ljust(100, b'\0')
            elif attack in ('parent', 'absolute'):
                raw[:100] = (b'../escape' if attack == 'parent' else b'/absolute').ljust(100, b'\0')
            elif attack == 'duplicate':
                raw = raw[:1024] + raw
            elif attack == 'mode':
                raw[100:108] = b'0004777\0'
            elif attack == 'hash':
                raw[512] ^= 1
            else:
                raw += b'trailing data'
            raw[148:156] = b' ' * 8
            raw[148:156] = ('%06o\0 ' % sum(raw[:512])).encode()
            self.changed_archive(raw)  # even a signed malicious tar must fail
            with self.subTest(attack=attack), self.assertRaises((RuntimeError, ValueError, tarfile.TarError)):
                self.run_stage(stage=True)
            self.assertFalse(self.destination.exists())
            self.assertFalse((self.root / 'escape').exists())
            self.assertFalse(self.receipt.exists())
            self.assert_protected()

    def test_input_state_lock_and_bootstrap_symlinks_or_hardlinks_are_rejected(self):
        # These paths are never followed as mutable metadata or executable trust.
        lock = self.state / 'system-update.lock'
        lock.touch(mode=0o600)
        for path in (self.manifest, self.archive, self.path, lock, self.app / 'cartridge'):
            for kind in ('symlink', 'hardlink'):
                outside = self.root / ('outside-' + path.name)
                if kind == 'symlink':
                    path.rename(outside)
                    path.symlink_to(outside)
                else:
                    os.link(str(path), str(outside))
                try:
                    with self.subTest(path=path.name, kind=kind), self.assertRaises((RuntimeError, OSError)):
                        self.run_stage(stage=True)
                    self.assertFalse(self.destination.exists())
                    self.assertFalse(self.receipt.exists())
                finally:
                    if kind == 'symlink':
                        path.unlink()
                        outside.rename(path)
                    else:
                        outside.unlink()
                self.assert_protected()

    def test_symlinked_storage_roots_and_candidate_are_not_followed(self):
        self.run_stage()
        for path in (self.destination, self.app / 'releases', self.app, self.state):
            outside = self.root / ('saved-' + path.name)
            path.rename(outside)
            path.symlink_to(outside, target_is_directory=True)
            try:
                with self.subTest(path=str(path)), self.assertRaises(RuntimeError):
                    self.run_stage(stage=True)
            finally:
                path.unlink()
                outside.rename(path)
            self.assert_protected()
        self.assertFalse(self.receipt.exists())

    def test_existing_candidate_links_extras_or_hash_changes_cannot_be_staged(self):
        self.run_stage()
        path = self.destination / 'registry.json'
        original = path.read_bytes()
        for kind in ('symlink', 'hardlink', 'hash', 'extra'):
            outside = self.root / 'outside-candidate'
            if kind == 'symlink':
                path.rename(outside)
                path.symlink_to(outside)
            elif kind == 'hardlink':
                os.link(str(path), str(outside))
            elif kind == 'hash':
                path.write_bytes(b'forged')
            else:
                (self.destination / 'extra').write_text('unlisted')
            try:
                with self.subTest(kind=kind), self.assertRaises(RuntimeError):
                    self.run_stage(stage=True)
                self.assertFalse(self.receipt.exists())
            finally:
                if kind == 'symlink':
                    path.unlink(); outside.rename(path)
                elif kind == 'hardlink':
                    outside.unlink()
                elif kind == 'hash':
                    path.write_bytes(original)
                else:
                    (self.destination / 'extra').unlink()
            self.assert_protected()

    def test_malformed_duplicate_missing_or_oversized_state_fails_without_reset(self):
        values = [b'{}', b'{"schema":1,"schema":1}', b'x' * 16385]
        for change in ({'schema': True}, {'active': '../escape'}, {'unexpected': 1}):
            value = dict(self.initial); value.update(change); values.append(json.dumps(value).encode())
        for raw in values:
            self.path.write_bytes(raw)
            with self.subTest(raw=raw[:50]), self.assertRaises((RuntimeError, ValueError)):
                self.run_stage(stage=True)
            self.assertEqual(self.path.read_bytes(), raw)
        self.path.unlink()
        with self.assertRaises(FileNotFoundError):
            self.run_stage(stage=True)
        self.assertFalse(self.path.exists())
        self.assertFalse(self.destination.exists())
        self.assertFalse(self.events())
        self.path.write_bytes(self.original_state)
        self.assert_protected()

    def test_pending_trial_and_expected_active_conflicts_preserve_state(self):
        for changes in ({'pending': '0.9.0-333333333333'}, {'trial': NEW}, {'pending': NEW}):
            value = dict(self.initial, **changes)
            stager.session.atomic_json(self.path, value)
            raw = self.path.read_bytes()
            with self.subTest(changes=changes), self.assertRaises((RuntimeError, OSError)):
                self.run_stage(stage=True)
            self.assertEqual(self.path.read_bytes(), raw)
        self.path.write_bytes(self.original_state)
        with self.assertRaisesRegex(RuntimeError, 'active release changed'):
            self.run_stage(expected_active=PREVIOUS)
        self.assertFalse(self.events())
        self.assert_protected()

    def test_pending_retry_requires_its_exact_original_receipt_state_pair(self):
        self.run_stage(stage=True)
        pending = self.path.read_bytes()
        self.receipt.write_text(json.dumps(dict(self.initial, previous=None)))
        with self.assertRaisesRegex(RuntimeError, 'does not match'):
            self.run_stage(stage=True)
        self.assertEqual(self.path.read_bytes(), pending)
        self.assert_protected(state=False)

    def test_real_supervisor_lock_serializes_and_is_released_on_failure(self):
        path = self.state / 'system-update.lock'
        fd = os.open(str(path), os.O_CREAT | os.O_RDWR, 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(RuntimeError, 'holds the lock'):
                self.run_stage(stage=True)
        finally:
            os.close(fd)
        self.policy.write_text('{"fail_verify":true}')
        with self.assertRaises(RuntimeError):
            self.run_stage(stage=True)
        self.policy.write_text('{}')
        self.run_stage()
        self.assert_protected()

    def test_low_unknown_power_and_second_stage_power_check_preserve_boot_state(self):
        for power in ((29, False), (None, False)):
            self.power.return_value = power
            with self.subTest(power=power), self.assertRaisesRegex(RuntimeError, 'charger'):
                self.run_stage(stage=True)
            self.assertFalse(self.destination.exists())
            self.assert_protected()
        self.power.side_effect = [(80, False), (29, False)]
        with self.assertRaisesRegex(RuntimeError, 'charger'):
            self.run_stage(stage=True)
        self.assertTrue(self.destination.exists())
        self.assertFalse(self.receipt.exists())
        self.assert_protected()
        self.power.side_effect = None
        self.power.return_value = (None, True)
        self.assertEqual(self.run_stage(stage=True)['status'], 'pending')
        self.assert_protected(state=False)

    def test_vm_preparation_allows_absent_telemetry_but_stage_still_requires_power(self):
        self.power.return_value = (None, False)
        self.assertEqual(self.run_stage()['status'], 'prepared')
        self.assertFalse(self.receipt.exists())
        with self.assertRaisesRegex(RuntimeError, 'charger'):
            self.run_stage(stage=True)
        self.assert_protected()
        self.power.return_value = (29, False)
        with self.assertRaisesRegex(RuntimeError, 'charger'):
            self.run_stage()
        self.power.return_value = (30, False)
        self.assertEqual(self.run_stage(stage=True)['status'], 'pending')
        self.assert_protected(state=False)

    def test_low_release_and_state_space_fail_closed_even_with_reused_candidate(self):
        usage = shutil.disk_usage(str(self.root))
        required = self.payload['archive']['size'] + self.payload['archive']['unpacked_size'] + 32 * 1024 * 1024
        with mock.patch.object(stager.shutil, 'disk_usage', return_value=usage._replace(free=required - 1)):
            with self.assertRaisesRegex(RuntimeError, 'runtime staging'):
                self.run_stage(stage=True)
        self.assertFalse(self.destination.exists())
        self.assert_protected()
        self.run_stage()
        with mock.patch.object(stager.shutil, 'disk_usage', return_value=usage._replace(free=2 * 1024 * 1024 - 1)):
            with self.assertRaisesRegex(RuntimeError, 'update state'):
                self.run_stage(stage=True)
        self.assertFalse(self.receipt.exists())
        self.assert_protected()

    def test_old_bootstrap_nonzero_version_and_nonnewer_release_fail_closed(self):
        for policy in ({'version': 'cartridge 0.6.0'}, {'version': 'unrecognized'},
                       {'version': 'cartridge 0.8.0\nextra'}, {'fail_version': True}):
            self.policy.write_text(json.dumps(policy))
            with self.subTest(policy=policy), self.assertRaises((RuntimeError, ValueError)):
                self.run_stage(stage=True)
        self.policy.write_text('{}')
        value = dict(self.initial, active=NEW)
        stager.session.atomic_json(self.path, value)
        with self.assertRaisesRegex(RuntimeError, 'newer runtime'):
            self.run_stage(stage=True)
        self.assertEqual(json.loads(self.path.read_text()), value)
        self.path.write_bytes(self.original_state)
        self.assertFalse(self.destination.exists())
        self.assert_protected()

    def test_unsigned_wrong_envelope_key_or_signature_shape_fail_before_verifier(self):
        envelope = json.loads(self.manifest.read_text())
        for changed in (self.payload, dict(envelope, key_id='other'), dict(envelope, signature='ff'),
                        dict(envelope, unexpected=True)):
            self.manifest.write_text(json.dumps(changed))
            with self.assertRaises((RuntimeError, ValueError)):
                self.run_stage(stage=True)
        self.assertFalse(self.events())
        self.assertFalse(self.destination.exists())
        self.assert_protected()

    def test_changed_state_during_verification_is_not_overwritten(self):
        foreign = dict(self.initial, active=PREVIOUS, previous=None, last_result='another writer')
        self.policy.write_text(json.dumps({'mutate_state': foreign}))
        with self.assertRaisesRegex(RuntimeError, 'State changed'):
            self.run_stage(stage=True)
        self.assertEqual(json.loads(self.path.read_text()), foreign)
        self.assertFalse(self.receipt.exists())
        self.assert_protected(state=False)

    def test_stale_invalid_or_linked_backup_receipt_is_never_overwritten(self):
        self.run_stage()
        for raw in (b'{}', b'{"schema":1,"schema":1}', b'x' * 16385,
                    json.dumps(dict(self.initial, active=PREVIOUS)).encode()):
            self.receipt.write_bytes(raw)
            with self.assertRaises((RuntimeError, ValueError)):
                self.run_stage(stage=True)
            self.assertEqual(self.receipt.read_bytes(), raw)
            self.assert_protected()
        self.receipt.unlink()
        outside = self.root / 'receipt-outside'
        outside.write_text(json.dumps(self.initial))
        for kind in ('symlink', 'hardlink', 'dangling'):
            if kind in ('symlink', 'dangling'):
                self.receipt.symlink_to(outside if kind == 'symlink' else self.root / 'missing')
            else:
                os.link(str(outside), str(self.receipt))
            with self.subTest(kind=kind), self.assertRaises(RuntimeError):
                self.run_stage(stage=True)
            self.receipt.unlink()
            self.assert_protected()

    def test_receipt_and_pending_atomic_replace_interruptions_are_recoverable(self):
        self.run_stage()
        replace, fsync = os.replace, os.fsync
        for phase in ('receipt-before', 'receipt-after', 'state-before', 'state-after'):
            self.path.write_bytes(self.original_state)
            if self.receipt.exists():
                self.receipt.unlink()
            renamed = []
            def interrupt(source, target):
                target = Path(target)
                receipt_write = target == self.receipt
                state_write = target == self.path
                if state_write:
                    # A validated, synced full receipt must precede pending.
                    self.assertEqual(json.loads(self.receipt.read_text()), self.initial)
                selected = (receipt_write and phase.startswith('receipt')) or (state_write and phase.startswith('state'))
                if selected and phase.endswith('before'):
                    raise OSError('injected interruption before rename')
                replace(source, target)
                if selected:
                    renamed.append(True)
            def interrupt_sync(fd):
                if renamed and stat.S_ISDIR(os.fstat(fd).st_mode):
                    raise OSError('injected interruption after rename before directory fsync')
                return fsync(fd)
            with self.subTest(phase=phase), mock.patch.object(stager.session.os, 'replace', side_effect=interrupt), \
                    mock.patch.object(stager.session.os, 'fsync', side_effect=interrupt_sync):
                with self.assertRaisesRegex(OSError, 'injected interruption'):
                    self.run_stage(stage=True)
            if phase == 'state-after':
                self.assertEqual(json.loads(self.path.read_text()), stager.pending_state(self.initial, NEW))
            else:
                self.assertEqual(self.path.read_bytes(), self.original_state)
            if phase != 'receipt-before':
                self.assertEqual(json.loads(self.receipt.read_text()), self.initial)
            self.assertEqual({p.name for p in self.state.iterdir()},
                             {'system-update.json', 'system-update.lock'} | ({self.receipt.name} if self.receipt.exists() else set()))
            self.run_stage(stage=True)
            self.assertEqual(json.loads(self.receipt.read_text()), self.initial)
            self.assert_protected(state=False)

    def test_receipt_fsync_failure_cannot_write_pending(self):
        self.run_stage()
        fsync = os.fsync
        def fail_receipt(fd):
            # atomic_json's first fsync is the complete receipt temporary file.
            if stat.S_ISREG(os.fstat(fd).st_mode):
                raise OSError('injected receipt fsync failure')
            return fsync(fd)
        with mock.patch.object(stager.session.os, 'fsync', side_effect=fail_receipt):
            with self.assertRaisesRegex(OSError, 'receipt fsync'):
                self.run_stage(stage=True)
        self.assertFalse(self.receipt.exists())
        self.assert_protected()
        self.run_stage(stage=True)
        self.assert_protected(state=False)

    def test_cli_has_no_platform_or_power_override_and_source_parses_as_python37(self):
        if sys.version_info >= (3, 8):
            ast.parse(SCRIPT.read_text(), feature_version=7)
        command = [sys.executable, str(SCRIPT), '--root', str(self.app), '--state', str(self.state),
                   '--manifest', str(self.manifest), '--archive', str(self.archive)]
        for bypass in ('--desktop', '--skip-power', '--skip-verify', '--force'):
            result = subprocess.run(command + [bypass], capture_output=True, universal_newlines=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('unrecognized arguments', result.stderr)
        if sys.platform != 'linux' or os.uname().machine not in ('arm64', 'aarch64') or os.geteuid() == 0:
            result = subprocess.run(command, capture_output=True, universal_newlines=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Local update stopped:', result.stderr)
        self.assertFalse(self.destination.exists())
        self.assert_protected()


if __name__ == '__main__':
    unittest.main()
