"""A failed spare-card file update must restore files from the verified backup."""

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock


REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / 'installer'))
spec = importlib.util.spec_from_file_location('update_spare_card', REPO / 'installer/update_spare_card.py')
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)


class SpareUpdateRollbackTest(unittest.TestCase):
    def test_second_copy_failure_restores_first_file(self):
        with tempfile.TemporaryDirectory() as root:
            base = Path(root)
            backup, bundle, boot, roms = [base / name for name in ('backup', 'bundle', 'boot', 'roms')]
            for root_path in (backup, bundle, boot, roms):
                root_path.mkdir()
            manifest = {'disk': 'disk9', 'logo_sha256': '', 'files': {}}
            for _, old, source in updater.CHANGES:
                original = (boot if old == 'logo.bmp' else roms) / old
                saved = backup / old
                new = bundle / source
                for path in (original, saved, new):
                    path.parent.mkdir(parents=True, exist_ok=True)
                original.write_bytes(('old-' + old).encode())
                saved.write_bytes(original.read_bytes())
                new.write_bytes(('new-' + old).encode())
                if old == 'logo.bmp':
                    manifest['logo_sha256'] = updater.sha256(saved)
                else:
                    manifest['files'][old.removeprefix('Cartridge/')] = updater.sha256(saved)
            args = SimpleNamespace(backup=backup, bundle=bundle)
            prior = {'state': 'preflight_passed', 'disk': 'disk9',
                     'sources': {name: {'sha256': updater.sha256(bundle / source)}
                                 for name, _, source in updater.CHANGES}}
            (backup / 'update-preflight.json').write_text(json.dumps(prior))
            real_copy = updater.atomic_copy

            def interrupted_copy(source, destination, **kwargs):
                if source.resolve() == (bundle / 'Cartridge/assets/boot_logo.png').resolve():
                    raise RuntimeError('simulated card write interruption')
                return real_copy(source, destination, **kwargs)

            with (mock.patch.object(updater, 'preflight', return_value=prior),
                  mock.patch.object(updater, 'read_backup', return_value=manifest),
                  mock.patch.object(updater, 'checked_card', return_value=(None, boot, roms)),
                  mock.patch.object(updater, 'atomic_copy', side_effect=interrupted_copy)):
                with self.assertRaisesRegex(RuntimeError, 'simulated card write interruption'):
                    updater.apply(args)

            self.assertEqual((roms / 'Cartridge/cartridge').read_bytes(),
                             (backup / 'Cartridge/cartridge').read_bytes())
            report = json.loads((backup / 'update-result.json').read_text())
            self.assertEqual(report['state'], 'rollback_verified')
            self.assertEqual(report['rollback_failures'], [])


if __name__ == '__main__':
    unittest.main()
