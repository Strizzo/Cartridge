#!/usr/bin/env python3
"""Read-only macOS inventory for CartridgeOS SD installation.

This module never unmounts, writes, formats, or chooses a target automatically.
The preserve verdict is a partition-layout candidate, not proof that the Linux
system is compatible. A later installer must inspect a clone of that partition.
"""

import argparse
import hashlib
import json
import plistlib
import re
import subprocess


DISK_ID = re.compile(r"disk[0-9]+\Z")
MINIMUM_BYTES = 8 * 1024**3


class InventoryError(RuntimeError):
    pass


def diskutil_plist(*args):
    try:
        result = subprocess.run(
            ["diskutil", *args], check=True, capture_output=True, timeout=20
        )
        value = plistlib.loads(result.stdout)
        if not isinstance(value, dict):
            raise ValueError("expected a property-list dictionary")
        return value
    except (OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired, ValueError) as exc:
        raise InventoryError(f"diskutil {' '.join(args)} failed: {exc}") from exc


def partition_record(partition, whole, info):
    identifier = partition.get("DeviceIdentifier")
    if (
        not isinstance(info, dict)
        or not isinstance(identifier, str)
        or not re.fullmatch(re.escape(whole) + r"s[0-9]+", identifier)
        or info.get("DeviceIdentifier") != identifier
        or info.get("ParentWholeDisk") != whole
        or info.get("WholeDisk") is not False
    ):
        raise InventoryError(f"Partition identity changed while inspecting {whole}")
    return {
        "identifier": identifier,
        "content": partition.get("Content"),
        "size_bytes": partition.get("Size"),
        "volume_name": info.get("VolumeName") or partition.get("VolumeName") or "",
        "filesystem": info.get("FilesystemType") or "",
        "volume_uuid": info.get("VolumeUUID") or partition.get("VolumeUUID") or "",
        "mount_point": info.get("MountPoint") or "",
        "mounted": bool(info.get("MountPoint")),
    }


def layout_verdict(disk, partitions):
    if not partitions:
        return "fresh_image_candidate", "No partitions found; existing data has not been ruled out"
    if disk.get("Content") != "FDisk_partition_scheme" or len(partitions) != 3:
        return "unsupported_layout", "Expected the three-partition R36S Plus MBR layout"
    boot, system, roms = partitions
    if (
        [part["identifier"] for part in partitions] !=
        [disk["DeviceIdentifier"] + suffix for suffix in ("s1", "s2", "s3")]
        or any(not isinstance(part["size_bytes"], int) or part["size_bytes"] <= 0
               for part in partitions)
        or boot["content"] != "DOS_FAT_32"
        or boot["filesystem"] != "msdos"
        or boot["volume_name"] != "BOOT"
        or system["content"] != "Linux"
        or roms["content"] != "Windows_NTFS"
        or roms["filesystem"] != "exfat"
        or roms["volume_name"] != "EASYROMS"
    ):
        return "unsupported_layout", "Boot, Linux, or games partition differs from the supported layout"
    return "preserve_candidate", "Layout matches; offline Linux compatibility remains to be checked"


def inspect_disk(disk, info, partition_info):
    identifier = disk.get("DeviceIdentifier")
    if not isinstance(identifier, str) or not DISK_ID.fullmatch(identifier):
        raise InventoryError("Invalid whole-disk identifier in diskutil output")
    if info.get("DeviceIdentifier") != identifier or info.get("ParentWholeDisk") != identifier:
        raise InventoryError(f"Disk identity changed while inspecting {identifier}")
    listed_partitions = disk.get("Partitions", [])
    if not isinstance(listed_partitions, list) or not all(isinstance(p, dict) for p in listed_partitions):
        raise InventoryError(f"Invalid partition list for {identifier}")
    partitions = [partition_record(p, identifier, partition_info.get(p.get("DeviceIdentifier")))
                  for p in listed_partitions]
    size = info.get("TotalSize")
    reasons = []
    if info.get("WholeDisk") is not True:
        reasons.append("Not a whole disk")
    if info.get("Internal") is not False or disk.get("OSInternal") is not False:
        reasons.append("Internal or unknown internal-disk status")
    if info.get("VirtualOrPhysical") != "Physical":
        reasons.append("Virtual disk or unknown physical-media status")
    if info.get("RemovableMedia") is not True:
        reasons.append("Media is not reported as removable")
    if info.get("Ejectable") is not True:
        reasons.append("Media is not reported as ejectable")
    if not isinstance(size, int) or isinstance(size, bool) or size < MINIMUM_BYTES:
        reasons.append("Capacity is below 8 GiB or unknown")
    if size != disk.get("Size"):
        reasons.append("Disk size changed while inspecting")
    if info.get("Content") != disk.get("Content"):
        reasons.append("Partition map changed while inspecting")
    if reasons:
        status, detail = "blocked", "; ".join(reasons)
    else:
        status, detail = layout_verdict(disk, partitions)
    record = {
        "identifier": identifier,
        "media_name": info.get("MediaName") or "",
        "bus": info.get("BusProtocol") or "",
        "size_bytes": size,
        "partition_map": disk.get("Content"),
        "partitions": partitions,
        "status": status,
        "detail": detail,
        "read_only_inventory": True,
    }
    identity = {key: record[key] for key in ("identifier", "media_name", "bus", "size_bytes", "partition_map")}
    identity["partitions"] = [{key: part[key] for key in ("identifier", "content", "size_bytes", "volume_name", "filesystem", "volume_uuid")}
                              for part in partitions]
    record["inventory_fingerprint"] = hashlib.sha256(
        json.dumps(identity, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    return record


def inventory(*, list_disks=diskutil_plist, disk_info=diskutil_plist):
    listing = list_disks("list", "-plist", "external")
    disks = listing.get("AllDisksAndPartitions")
    if not isinstance(disks, list):
        raise InventoryError("diskutil did not return an external-disk inventory")
    result = []
    for disk in disks:
        if not isinstance(disk, dict):
            raise InventoryError("Unexpected external-disk record")
        identifier = disk.get("DeviceIdentifier")
        if not isinstance(identifier, str) or not DISK_ID.fullmatch(identifier):
            raise InventoryError("Unexpected disk identifier in external-disk inventory")
        info = disk_info("info", "-plist", identifier)
        partition_info = {}
        listed_partitions = disk.get("Partitions", [])
        if not isinstance(listed_partitions, list) or not all(isinstance(part, dict) for part in listed_partitions):
            raise InventoryError(f"Invalid partition list for {identifier}")
        for part in listed_partitions:
            part_id = part.get("DeviceIdentifier")
            if not isinstance(part_id, str) or not re.fullmatch(re.escape(identifier) + r"s[0-9]+", part_id):
                raise InventoryError(f"Unexpected partition identifier on {identifier}")
            partition_info[part_id] = disk_info("info", "-plist", part_id)
        result.append(inspect_disk(disk, info, partition_info))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--disk", help="Inspect one whole disk, such as disk6; never accepts a partition")
    args = parser.parse_args()
    if args.disk and not DISK_ID.fullmatch(args.disk):
        parser.error("--disk must identify a whole disk, such as disk6")
    try:
        records = inventory()
        if args.disk:
            records = [record for record in records if record["identifier"] == args.disk]
            if not records:
                raise InventoryError("Selected disk is absent from the external-disk inventory")
    except InventoryError as exc:
        parser.exit(2, f"Card inventory stopped: {exc}\n")
    print(json.dumps({"disks": records, "writes_performed": False}, indent=2))


if __name__ == "__main__":
    main()
