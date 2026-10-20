"""The Mac handoff must keep the immutable clone and personal files intact."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO/'installer'))
from host_prepare import HostPrepareError, prepare_clone, stage_selected_roms  # noqa: E402
from offline_prepare import bundle_digest, sha256  # noqa: E402


class HostPrepareTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='cartridge-host-prep-')
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.roms = self.base/'card-roms'
        (self.roms/'Cartridge/ssh').mkdir(parents=True)
        (self.roms/'Cartridge/cartridge').write_bytes(b'previous executable')
        (self.roms/'Cartridge/ssh/id_ed25519').write_bytes(b'user key')
        (self.roms/'tools').mkdir()
        (self.roms/'psx').mkdir()
        (self.roms/'psx/game.chd').write_bytes(b'game bytes')
        (self.roms/'psx/game.srm').write_bytes(b'save bytes')
        self.bundle = self.base/'bundle/Cartridge'
        (self.bundle/'dev').mkdir(parents=True)
        (self.bundle/'dev/build-revision').write_text('test-ci-build')
        for name in ('cartridge', 'autosetup.sh', 'cartridge-session.py',
                     'setup-primary.py', 'game-library.py', 'registry.json',
                     'assets/boot_logo.png', 'assets/fonts/font.ttf',
                     'assets/overlays/layer.png', 'lua_cartridges/weather/main.lua',
                     'tools/Cartridge.sh', 'tools/Setup Cartridge Boot.sh',
                     'tools/Undo Cartridge Boot.sh'):
            target = self.bundle/name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(('new '+name).encode())
        self.original = self.base/'original'
        self.original.mkdir()
        original_image = self.original/'root.ext4'
        original_image.write_bytes(b'A'*(1024*1024))
        digest = sha256(original_image)
        self.fingerprint = 'selected-disposable-card'
        (self.original/'manifest.json').write_text(json.dumps({
            'state': 'verified', 'selected_disk': 'disk6',
            'inventory_fingerprint': self.fingerprint, 'source_partition': 'disk6s2',
            'source_bytes': original_image.stat().st_size, 'sha256': digest,
            'source_read_passes': 2, 'destination_readback_verified': True,
            'ext4_check': 'clean'}))
        self.record = {'identifier': 'disk6', 'inventory_fingerprint': self.fingerprint,
                       'status': 'preserve_candidate', 'partitions': [
                           {'identifier': 'disk6s1', 'mounted': False, 'mount_point': ''},
                           {'identifier': 'disk6s2', 'content': 'Linux', 'mounted': False,
                            'mount_point': '', 'size_bytes': original_image.stat().st_size,
                            'device_size_bytes': original_image.stat().st_size},
                           {'identifier': 'disk6s3', 'mounted': True,
                            'mount_point': str(self.roms)}]}
        self.work = self.base/'work'

    def fake_vm(self, work, bundle, app_path):
        image = work/'prepared-root.ext4'
        with image.open('r+b') as stream:
            stream.write(b'prepared startup')
        (work/'preparation-backup').mkdir()
        (work/'preparation-backup/manifest.json').write_text(json.dumps({
            'state': 'root_prepared_and_verified', 'app_path': app_path,
            'build_revision': 'test-ci-build',
            'cartridge_sha256': sha256(bundle/'cartridge'),
            'session_sha256': sha256(bundle/'cartridge-session.py'),
            'bundle_sha256': bundle_digest(bundle), 'files': [],
            'games_and_saves_written': False}))
        (work/'guest-report.json').write_text(json.dumps({
            'state': 'root_prepared_and_verified', 'app_path': app_path,
            'image_sha256': sha256(image), 'image_bytes': image.stat().st_size}))

    def prepare(self, **overrides):
        kwargs = dict(inventory_fn=lambda: [self.record], vm_runner=self.fake_vm,
                      filesystem_check=lambda _: None, root_validator=lambda *_: None)
        kwargs.update(overrides)
        return prepare_clone('disk6', self.fingerprint, self.original,
                             self.work, self.bundle, **kwargs)

    def stage(self, **overrides):
        kwargs = dict(inventory_fn=lambda: [self.record], require_mount=False,
                      filesystem_check=lambda _: None, root_validator=lambda *_: None)
        kwargs.update(overrides)
        return stage_selected_roms('disk6', self.fingerprint, self.work,
                                   self.roms, self.bundle, **kwargs)

    def test_host_handoff_and_selected_roms_stage(self):
        original_hash = sha256(self.original/'root.ext4')
        report = self.prepare()
        self.assertEqual(report['state'], 'root_prepared_and_verified')
        self.assertEqual(sha256(self.original/'root.ext4'), original_hash)
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'previous executable')
        result = self.stage()
        self.assertEqual(result['state'], 'prepared_and_verified')
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'new cartridge')
        self.assertEqual((self.roms/'psx/game.chd').read_bytes(), b'game bytes')
        self.assertEqual((self.roms/'psx/game.srm').read_bytes(), b'save bytes')
        self.assertEqual((self.roms/'Cartridge/ssh/id_ed25519').read_bytes(), b'user key')

    def test_workspace_on_selected_card_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, 'outside the selected card'):
            prepare_clone('disk6', self.fingerprint, self.original,
                          self.roms/'work', self.bundle,
                          inventory_fn=lambda: [self.record], vm_runner=self.fake_vm)
        self.assertFalse((self.roms/'work').exists())

    def test_vm_disagreement_stops_before_card_staging(self):
        def wrong_vm(work, bundle, app_path):
            self.fake_vm(work, bundle, app_path)
            (work/'guest-report.json').write_text(json.dumps({
                'state': 'root_prepared_and_verified', 'app_path': app_path,
                'image_sha256': '0'*64, 'image_bytes': 1024*1024}))
        with self.assertRaisesRegex(HostPrepareError, 'VM preparation report disagrees'):
            self.prepare(vm_runner=wrong_vm)
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'previous executable')
        self.assertEqual(json.loads((self.work/'host-report.json').read_text())['state'], 'incomplete')

    def test_changed_bundle_blocks_roms_stage(self):
        self.prepare()
        (self.bundle/'assets/fonts/font.ttf').write_bytes(b'different release font')
        with self.assertRaisesRegex(HostPrepareError, 'no longer matches'):
            self.stage()
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'previous executable')


if __name__ == '__main__':
    unittest.main()
