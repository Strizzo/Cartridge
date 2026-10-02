"""Reject malformed splashes before replacing BOOT contents in a virtual image."""

import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('build_firstboot_trial', REPO / 'sim/device/build-firstboot-trial.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class BootLogoTest(unittest.TestCase):
    def test_shipped_logo_has_bootloader_compatible_format(self):
        self.assertEqual(builder.checked_boot_logo(REPO / 'assets/logo.bmp'),
                         (REPO / 'assets/logo.bmp').resolve())

    def test_wrong_dimensions_and_compression_are_rejected(self):
        header = bytearray((REPO / 'assets/logo.bmp').read_bytes()[:54])
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / 'logo.bmp'
            for offset, fmt, value in ((18, '<i', 640), (30, '<I', 1)):
                candidate = bytearray(header)
                struct.pack_into(fmt, candidate, offset, value)
                struct.pack_into('<I', candidate, 2, len(candidate))
                file.write_bytes(candidate)
                with self.subTest(offset=offset), self.assertRaisesRegex(RuntimeError, '720x720'):
                    builder.checked_boot_logo(file)


if __name__ == '__main__':
    unittest.main()
