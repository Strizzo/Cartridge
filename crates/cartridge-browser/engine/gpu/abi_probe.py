#!/usr/bin/env python3
"""Read ELF64 little-endian GBM symbol coverage without loading either library.

Exit 0: necessary symbol/architecture checks pass (not a GPU compatibility pass).
Exit 1: incompatible GBM symbol coverage or architecture. Exit 2: invalid input.
No driver constructors, device opens, EGL calls, subprocesses or browser launches.
"""
import argparse
import json
import mmap
import os
from pathlib import Path
import stat
import struct
import sys

HEADER = struct.Struct('<16sHHIQQQIHHHHHH')
SECTION = struct.Struct('<IIQQQQIIQQ')
SYMBOL = struct.Struct('<IBBHQQ')
DYNAMIC = struct.Struct('<qQ')
MAX_FILE = 512 * 1024 * 1024


class ElfError(ValueError):
    pass


def inspect_elf(path):
    """Accept explicit regular files, resolving ordinary library symlinks first."""
    path = Path(path).resolve(strict=True)
    if not stat.S_ISREG(path.stat().st_mode):
        raise ElfError('Expected a regular ELF file: ' + str(path))
    fd = os.open(path, os.O_RDONLY | getattr(os, 'O_NOFOLLOW', 0) | os.O_NONBLOCK)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or not HEADER.size <= info.st_size <= MAX_FILE:
            raise ElfError('Not a supported regular ELF file (64 B to 512 MiB)')
        with mmap.mmap(fd, 0, access=mmap.ACCESS_READ) as data:
            return _inspect(data, str(path))
    finally:
        os.close(fd)


def _inspect(data, path):
    def bounds(offset, size):
        if offset < 0 or size < 0 or offset + size > len(data):
            raise ElfError('ELF field outside file bounds')

    def string(section, offset):
        start, size = section[4], section[5]
        if not 0 <= offset < size:
            raise ElfError('ELF string offset outside string table')
        end = data.find(b'\0', start + offset, start + size)
        if end < 0:
            raise ElfError('Unterminated ELF string')
        return data[start + offset:end].decode('utf-8', errors='replace')

    h = HEADER.unpack_from(data)
    if h[0][:7] != b'\x7fELF\x02\x01\x01' or h[3] != 1 or h[1] not in (2, 3):
        raise ElfError('Expected an ELF64 little-endian executable/shared library')
    if h[8] != HEADER.size or h[11] != SECTION.size or not h[12]:
        raise ElfError('Missing or unsupported ELF section table')
    bounds(h[6], h[12] * SECTION.size)
    sections = [SECTION.unpack_from(data, h[6] + i * SECTION.size) for i in range(h[12])]
    for s in sections:
        if s[1] != 8:  # SHT_NOBITS has no bytes in the file.
            bounds(s[4], s[5])

    def strings_for(s):
        if s[6] >= len(sections) or sections[s[6]][1] != 3:
            raise ElfError('Invalid linked ELF string table')
        return sections[s[6]]

    exports, strong, weak = set(), set(), set()
    needed, soname, versioned = [], None, False
    symbol_tables = 0
    for s in sections:
        if s[1] == 11:  # SHT_DYNSYM; static symbols cannot satisfy dynamic linking.
            symbol_tables += 1
            if s[9] != SYMBOL.size or s[5] % SYMBOL.size:
                raise ElfError('Invalid dynamic symbol table dimensions')
            strings = strings_for(s)
            for off in range(s[4], s[4] + s[5], SYMBOL.size):
                name, flags, visibility, index, _, _ = SYMBOL.unpack_from(data, off)
                binding, kind = flags >> 4, flags & 15
                if not name or binding not in (1, 2, 10):
                    continue
                name = string(strings, name)
                if not name.startswith('gbm_'):
                    continue
                if index == 0:
                    (weak if binding == 2 else strong).add(name)
                elif visibility & 3 in (0, 3) and kind in (0, 2, 10):
                    exports.add(name)
        elif s[1] == 6:  # SHT_DYNAMIC
            if s[9] != DYNAMIC.size or s[5] % DYNAMIC.size:
                raise ElfError('Invalid dynamic table dimensions')
            strings = strings_for(s)
            for off in range(s[4], s[4] + s[5], DYNAMIC.size):
                tag, value = DYNAMIC.unpack_from(data, off)
                if tag == 0:
                    break
                if tag == 1:
                    needed.append(string(strings, value))
                elif tag == 14:
                    soname = string(strings, value)
        elif s[1] in (0x6ffffffd, 0x6ffffffe, 0x6fffffff):
            versioned = True
    if not symbol_tables:
        raise ElfError('No dynamic symbol table; cannot assess dynamic GBM coverage')
    return dict(path=path, machine=h[2], gbm_exports=sorted(exports),
                gbm_strong_imports=sorted(strong), gbm_weak_imports=sorted(weak),
                needed=needed, soname=soname, has_gnu_symbol_version_tables=versioned)


def compare(backend, consumer):
    exports = set(backend['gbm_exports'])
    required = set(consumer['gbm_strong_imports'])
    missing = sorted(required - exports)
    architecture_match = backend['machine'] == consumer['machine']
    sufficient = architecture_match and bool(required) and not missing
    return dict(schema=1, backend=backend, consumer=consumer,
                architecture_match=architecture_match,
                missing_strong_imports=missing,
                missing_weak_imports=sorted(set(consumer['gbm_weak_imports']) - exports),
                necessary_symbol_checks_pass=sufficient,
                gpu_compatibility_established=False,
                verdict='necessary-checks-only' if sufficient else 'incompatible-symbol-coverage',
                limitations=[
                    'Names/architecture only; symbol versions and dependency resolution are not validated.',
                    'Present symbols do not prove correct implementations or buffer layout interoperability.',
                    'No EGL/GLES, kernel ABI, device permission, sandbox, rendering or browser trial was performed.'])


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--backend', type=Path, required=True)
    parser.add_argument('--consumer', type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        report = compare(inspect_elf(args.backend), inspect_elf(args.consumer))
    except (OSError, ElfError, struct.error) as error:
        print(json.dumps(dict(schema=1, verdict='invalid-input', error=str(error)), indent=2))
        return 2
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report['necessary_symbol_checks_pass'] else 1


if __name__ == '__main__':
    sys.exit(main())
