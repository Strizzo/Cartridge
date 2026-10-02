"""The recovery readback compares every byte without altering the card."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest import mock


SOURCE = Path(__file__).resolve().parents[1] / 'sim/device/verify-firstboot-trial.py'
spec = importlib.util.spec_from_file_location('verify_firstboot_trial', SOURCE)
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)


class FirstbootReadbackTest(unittest.TestCase):
    def test_complete_match_and_region_hashes(self):
        payload = b'abcdefgh' + b'1234567890abcdef' + b'rom-data' * 5
        with tempfile.TemporaryDirectory() as folder:
            source, card = Path(folder) / 'source', Path(folder) / 'card'
            source.write_bytes(payload)
            card.write_bytes(payload)
            with mock.patch.object(verify.flash, 'ROOT_OFFSET', 8):
                result = verify.compare(source, card, len(payload), 16)
            self.assertEqual(result['bytes_compared'], len(payload))
            self.assertEqual(result['mismatch_chunks'], 0)
            self.assertIsNone(result['first_mismatch_byte'])
            self.assertEqual(result['source_full_sha256'], result['card_full_sha256'])
            self.assertEqual(result['source_boot_sha256'], result['card_boot_sha256'])
            self.assertEqual(result['source_root_sha256'], result['card_root_sha256'])
            self.assertEqual(card.read_bytes(), payload)

    def test_mismatch_location_and_short_read(self):
        payload = b'abcdefgh' + b'1234567890abcdef' + b'rom-data'
        changed = payload[:11] + b'X' + payload[12:]
        with tempfile.TemporaryDirectory() as folder:
            source, card = Path(folder) / 'source', Path(folder) / 'card'
            source.write_bytes(payload)
            card.write_bytes(changed)
            with mock.patch.object(verify.flash, 'ROOT_OFFSET', 8):
                result = verify.compare(source, card, len(payload), 16)
                self.assertEqual(result['first_mismatch_byte'], 11)
                self.assertNotEqual(result['source_root_sha256'], result['card_root_sha256'])
                card.write_bytes(changed[:-1])
                with self.assertRaisesRegex(RuntimeError, 'Short read'):
                    verify.compare(source, card, len(payload), 16)


if __name__ == '__main__':
    unittest.main()
