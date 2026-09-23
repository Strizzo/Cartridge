"""Card-selection preflight must reject lookalike disks before any write phase."""

import importlib.util
from pathlib import Path
import unittest


REPO = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("cartridge_card_inventory", REPO/"installer/card_inventory.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def fixture():
    card = {
        "DeviceIdentifier": "disk6", "OSInternal": False,
        "Content": "FDisk_partition_scheme", "Size": 128_000_000_000,
        "Partitions": [
            {"DeviceIdentifier": "disk6s1", "Content": "DOS_FAT_32", "Size": 117_440_512},
            {"DeviceIdentifier": "disk6s2", "Content": "Linux", "Size": 10_351_525_376},
            {"DeviceIdentifier": "disk6s3", "Content": "Windows_NTFS", "Size": 117_513_912_320},
        ],
    }
    ssd = {
        "DeviceIdentifier": "disk7", "OSInternal": False,
        "Content": "FDisk_partition_scheme", "Size": 480_103_981_056,
        "Partitions": [{"DeviceIdentifier": "disk7s1", "Content": "Windows_NTFS", "Size": 480_101_007_360}],
    }
    info = {
        "disk6": {"DeviceIdentifier": "disk6", "ParentWholeDisk": "disk6", "WholeDisk": True,
                  "Internal": False, "VirtualOrPhysical": "Physical", "RemovableMedia": True,
                  "Ejectable": True, "BusProtocol": "USB", "MediaName": "Card Reader",
                  "TotalSize": card["Size"], "Content": card["Content"]},
        "disk7": {"DeviceIdentifier": "disk7", "ParentWholeDisk": "disk7", "WholeDisk": True,
                  "Internal": False, "VirtualOrPhysical": "Physical", "RemovableMedia": False,
                  "Ejectable": True, "BusProtocol": "USB", "MediaName": "External SSD",
                  "TotalSize": ssd["Size"], "Content": ssd["Content"]},
    }
    for disk in (card, ssd):
        for part in disk["Partitions"]:
            number = part["DeviceIdentifier"][-1]
            names = {"1": ("BOOT", "msdos"), "2": ("", ""), "3": ("EASYROMS", "exfat")}
            name, filesystem = names[number]
            if disk is ssd:
                name, filesystem = "SL-EG5", "exfat"
            info[part["DeviceIdentifier"]] = {
                "DeviceIdentifier": part["DeviceIdentifier"],
                "ParentWholeDisk": disk["DeviceIdentifier"], "WholeDisk": False,
                "VolumeName": name, "FilesystemType": filesystem,
                "VolumeUUID": "test-" + part["DeviceIdentifier"],
                "MountPoint": "/Volumes/" + name if name else "",
            }
    return [card, ssd], info


class CardInventoryTest(unittest.TestCase):
    def test_card_candidate_but_external_ssd_blocked(self):
        disks, info = fixture()
        result = module.inventory(
            list_disks=lambda *args: {"AllDisksAndPartitions": disks},
            disk_info=lambda *args: info[args[-1]],
        )
        self.assertEqual(result[0]["status"], "preserve_candidate")
        self.assertEqual(result[1]["status"], "blocked")
        self.assertIn("not reported as removable", result[1]["detail"])
        self.assertTrue(all(row["read_only_inventory"] for row in result))

    def test_identity_change_or_internal_media_fails_closed(self):
        disks, info = fixture()
        info["disk6s2"]["ParentWholeDisk"] = "disk8"
        with self.assertRaisesRegex(module.InventoryError, "Partition identity changed"):
            module.inspect_disk(disks[0], info["disk6"], {
                p["DeviceIdentifier"]: info[p["DeviceIdentifier"]] for p in disks[0]["Partitions"]
            })
        disks, info = fixture()
        info["disk6"]["Internal"] = True
        row = module.inspect_disk(disks[0], info["disk6"], {
            p["DeviceIdentifier"]: info[p["DeviceIdentifier"]] for p in disks[0]["Partitions"]
        })
        self.assertEqual(row["status"], "blocked")

    def test_unknown_layout_is_never_a_preserve_candidate(self):
        disks, info = fixture()
        disks[0]["Partitions"][2]["Content"] = "Apple_APFS"
        row = module.inspect_disk(disks[0], info["disk6"], {
            p["DeviceIdentifier"]: info[p["DeviceIdentifier"]] for p in disks[0]["Partitions"]
        })
        self.assertEqual(row["status"], "unsupported_layout")
        disks, info = fixture()
        disks[0]["Partitions"] = []
        self.assertEqual(module.inspect_disk(disks[0], info["disk6"], {})["status"], "fresh_image_candidate")

    def test_mount_state_does_not_change_inventory_fingerprint(self):
        disks, info = fixture()
        part_info = {p["DeviceIdentifier"]: info[p["DeviceIdentifier"]] for p in disks[0]["Partitions"]}
        first = module.inspect_disk(disks[0], info["disk6"], part_info)["inventory_fingerprint"]
        info["disk6s3"]["MountPoint"] = ""
        second = module.inspect_disk(disks[0], info["disk6"], part_info)["inventory_fingerprint"]
        self.assertEqual(first, second)


if __name__ == "__main__":
    unittest.main()
