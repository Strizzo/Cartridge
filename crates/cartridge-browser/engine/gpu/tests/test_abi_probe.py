import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

GPU = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('abi_probe', GPU / 'abi_probe.py')
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


def elf(exports=(), strong=(), weak=(), machine=183, hidden=(), objects=()):
    """Minimal ELF fixture with a real dynamic symbol/string/dynamic section."""
    strings = bytearray(b'\0')

    def name(value):
        offset = len(strings)
        strings.extend(value.encode() + b'\0')
        return offset

    symbols = bytearray(probe.SYMBOL.pack(0, 0, 0, 0, 0, 0))
    for names, binding, visibility, index, kind in [
            (exports, 1, 0, 0xfff1, 2), (strong, 1, 0, 0, 2),
            (weak, 2, 0, 0, 2), (hidden, 1, 2, 0xfff1, 2),
            (objects, 1, 0, 0xfff1, 1)]:
        for symbol in names:
            symbols.extend(probe.SYMBOL.pack(name(symbol), binding << 4 | kind, visibility, index, 0, 0))
    needed, soname = name('libc.so.6'), name('libgbm.so.1')
    dynamic = probe.DYNAMIC.pack(1, needed) + probe.DYNAMIC.pack(14, soname) + probe.DYNAMIC.pack(0, 0)
    start = probe.HEADER.size + 4 * probe.SECTION.size
    str_off, sym_off = start, start + len(strings)
    dyn_off = sym_off + len(symbols)
    sections = bytes(probe.SECTION.size)
    sections += probe.SECTION.pack(0, 3, 0, 0, str_off, len(strings), 0, 0, 1, 0)
    sections += probe.SECTION.pack(0, 11, 0, 0, sym_off, len(symbols), 1, 1, 8, probe.SYMBOL.size)
    sections += probe.SECTION.pack(0, 6, 0, 0, dyn_off, len(dynamic), 1, 0, 8, probe.DYNAMIC.size)
    header = probe.HEADER.pack(b'\x7fELF\x02\x01\x01' + bytes(9), 3, machine, 1, 0, 0,
                              probe.HEADER.size, 0, probe.HEADER.size, 0, 0,
                              probe.SECTION.size, 4, 0)
    return header + sections + strings + symbols + dynamic


class ProbeTests(unittest.TestCase):
    def setUp(self):
        (GPU / 'build').mkdir(exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(prefix='elf-tests-', dir=GPU / 'build')
        self.root = Path(self.directory.name)
        self.addCleanup(self.directory.cleanup)

    def inspect(self, data, name='fixture.so'):
        p = self.root / name
        p.write_bytes(data)
        return probe.inspect_elf(p)

    def test_target_missing_strong_and_optional_weak_names_are_separate(self):
        backend = self.inspect(elf(exports=['gbm_create_device']), 'backend.so')
        consumer = self.inspect(elf(strong=['gbm_create_device', 'gbm_bo_get_plane_count'],
                                    weak=['gbm_bo_get_fd_for_plane']), 'consumer.so')
        result = probe.compare(backend, consumer)
        self.assertEqual(result['missing_strong_imports'], ['gbm_bo_get_plane_count'])
        self.assertEqual(result['missing_weak_imports'], ['gbm_bo_get_fd_for_plane'])
        self.assertFalse(result['necessary_symbol_checks_pass'])
        self.assertFalse(result['gpu_compatibility_established'])

    def test_all_names_present_still_does_not_claim_hardware_compatibility(self):
        backend = self.inspect(elf(exports=['gbm_create_device']))
        consumer = self.inspect(elf(strong=['gbm_create_device'], weak=['gbm_bo_get_fd_for_plane']))
        result = probe.compare(backend, consumer)
        self.assertTrue(result['necessary_symbol_checks_pass'])
        self.assertFalse(result['gpu_compatibility_established'])
        self.assertEqual(result['verdict'], 'necessary-checks-only')

    def test_architecture_mismatch_is_not_a_pass(self):
        a = self.inspect(elf(exports=['gbm_create_device'], machine=62))
        b = self.inspect(elf(strong=['gbm_create_device']))
        self.assertFalse(probe.compare(a, b)['necessary_symbol_checks_pass'])

    def test_no_consumer_gbm_imports_cannot_pass_vacuously(self):
        a = self.inspect(elf(exports=['gbm_create_device']))
        b = self.inspect(elf())
        self.assertFalse(probe.compare(a, b)['necessary_symbol_checks_pass'])

    def test_hidden_or_data_symbols_cannot_supply_gbm_function_imports(self):
        a = self.inspect(elf(hidden=['gbm_create_device'], objects=['gbm_bo_map']))
        b = self.inspect(elf(strong=['gbm_create_device', 'gbm_bo_map']))
        self.assertEqual(probe.compare(a, b)['missing_strong_imports'], ['gbm_bo_map', 'gbm_create_device'])

    def test_dynamic_metadata_is_read_without_loading(self):
        a = self.inspect(elf())
        self.assertEqual(a['soname'], 'libgbm.so.1')
        self.assertEqual(a['needed'], ['libc.so.6'])

    def test_normal_library_symlink_is_resolved(self):
        self.inspect(elf(), 'real.so')
        link = self.root / 'libgbm.so.1'
        link.symlink_to('real.so')
        self.assertEqual(probe.inspect_elf(link)['machine'], 183)

    def test_fifo_rejected_before_opening_it(self):
        path = self.root / 'not-a-library'
        os.mkfifo(path)
        with patch.object(probe.os, 'open', side_effect=AssertionError('must not open FIFO')):
            with self.assertRaises(probe.ElfError):
                probe.inspect_elf(path)

    def test_truncated_header_or_wrong_class_or_endianness(self):
        for data in [b'', b'not ELF' * 10, elf()[:63], elf()[:4] + b'\x01' + elf()[5:],
                     elf()[:5] + b'\x02' + elf()[6:]]:
            with self.subTest(data=data[:8]):
                with self.assertRaises(probe.ElfError):
                    self.inspect(data)

    def test_section_bounds_and_string_offsets_are_checked(self):
        for offset, value in [(40, 2**63),  # e_shoff
                              (probe.HEADER.size + probe.SECTION.size + 24, 2**63),  # strtab offset
                              (probe.HEADER.size + 2 * probe.SECTION.size + 40, 999)]:  # sh_link
            data = bytearray(elf(exports=['gbm_create_device']))
            struct.pack_into('<I' if offset % probe.SECTION.size == 40 and offset > 64 else '<Q', data, offset, value)
            with self.subTest(offset=offset):
                with self.assertRaises(probe.ElfError):
                    self.inspect(data)

    def test_cli_exit_codes_are_machine_readable(self):
        backend, consumer = self.root / 'backend.so', self.root / 'consumer.so'
        backend.write_bytes(elf(exports=['gbm_create_device']))
        command = [sys.executable, str(GPU / 'abi_probe.py'), '--backend', str(backend), '--consumer', str(consumer)]
        for data, status in [(elf(strong=['gbm_create_device']), 0),
                             (elf(strong=['gbm_bo_map']), 1), (b'invalid', 2)]:
            consumer.write_bytes(data)
            result = subprocess.run(command, capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, status, result.stderr)
            report = json.loads(result.stdout)
            self.assertNotEqual(report.get('gpu_compatibility_established'), True)
