"""The Mac UI bridge must refuse a game card or external SSD before writing."""

import importlib.util
from pathlib import Path
import unittest
from unittest import mock


SOURCE = Path(__file__).resolve().parents[1] / 'installer/mac_bridge.py'
spec = importlib.util.spec_from_file_location('mac_bridge', SOURCE)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


def card(status='unsupported_layout', filesystem='exfat'):
    return {
        'identifier': 'disk6', 'status': status, 'size_bytes': 128_000_000_000,
        'inventory_fingerprint': 'expected',
        'partitions': [{'filesystem': filesystem, 'volume_uuid': 'empty-uuid'}],
    }


class InstallerBridgeSelectionTest(unittest.TestCase):
    def test_only_matching_spare_passes_to_backend(self):
        with mock.patch.object(bridge, 'inventory', return_value=[card()]):
            self.assertEqual(
                bridge.selected_empty_card('disk6', 'expected', 128_000_000_000),
                card(),
            )
            for disk, fingerprint, size in (
                ('disk7', 'expected', 128_000_000_000),
                ('disk6', 'stale', 128_000_000_000),
                ('disk6', 'expected', 64_000_000_000),
            ):
                with self.subTest(disk=disk, fingerprint=fingerprint, size=size):
                    with self.assertRaises(RuntimeError):
                        bridge.selected_empty_card(disk, fingerprint, size)

    def test_existing_game_card_and_non_exfat_media_are_rejected(self):
        for row in (card(status='preserve_candidate'), card(filesystem='apfs')):
            with self.subTest(row=row), mock.patch.object(bridge, 'inventory', return_value=[row]):
                with self.assertRaisesRegex(RuntimeError, 'empty exFAT spare'):
                    bridge.selected_empty_card('disk6', 'expected', 128_000_000_000)


if __name__ == '__main__':
    unittest.main()
