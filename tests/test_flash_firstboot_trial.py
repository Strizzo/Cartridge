"""A card's identity must survive macOS's exFAT metadata change on unmount."""

import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest import mock


SOURCE = Path(__file__).resolve().parents[1] / 'sim/device/flash-firstboot-trial.py'
spec = importlib.util.spec_from_file_location('flash_firstboot_trial', SOURCE)
flash = importlib.util.module_from_spec(spec)
spec.loader.exec_module(flash)


class FlashTargetIdentityTest(unittest.TestCase):
    def setUp(self):
        self.args = SimpleNamespace(disk='disk6', fingerprint='mounted-fingerprint',
                                    card_bytes=128_000_000_000, empty_uuid='empty-uuid')
        self.mounted = {
            'identifier': 'disk6', 'media_name': 'MassStorageClass', 'bus': 'USB',
            'size_bytes': self.args.card_bytes, 'partition_map': 'FDisk_partition_scheme',
            'status': 'unsupported_layout', 'inventory_fingerprint': self.args.fingerprint,
            'partitions': [{
                'identifier': 'disk6s1', 'content': 'Windows_NTFS',
                'size_bytes': 127_982_895_104, 'device_size_bytes': 127_966_248_960,
                'volume_name': 'Untitled', 'filesystem': 'exfat',
                'volume_uuid': self.args.empty_uuid, 'mount_point': '/Volumes/Untitled',
                'mounted': True,
            }],
        }
        self.whole = {
            'DeviceIdentifier': 'disk6', 'ParentWholeDisk': 'disk6',
            'WholeDisk': True, 'VirtualOrPhysical': 'Physical', 'Internal': False,
            'RemovableMedia': True, 'Ejectable': True, 'WritableMedia': True,
            'DeviceBlockSize': 512, 'TotalSize': self.args.card_bytes,
        }

    def check(self, row, **kwargs):
        with (mock.patch.object(flash, 'inventory', return_value=[row]),
              mock.patch.object(flash, 'disk_info', return_value=self.whole),
              mock.patch.object(flash.os.path, 'ismount', return_value=True),
              mock.patch.object(flash.Path, 'iterdir', return_value=iter(()))):
            return flash.check_target(self.args, **kwargs)

    def unmounted(self):
        row = {**self.mounted, 'inventory_fingerprint': 'changed-after-unmount'}
        row['partitions'] = [{**self.mounted['partitions'][0],
                              'device_size_bytes': 127_982_895_104,
                              'volume_name': '', 'mount_point': '', 'mounted': False}]
        return row

    def test_unmount_metadata_change_does_not_abort_verified_card(self):
        original = self.check(self.mounted)
        row = self.unmounted()
        self.assertIs(self.check(row, mounted=False, original=original), row)

    def test_unmounted_check_requires_original_mounted_identity(self):
        with self.assertRaisesRegex(RuntimeError, 'identity changed'):
            self.check(self.unmounted(), mounted=False)

    def test_unmounted_check_rejects_different_partition(self):
        original = self.check(self.mounted)
        for field, changed in (('content', 'Linux'), ('size_bytes', 12_000),
                               ('identifier', 'disk6s2')):
            row = self.unmounted()
            row['partitions'][0][field] = changed
            with self.subTest(field=field), self.assertRaisesRegex(RuntimeError, 'identity changed'):
                self.check(row, mounted=False, original=original)


if __name__ == '__main__':
    unittest.main()
