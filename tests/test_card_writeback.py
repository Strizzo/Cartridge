"""Writeback is transactional against disposable files; never use a real card here."""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO/'installer'))
import card_writeback  # noqa: E402


def sha(data):
    return hashlib.sha256(data).hexdigest()


class CardWritebackTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='cartridge-writeback-')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.original = b'A' * (5 * 1024 * 1024)
        self.updated = b'B' * (5 * 1024 * 1024)
        self.target = self.base/'disposable-partition.ext4'
        self.target.write_bytes(self.original)
        self.prepared = self.base/'prepared.ext4'
        self.prepared.write_bytes(self.updated)
        self.backup = self.base/'backup'
        self.backup.mkdir()
        (self.backup/'root.ext4').write_bytes(self.original)
        self.fingerprint = 'selected-card-fingerprint'
        (self.backup/'manifest.json').write_text(json.dumps({
            'state': 'verified', 'selected_disk': 'disk6',
            'inventory_fingerprint': self.fingerprint, 'source_partition': 'disk6s2',
            'source_bytes': len(self.original), 'sha256': sha(self.original),
            'source_read_passes': 2, 'destination_readback_verified': True,
            'ext4_check': 'clean',
        }))
        self.journal = self.base/'journal'
        self.games = self.base/'games.img'
        self.games.write_bytes(b'game and save bytes')
        self.boot = self.base/'boot.img'
        self.boot.write_bytes(b'boot bytes')
        self.record = {
            'identifier': 'disk6', 'inventory_fingerprint': self.fingerprint,
            'status': 'preserve_candidate',
            'partitions': [
                {'identifier': 'disk6s1', 'content': 'DOS_FAT_32',
                 'size_bytes': 100, 'device_size_bytes': 100, 'mounted': False, 'mount_point': ''},
                {'identifier': 'disk6s2', 'content': 'Linux', 'size_bytes': len(self.original),
                 'device_size_bytes': len(self.original), 'mounted': False, 'mount_point': ''},
                {'identifier': 'disk6s3', 'content': 'Windows_NTFS',
                 'size_bytes': 100, 'device_size_bytes': 100, 'mounted': False, 'mount_point': ''},
            ],
        }

    def apply(self, **kwargs):
        options = dict(inventory_fn=lambda: [self.record], target_override=self.target,
                       filesystem_check=lambda path: None)
        options.update(kwargs)
        return card_writeback.writeback('disk6', self.fingerprint, self.backup,
                                        self.prepared, sha(self.updated), self.journal,
                                        **options)

    def test_success_changes_only_disposable_linux_partition(self):
        result = self.apply()
        self.assertEqual(result['state'], 'verified')
        self.assertEqual(self.target.read_bytes(), self.updated)
        self.assertEqual(self.games.read_bytes(), b'game and save bytes')
        self.assertEqual(self.boot.read_bytes(), b'boot bytes')
        self.assertEqual((self.backup/'root.ext4').read_bytes(), self.original)
        self.assertEqual(json.loads((self.journal/'manifest.json').read_text())['state'], 'verified')

    def test_partial_write_failure_rolls_back_and_verifies(self):
        def fail_after_first_chunk(written):
            raise RuntimeError(f'injected failure after {written} bytes')
        with self.assertRaisesRegex(card_writeback.WritebackError, 'rolled_back_and_verified'):
            self.apply(chunk_hook=fail_after_first_chunk)
        self.assertEqual(self.target.read_bytes(), self.original)
        self.assertEqual(json.loads((self.journal/'manifest.json').read_text())['state'],
                         'rolled_back_and_verified')

    def test_interrupted_write_can_restore_from_durable_journal(self):
        # Simulate a killed process after recording write_started and writing
        # only the first chunk; the next process uses the journal to restore.
        self.target.write_bytes(self.updated[:4 * 1024 * 1024] + self.original[4 * 1024 * 1024:])
        self.journal.mkdir()
        card_writeback.save_journal(self.journal/'manifest.json', {
            'state': 'write_started', 'disk': 'disk6', 'fingerprint': self.fingerprint,
            'partition': 'disk6s2', 'bytes': len(self.original),
            'original_backup': str(self.backup/'root.ext4'),
            'original_sha256': sha(self.original), 'prepared_sha256': sha(self.updated),
            'target': '/dev/rdisk6s2',
        })
        result = card_writeback.restore(self.journal, inventory_fn=lambda: [self.record],
                                        target_override=self.target)
        self.assertEqual(result['state'], 'rolled_back_and_verified')
        self.assertEqual(self.target.read_bytes(), self.original)

    def test_changed_card_and_identity_stop_before_write(self):
        self.target.write_bytes(b'C' * len(self.original))
        with self.assertRaisesRegex(card_writeback.WritebackError, 'changed since'):
            self.apply()
        self.assertFalse(self.journal.exists())
        self.assertEqual(self.target.read_bytes(), b'C' * len(self.original))
        self.target.write_bytes(self.original)
        self.record['inventory_fingerprint'] = 'different-card'
        with self.assertRaisesRegex(card_writeback.CloneError, 'identity changed'):
            self.apply()
        self.assertEqual(self.target.read_bytes(), self.original)

    def test_bad_prepared_image_and_backup_stop_before_write(self):
        self.prepared.write_bytes(b'C' * len(self.updated))
        with self.assertRaisesRegex(card_writeback.WritebackError, 'Prepared image checksum'):
            self.apply()
        self.assertFalse(self.journal.exists())
        self.prepared.write_bytes(self.updated)
        (self.backup/'root.ext4').write_bytes(b'D' * len(self.original))
        with self.assertRaisesRegex(card_writeback.WritebackError, 'Original image checksum'):
            self.apply()
        self.assertEqual(self.target.read_bytes(), self.original)

    def test_failed_ext4_preflight_does_not_write(self):
        def bad_fsck(path):
            raise OSError('invalid ext4')
        with self.assertRaisesRegex(card_writeback.WritebackError, 'ext4 check failed'):
            self.apply(filesystem_check=bad_fsck)
        self.assertFalse(self.journal.exists())
        self.assertEqual(self.target.read_bytes(), self.original)

    def test_journal_on_selected_card_is_rejected(self):
        games_mount = self.base/'mounted-games'
        games_mount.mkdir()
        self.record['partitions'][2].update(mounted=True, mount_point=str(games_mount))
        self.journal = games_mount/'journal'
        with self.assertRaisesRegex(card_writeback.WritebackError, 'outside the selected card'):
            self.apply()
        self.assertEqual(self.target.read_bytes(), self.original)

    def test_post_write_ext4_failure_restores_original(self):
        calls = 0
        def fsck(path):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise OSError('injected device ext4 failure')
        with self.assertRaisesRegex(card_writeback.WritebackError, 'rolled_back_and_verified'):
            self.apply(filesystem_check=fsck)
        self.assertEqual(calls, 2)
        self.assertEqual(self.target.read_bytes(), self.original)

    def test_real_ext4_image_write_and_readback(self):
        tools = [Path('/opt/homebrew/opt/e2fsprogs/sbin')/name
                 for name in ('mkfs.ext4', 'e2label', 'e2fsck')]
        if not all(path.is_file() for path in tools):
            self.skipTest('e2fsprogs tools not installed')
        with self.target.open('wb') as stream:
            stream.truncate(16 * 1024 * 1024)
        subprocess.run([str(tools[0]), '-F', '-q', str(self.target)], check=True)
        original_hash = sha(self.target.read_bytes())
        shutil.copyfile(self.target, self.backup/'root.ext4')
        shutil.copyfile(self.target, self.prepared)
        subprocess.run([str(tools[1]), str(self.prepared), 'CARTRIDGE'], check=True)
        prepared_hash = sha(self.prepared.read_bytes())
        manifest = json.loads((self.backup/'manifest.json').read_text())
        manifest.update(source_bytes=self.target.stat().st_size, sha256=original_hash)
        (self.backup/'manifest.json').write_text(json.dumps(manifest))
        self.record['partitions'][1]['size_bytes'] = self.target.stat().st_size
        self.record['partitions'][1]['device_size_bytes'] = self.target.stat().st_size
        result = card_writeback.writeback('disk6', self.fingerprint, self.backup,
                                          self.prepared, prepared_hash, self.journal,
                                          inventory_fn=lambda: [self.record],
                                          target_override=self.target)
        self.assertEqual(result['state'], 'verified')
        self.assertEqual(sha(self.target.read_bytes()), prepared_hash)


if __name__ == '__main__':
    unittest.main()
