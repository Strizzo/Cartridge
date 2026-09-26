"""Offline conversion must boot Cartridge while retaining a populated game card."""
import importlib.util
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest import mock

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO/'installer'))
import install_transaction  # noqa: E402
spec = importlib.util.spec_from_file_location('cartridge_offline_prepare', REPO/'installer/offline_prepare.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class OfflinePrepareTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='cartridge-offline-test-')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base/'mounted-root'
        self.roms = self.root/'roms'
        self.bundle = self.base/'bundle/Cartridge'
        self.backup = self.base/'backup'
        service = self.root/'etc/systemd/system/emulationstation.service'
        service.parent.mkdir(parents=True)
        self.stock = b'[Service]\nUser=ark\nExecStart=/usr/bin/emulationstation/emulationstation.sh\n'
        service.write_bytes(self.stock)
        wants = service.parent/'multi-user.target.wants'
        wants.mkdir()
        (wants/'emulationstation.service').symlink_to('../emulationstation.service')
        es = self.root/'usr/bin/emulationstation/emulationstation.sh'
        es.parent.mkdir(parents=True)
        es.write_bytes(b'original ES binary/script')
        app = self.roms/'Cartridge'
        (app/'assets/fonts').mkdir(parents=True)
        (app/'cartridge').write_bytes(b'old Cartridge executable')
        (app/'assets/fonts/old.ttf').write_bytes(b'old font')
        (app/'ssh').mkdir()
        (app/'ssh/id_ed25519').write_bytes(b'user key stays on card')
        (self.roms/'tools').mkdir()
        (self.roms/'psx').mkdir()
        (self.roms/'psx/game.chd').write_bytes(b'unchanged ROM')
        (self.roms/'psx/game.srm').write_bytes(b'unchanged save')
        (self.roms/'psx/gamelist.xml').write_bytes(b'unchanged metadata')
        (self.bundle/'dev').mkdir(parents=True)
        (self.bundle/'dev/build-revision').write_text('test-ci-revision')
        for name in ('cartridge', 'autosetup.sh', 'game-library.py', 'registry.json', 'assets/boot_logo.png'):
            target = self.bundle/name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(('new '+name).encode())
        shutil.copyfile(REPO/'deploy/setup-primary.py', self.bundle/'setup-primary.py')
        shutil.copyfile(REPO/'deploy/cartridge-session.py', self.bundle/'cartridge-session.py')
        for name in ('assets/fonts/new.ttf', 'assets/overlays/vignette.png',
                     'lua_cartridges/weather/main.lua', 'tools/Cartridge.sh',
                     'tools/Setup Cartridge Boot.sh', 'tools/Undo Cartridge Boot.sh'):
            target = self.bundle/name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(('new '+name).encode())

    def test_populated_card_prepares_direct_boot_without_touching_games(self):
        result = module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        self.assertEqual(result['state'], 'prepared_and_verified')
        self.assertTrue(result['cartridge_default_on_next_boot'])
        dropin = self.root/'etc/systemd/system/emulationstation.service.d/99-cartridge-primary.conf'
        self.assertIn('ExecStart=/usr/bin/python3 /usr/local/lib/cartridge/cartridge-session.py', dropin.read_text())
        self.assertEqual((self.root/'etc/systemd/system/emulationstation.service').read_bytes(), self.stock)
        self.assertTrue((self.root/'etc/systemd/system/multi-user.target.wants/emulationstation.service').is_symlink())
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'new cartridge')
        self.assertEqual((self.roms/'psx/game.chd').read_bytes(), b'unchanged ROM')
        self.assertEqual((self.roms/'psx/game.srm').read_bytes(), b'unchanged save')
        self.assertEqual((self.roms/'psx/gamelist.xml').read_bytes(), b'unchanged metadata')
        self.assertEqual((self.roms/'Cartridge/ssh/id_ed25519').read_bytes(), b'user key stays on card')
        self.assertEqual((self.backup/'previous/roms/Cartridge/cartridge').read_bytes(), b'old Cartridge executable')
        self.assertEqual(json.loads((self.backup/'manifest.json').read_text())['state'], 'prepared_and_verified')

    def test_split_preparation_configures_clone_before_staging_roms(self):
        root = module.prepare_root(self.root, self.bundle, self.backup, require_mount=False)
        self.assertEqual(root['state'], 'root_prepared_and_verified')
        self.assertFalse(root.get('cartridge_default_on_next_boot', False))
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')
        self.assertEqual((self.roms/'psx/game.chd').read_bytes(), b'unchanged ROM')
        self.assertEqual((self.root/'etc/systemd/system/emulationstation.service').read_bytes(), self.stock)
        result = module.stage_roms(self.roms, self.bundle, self.backup, require_mount=False)
        self.assertEqual(result['state'], 'prepared_and_verified')
        self.assertTrue(result['cartridge_default_on_next_boot'])
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'new cartridge')
        self.assertEqual((self.roms/'psx/game.srm').read_bytes(), b'unchanged save')
        self.assertEqual((self.roms/'Cartridge/ssh/id_ed25519').read_bytes(), b'user key stays on card')
        rollback = module.restore_roms(self.roms, self.backup, require_mount=False)
        self.assertEqual(rollback['state'], 'roms_rollback_verified')
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')

    def test_split_preparation_rejects_different_bundle_before_roms_writes(self):
        module.prepare_root(self.root, self.bundle, self.backup, require_mount=False)
        (self.bundle/'cartridge').write_bytes(b'different device build')
        with self.assertRaisesRegex(RuntimeError, 'Bundle differs'):
            module.stage_roms(self.roms, self.bundle, self.backup, require_mount=False)
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')
        self.assertEqual(json.loads((self.backup/'manifest.json').read_text())['state'],
                         'root_prepared_and_verified')

    def test_split_preparation_root_failure_leaves_roms_unchanged(self):
        (self.bundle/'setup-primary.py').write_text('raise RuntimeError("simulated setup failure")\n')
        with self.assertRaisesRegex(RuntimeError, 'simulated setup failure'):
            module.prepare_root(self.root, self.bundle, self.backup, require_mount=False)
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')
        self.assertEqual(json.loads((self.backup/'manifest.json').read_text())['state'],
                         'root_rolled_back_after_failure')

    def test_split_roms_copy_failure_restores_cartridge_files(self):
        module.prepare_root(self.root, self.bundle, self.backup, require_mount=False)
        original_copy = module.atomic_copy
        def fail_second_install(source, destination, **kwargs):
            if destination.is_relative_to(self.roms.resolve()) and destination.name == 'autosetup.sh':
                raise OSError('simulated ROMS write failure')
            return original_copy(source, destination, **kwargs)
        with mock.patch.object(module, 'atomic_copy', side_effect=fail_second_install):
            with self.assertRaisesRegex(OSError, 'simulated ROMS write failure'):
                module.stage_roms(self.roms, self.bundle, self.backup, require_mount=False)
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')
        self.assertEqual((self.roms/'psx/game.chd').read_bytes(), b'unchanged ROM')
        self.assertEqual(json.loads((self.backup/'manifest.json').read_text())['state'],
                         'roms_rollback_verified')

    def test_refuses_unmounted_paths_and_unsupported_boot(self):
        with self.assertRaisesRegex(RuntimeError, 'offline mounts'):
            module.prepare(self.root, self.roms, self.bundle, self.backup)
        (self.root/'etc/systemd/system/emulationstation.service').write_text('unsupported boot service')
        with self.assertRaisesRegex(RuntimeError, 'Unsupported stock service'):
            module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        self.assertFalse(self.backup.exists())
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')

    def test_refuses_symlink_parent_before_writing(self):
        fonts = self.roms/'Cartridge/assets/fonts'
        shutil.rmtree(fonts)
        fonts.symlink_to(self.base)
        with self.assertRaisesRegex(RuntimeError, 'symlink parent'):
            module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        self.assertFalse(self.backup.exists())
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')

    def test_known_recovery_logging_override_is_allowed(self):
        overrides = self.root/'etc/systemd/system/emulationstation.service.d'
        overrides.mkdir()
        (overrides/'90-cartridge-recovery.conf').write_text(
            '# Cartridge recovery\n[Service]\nStandardInput=null\n'
            'StandardOutput=append:/var/log/cartridge-recovery-es.log\n'
            'StandardError=inherit\nRestartSec=3\n')
        result = module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        self.assertEqual(result['state'], 'prepared_and_verified')

    def test_conflicting_service_override_stops_before_writes(self):
        overrides = self.root/'etc/systemd/system/emulationstation.service.d'
        overrides.mkdir()
        (overrides/'95-other-boot.conf').write_text('[Service]\nExecStart=/other/launcher\n')
        with self.assertRaisesRegex(RuntimeError, 'Unsupported service override'):
            module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        self.assertFalse(self.backup.exists())
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')

    def test_recovery_override_cannot_change_startup(self):
        overrides = self.root/'etc/systemd/system/emulationstation.service.d'
        overrides.mkdir()
        (overrides/'90-cartridge-recovery.conf').write_text('[Service]\nExecStart=/other/launcher\n')
        with self.assertRaisesRegex(RuntimeError, 'Recovery override changes unsupported'):
            module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        self.assertFalse(self.backup.exists())

    def test_commented_stock_service_claims_are_rejected(self):
        (self.root/'etc/systemd/system/emulationstation.service').write_text(
            '[Service]\n# User=ark\n# ExecStart=/usr/bin/emulationstation/emulationstation.sh\n')
        with self.assertRaisesRegex(RuntimeError, 'Unsupported stock service'):
            module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        self.assertFalse(self.backup.exists())

    def test_wrong_es_enable_link_stops_before_writes(self):
        enabled = self.root/'etc/systemd/system/multi-user.target.wants/emulationstation.service'
        enabled.unlink()
        enabled.symlink_to('/dev/null')
        with self.assertRaisesRegex(RuntimeError, 'Stock ES must be enabled'):
            module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        self.assertFalse(self.backup.exists())

    def test_failure_after_copy_restores_original_payload(self):
        (self.bundle/'setup-primary.py').write_text('raise RuntimeError("simulated setup failure")\n')
        with self.assertRaisesRegex(RuntimeError, 'simulated setup failure'):
            module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')
        self.assertFalse((self.roms/'Cartridge/assets/fonts/new.ttf').exists())
        self.assertFalse((self.root/'etc/systemd/system/emulationstation.service.d/99-cartridge-primary.conf').exists())
        self.assertEqual((self.roms/'psx/game.chd').read_bytes(), b'unchanged ROM')
        self.assertEqual(json.loads((self.backup/'manifest.json').read_text())['state'], 'rolled_back_after_failure')

    def test_later_roms_rollback_restores_payload_without_touching_games_or_key(self):
        module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        result = module.restore_roms(self.roms, self.backup, require_mount=False)
        self.assertEqual(result['state'], 'roms_rollback_verified')
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')
        self.assertFalse((self.roms/'Cartridge/assets/fonts/new.ttf').exists())
        self.assertEqual((self.roms/'psx/game.chd').read_bytes(), b'unchanged ROM')
        self.assertEqual((self.roms/'psx/game.srm').read_bytes(), b'unchanged save')
        self.assertEqual((self.roms/'Cartridge/ssh/id_ed25519').read_bytes(), b'user key stays on card')

    def test_later_rollback_refuses_unrelated_change_before_restoring_any_file(self):
        module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        (self.roms/'Cartridge/cartridge').write_bytes(b'changed outside installer')
        with self.assertRaisesRegex(RuntimeError, 'changed outside this install'):
            module.restore_roms(self.roms, self.backup, require_mount=False)
        self.assertTrue((self.roms/'Cartridge/assets/fonts/new.ttf').exists())
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'changed outside installer')

    def test_later_rollback_rejects_manifest_path_to_games(self):
        module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        manifest_path = self.backup/'manifest.json'
        manifest = json.loads(manifest_path.read_text())
        manifest['files'].append({'path': 'roms/psx/game.chd', 'previous_sha256': None,
                                  'installed_sha256': '0'*64})
        manifest_path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(RuntimeError, 'unexpected ROMS path'):
            module.restore_roms(self.roms, self.backup, require_mount=False)
        self.assertEqual((self.roms/'psx/game.chd').read_bytes(), b'unchanged ROM')

    def transaction_fixture(self):
        module.prepare(self.root, self.roms, self.bundle, self.backup, require_mount=False)
        old = b'A' * (5 * 1024 * 1024)
        new = b'B' * len(old)
        target = self.base/'disposable-s2.ext4'
        target.write_bytes(old)
        prepared = self.base/'prepared-s2.ext4'
        prepared.write_bytes(new)
        clone = self.base/'original-clone'
        clone.mkdir()
        (clone/'root.ext4').write_bytes(old)
        digest = lambda data: hashlib.sha256(data).hexdigest()
        fingerprint = 'selected-test-card'
        (clone/'manifest.json').write_text(json.dumps({
            'state': 'verified', 'selected_disk': 'disk6',
            'inventory_fingerprint': fingerprint, 'source_partition': 'disk6s2',
            'source_bytes': len(old), 'sha256': digest(old), 'source_read_passes': 2,
            'destination_readback_verified': True, 'ext4_check': 'clean',
        }))
        record = {
            'identifier': 'disk6', 'inventory_fingerprint': fingerprint,
            'status': 'preserve_candidate',
            'partitions': [
                {'identifier': 'disk6s1', 'content': 'DOS_FAT_32',
                 'size_bytes': 100, 'device_size_bytes': 100, 'mounted': False, 'mount_point': ''},
                {'identifier': 'disk6s2', 'content': 'Linux',
                 'size_bytes': len(old), 'device_size_bytes': len(old),
                 'mounted': False, 'mount_point': ''},
                {'identifier': 'disk6s3', 'content': 'Windows_NTFS',
                 'size_bytes': 100, 'device_size_bytes': 100,
                 'mounted': True, 'mount_point': str(self.roms)},
            ],
        }
        journal = self.base/'writeback-journal'
        def finish(**extra):
            options = dict(inventory_fn=lambda: [record], target_override=target,
                           filesystem_check=lambda path: None, require_mount=False,
                           root_validator=lambda path, preparation: None)
            options.update(extra)
            return install_transaction.finish_install('disk6', fingerprint, clone, prepared,
                                                      digest(new), self.backup, self.roms,
                                                      journal, **options)
        return old, new, target, journal, finish

    def test_transaction_success_keeps_games_and_commits_both_parts(self):
        old, new, target, journal, finish = self.transaction_fixture()
        result = finish()
        self.assertEqual(result['state'], 'installed_and_verified')
        self.assertEqual(target.read_bytes(), new)
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'new cartridge')
        self.assertEqual((self.roms/'psx/game.chd').read_bytes(), b'unchanged ROM')
        self.assertEqual((self.roms/'psx/game.srm').read_bytes(), b'unchanged save')
        self.assertEqual(json.loads((journal/'manifest.json').read_text())['state'], 'verified')

    def test_transaction_write_failure_rolls_back_root_and_roms(self):
        old, new, target, journal, finish = self.transaction_fixture()
        def fail_after_chunk(written):
            raise OSError('injected card write error')
        with self.assertRaisesRegex(install_transaction.InstallError, 'were restored'):
            finish(chunk_hook=fail_after_chunk)
        self.assertEqual(target.read_bytes(), old)
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')
        self.assertEqual((self.roms/'psx/game.chd').read_bytes(), b'unchanged ROM')
        self.assertEqual(json.loads((self.backup/'manifest.json').read_text())['state'],
                         'roms_rollback_verified')

    def test_transaction_changed_root_restores_roms_without_writing(self):
        old, new, target, journal, finish = self.transaction_fixture()
        target.write_bytes(b'C' * len(old))
        with self.assertRaisesRegex(install_transaction.InstallError, 'were restored'):
            finish()
        self.assertEqual(target.read_bytes(), b'C' * len(old))
        self.assertFalse(journal.exists())
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')

    def test_transaction_unverified_root_restores_roms_before_write(self):
        old, new, target, journal, finish = self.transaction_fixture()
        def reject_root(path, preparation):
            raise install_transaction.InstallError('prepared root has no startup override')
        with self.assertRaisesRegex(install_transaction.InstallError, 'were restored'):
            finish(root_validator=reject_root)
        self.assertEqual(target.read_bytes(), old)
        self.assertFalse(journal.exists())
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'old Cartridge executable')

    def test_transaction_missing_roms_backup_blocks_root_write(self):
        old, new, target, journal, finish = self.transaction_fixture()
        (self.backup/'previous/roms/Cartridge/cartridge').unlink()
        with self.assertRaisesRegex(install_transaction.InstallError, 'backup is missing'):
            finish()
        self.assertEqual(target.read_bytes(), old)
        self.assertFalse(journal.exists())

    def test_transaction_incomplete_root_keeps_roms_payload_for_recovery(self):
        old, new, target, journal, finish = self.transaction_fixture()
        def broken_writeback(*args, **kwargs):
            journal.mkdir()
            (journal/'manifest.json').write_text(json.dumps({'state': 'rollback_incomplete'}))
            target.write_bytes(new[:4 * 1024 * 1024] + old[4 * 1024 * 1024:])
            raise OSError('injected incomplete rollback')
        with mock.patch.object(install_transaction, 'writeback', side_effect=broken_writeback):
            with self.assertRaisesRegex(install_transaction.InstallError, 'remain on ROMS'):
                finish()
        self.assertEqual((self.roms/'Cartridge/cartridge').read_bytes(), b'new cartridge')
        self.assertEqual((self.roms/'psx/game.srm').read_bytes(), b'unchanged save')


if __name__ == '__main__':
    unittest.main()
