#!/usr/bin/env python3
"""Validated JSON bridge between the macOS installer UI and the spare-card backend.

This initial Mac installer accepts a local, already verified first-boot image
and one explicitly selected empty exFAT card. Existing game cards are never
passed to the raw writer.
"""

import argparse
from contextlib import redirect_stdout
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

from card_inventory import inventory


ROOT = Path(__file__).resolve().parents[1]
DEVICE = ROOT / 'sim/device'


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


flash = load_module('flash_firstboot_trial', DEVICE / 'flash-firstboot-trial.py')
verify = load_module('verify_firstboot_trial', DEVICE / 'verify-firstboot-trial.py')


def image_context(image):
    image = Path(image)
    if image.is_symlink() or not image.is_dir() or not image.name.endswith('.sparsebundle'):
        raise RuntimeError('Select a real .sparsebundle first-boot image')
    image = image.resolve(strict=True)
    report_path = image.parent / 'firstboot-image-report.json'
    if report_path.is_symlink():
        raise RuntimeError('Image report must not be a symlink')
    report = json.loads(report_path.read_text())
    if (report.get('state') != 'virtual_firstboot_image_verified' or
            report.get('image') != str(image) or
            not isinstance(report.get('logical_bytes'), int)):
        raise RuntimeError('The selected image has no matching verified build report')
    volume = next((p for p in image.parents if os.path.ismount(p)), None)
    if volume is None or volume == Path('/'):
        raise RuntimeError('Choose an image on an external workspace volume')
    detail = flash.disk_info(volume)
    if (detail.get('MountPoint') != str(volume) or
            not detail.get('VolumeUUID') or
            detail.get('Internal') is not False):
        raise RuntimeError('Image workspace volume is not an external mounted disk')
    return image, report, volume, detail['VolumeUUID']


def selected_empty_card(disk, fingerprint, expected_bytes):
    matches = [r for r in inventory() if r['identifier'] == disk]
    if len(matches) != 1:
        raise RuntimeError('Selected card is absent')
    row = matches[0]
    part = row['partitions']
    if (row['inventory_fingerprint'] != fingerprint or
            row['status'] != 'unsupported_layout' or
            row['size_bytes'] != expected_bytes or len(part) != 1 or
            part[0]['filesystem'] != 'exfat' or not part[0]['volume_uuid']):
        raise RuntimeError('Only the selected empty exFAT spare card may be replaced')
    return row


def backend_args(args, *, require_card):
    image, report, volume, volume_uuid = image_context(args.image)
    card_bytes = report['logical_bytes']
    row = selected_empty_card(args.disk, args.fingerprint, card_bytes) if require_card else None
    context = argparse.Namespace(
        disk=args.disk, fingerprint=args.fingerprint,
        empty_uuid=row['partitions'][0]['volume_uuid'] if row else '',
        image=image, source_volume=volume, source_volume_uuid=volume_uuid,
        boot_sha256=report['boot_sha256'], root_sha256=report['root_sha256'],
        root_bytes=report.get('root_bytes'), card_bytes=card_bytes,
        report_dir=image.parent,
        mount_guard=getattr(args, 'mount_guard', None),
    )
    if context.root_bytes is None:
        source = None
        try:
            source, _ = flash.attach_source(context)
            context.root_bytes = flash.disk_info(source + 's2')['IOKitSize']
        finally:
            if source is not None:
                flash.command('/usr/bin/hdiutil', 'detach', source)
    if not isinstance(context.root_bytes, int) or context.root_bytes <= 0:
        raise RuntimeError('Image has no valid Linux root size')
    return context, report


def run(args):
    if args.action == 'cards':
        return {'cards': inventory()}
    if args.action == 'image':
        image, report, volume, _ = image_context(args.image)
        return {'image': str(image), 'volume': str(volume),
                'card_bytes': report['logical_bytes'],
                'build_revision': report['build_revision'],
                'boot_logo_sha256': report.get('cartridge_boot_logo_sha256')}
    if args.action in {'preflight', 'write'}:
        context, _ = backend_args(args, require_card=True)
        if args.action == 'preflight':
            return flash.preflight(context)
        if context.mount_guard is None:
            raise RuntimeError('The Mac installer mount guard is missing')
        return flash.flash(context)
    if args.action == 'verify':
        context, _ = backend_args(args, require_card=False)
        return verify.verify(context)
    raise RuntimeError('Unknown installer action')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('cards', 'image', 'preflight', 'write', 'verify'))
    parser.add_argument('--disk')
    parser.add_argument('--fingerprint')
    parser.add_argument('--image', type=Path)
    parser.add_argument('--mount-guard', type=Path)
    args = parser.parse_args()
    try:
        if args.action not in {'cards', 'image'} and (not args.disk or not args.fingerprint):
            raise RuntimeError('An explicit disk and recorded inventory fingerprint are required')
        if args.action != 'cards' and args.image is None:
            raise RuntimeError('Select a verified local image')
        with redirect_stdout(sys.stderr):
            result = run(args)
        print(json.dumps(result, indent=2), flush=True)
        if args.action in {'write', 'verify'} and result['state'] != 'verified_and_ejected':
            parser.exit(2, 'Card is not fully verified; keep it out of the handheld.\n')
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
        parser.exit(2, f'Cartridge installer stopped: {exc}\n')


if __name__ == '__main__':
    main()
