"""Offline conversion must boot Cartridge while retaining a populated game card."""
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[1]
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


if __name__ == '__main__':
    unittest.main()
