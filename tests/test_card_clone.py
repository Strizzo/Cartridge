"""Read-only card cloning must detect changed media and protect backup placement."""

import json
from pathlib import Path
import sys
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO/'installer'))
import card_clone  # noqa: E402


class CardCloneTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='cartridge-clone-test-')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.source = self.base/'source.ext4'
        self.source.write_bytes(b'filesystem fixture\n' * 4096)
        self.destination = self.base/'backup'
        self.card_mount = self.base/'card-games'
        self.card_mount.mkdir()
        self.fingerprint = 'fingerprint-for-selected-card'
        self.record = {
            'identifier': 'disk6', 'inventory_fingerprint': self.fingerprint,
            'status': 'preserve_candidate',
            'partitions': [
                {'identifier': 'disk6s1', 'content': 'DOS_FAT_32', 'size_bytes': 100,
                 'mounted': True, 'mount_point': str(self.base/'card-boot')},
                {'identifier': 'disk6s2', 'content': 'Linux', 'size_bytes': self.source.stat().st_size,
                 'mounted': False, 'mount_point': ''},
                {'identifier': 'disk6s3', 'content': 'Windows_NTFS', 'size_bytes': 100,
                 'mounted': True, 'mount_point': str(self.card_mount)},
            ],
        }

    def test_two_source_reads_and_output_readback(self):
        result = card_clone.clone_root('disk6', self.fingerprint, self.destination,
                                      inventory_fn=lambda: [self.record],
                                      source_override=self.source,
                                      filesystem_check=lambda path: self.assertTrue(path.is_file()))
        self.assertEqual(result['state'], 'verified')
        self.assertEqual(result['source_read_passes'], 2)
        self.assertEqual((self.destination/'root.ext4').read_bytes(), self.source.read_bytes())
        self.assertFalse((self.destination/'root.ext4.partial').exists())
        self.assertFalse(result['writes_to_card'])
        self.assertEqual(json.loads((self.destination/'manifest.json').read_text())['state'], 'verified')

    def test_changed_source_is_incomplete_and_never_promoted(self):
        calls = 0
        def inventory_with_change():
            nonlocal calls
            calls += 1
            if calls == 2:
                self.source.write_bytes(b'X' * self.source.stat().st_size)
            return [self.record]
        with self.assertRaisesRegex(card_clone.CloneError, 'source reads differed'):
            card_clone.clone_root('disk6', self.fingerprint, self.destination,
                                 inventory_fn=inventory_with_change,
                                 source_override=self.source,
                                 filesystem_check=lambda path: None)
        self.assertFalse((self.destination/'root.ext4').exists())
        self.assertEqual(json.loads((self.destination/'manifest.json').read_text())['state'], 'incomplete')

    def test_rejects_card_destination_and_changed_identity_before_copy(self):
        with self.assertRaisesRegex(card_clone.CloneError, 'on the selected card'):
            card_clone.clone_root('disk6', self.fingerprint, self.card_mount/'backup',
                                 inventory_fn=lambda: [self.record], source_override=self.source,
                                 filesystem_check=lambda path: None)
        self.assertFalse((self.card_mount/'backup').exists())
        with self.assertRaisesRegex(card_clone.CloneError, 'identity changed'):
            card_clone.clone_root('disk6', 'old-fingerprint', self.destination,
                                 inventory_fn=lambda: [self.record], source_override=self.source,
                                 filesystem_check=lambda path: None)
        self.assertFalse(self.destination.exists())


if __name__ == '__main__':
    unittest.main()
