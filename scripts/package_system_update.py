#!/usr/bin/env python3
"""Build or verify offline CartridgeOS OTA artifacts; never publish or execute them.

Unsigned payloads are deliberately named *.unsigned.json. OpenSSL 3 performs
optional local Ed25519 signing; Python never opens the private key.
"""

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parents[1]
KEY_ID = "cartridge-os-v1"
TARGET = "r36s-plus-aarch64"
MIN_RUNTIME = "0.6.1"
MAX_ARCHIVE = 128 * 1024 * 1024
MAX_UNPACKED = 384 * 1024 * 1024
MAX_FILES = 8192
MAX_MANIFEST = 4 * 1024 * 1024
MAX_PATH = 220
REQUIRED = {"cartridge", "game-library.py", "registry.json"}
EXCLUDED_PARTS = {
    "boot", "dev", "diagnostics", "logs", "secrets", "credentials",
    "session", "setup", "tools", "__pycache__", "node_modules", "target",
}
EXCLUDED_NAMES = {
    "logo.bmp", "boot_logo.png", "cartridge-boot", "cartridge-boot.sh",
    "cartridge-boot.service", "cartridge-session.py", "setup-primary.py",
    "autosetup.sh", "system-update.json", "system-update.unsigned.json",
    "system-release.json", "id_rsa", "id_ed25519", "authorized_keys",
    "known_hosts", "wpa_supplicant.conf",
}
SEMVER = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def semver(value):
    require(isinstance(value, str) and len(value) <= 64 and SEMVER.fullmatch(value),
            "Expected a stable semantic version without prerelease or build metadata")
    require(all(int(part) <= 2**64 - 1 for part in value.split(".")), "Semantic version component exceeds u64")
    return value


def release_id(version, revision):
    semver(version)
    require(isinstance(revision, str) and re.fullmatch(r"[0-9a-f]{40}", revision),
            "Revision must be 40 lowercase hex characters")
    value = version + "-" + revision[:12]
    require(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,99}", value),
            "Release ID must be ASCII safe and at most 100 characters (no build metadata)")
    return value


def safe_path(name):
    require(isinstance(name, str) and 0 < len(name) <= MAX_PATH and name.isascii(),
            "Package path must be ASCII and at most 220 bytes")
    parts = name.split("/")
    require(len(parts) <= 12 and all(re.fullmatch(r"[A-Za-z0-9_-][A-Za-z0-9_.-]*", p)
                and not p.endswith(".") for p in parts), "Unsafe package path: " + repr(name))
    # Reject names USTAR cannot represent without a PAX/GNU extension.
    tarfile.TarInfo(name).tobuf(format=tarfile.USTAR_FORMAT, encoding="ascii")
    return name


def excluded(name):
    parts = name.lower().split("/")
    return (any(p.startswith(".") or p in EXCLUDED_PARTS for p in parts)
            or parts[-1] in EXCLUDED_NAMES
            or parts[-1].endswith((".pem", ".key", ".p12", ".pfx", ".log", ".pyc"))
            or any(p.startswith(("secret", "credential", "diagnostic")) for p in parts))


def allowed(name):
    return (not excluded(name) and (name in REQUIRED
            or name == "assets/gamecontrollerdb.txt"
            or any(name.startswith("assets/" + folder + "/") for folder in ("fonts", "icons", "overlays", "brand"))
            or name.startswith("lua_cartridges/")))


def regular_open(path):
    """Do not follow file symlinks, including a replacement since collection."""
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode), "Expected a regular file: " + str(path))
    fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0))
    stream = os.fdopen(fd, "rb")
    after = os.fstat(stream.fileno())
    if not stat.S_ISREG(after.st_mode) or (before.st_dev, before.st_ino) != (after.st_dev, after.st_ino):
        stream.close()
        raise ValueError("Source file changed while opening")
    return stream


def check_elf(header):
    require(len(header) >= 64 and header[:7] == b"\x7fELF\x02\x01\x01"
            and header[7] in (0, 3)
            and struct.unpack_from("<HHI", header, 16) in ((2, 183, 1), (3, 183, 1))
            and struct.unpack_from("<H", header, 52)[0] == 64,
            "cartridge must be a 64-bit little-endian AArch64 Linux ELF executable")


def collect(bundle):
    require(bundle.is_dir() and not bundle.is_symlink(), "Bundle must be a real directory")
    files = []

    def visit(directory, prefix=""):
        for child in sorted(directory.iterdir()):
            name = prefix + child.name
            info = child.lstat()
            require(not stat.S_ISLNK(info.st_mode), "Symlinks are forbidden: " + name)
            if excluded(name):
                continue
            if not prefix and child.name not in REQUIRED | {"assets", "lua_cartridges"}:
                continue
            if prefix == "assets/" and child.name not in {"fonts", "icons", "overlays", "brand", "gamecontrollerdb.txt"}:
                continue
            safe_path(name)
            if stat.S_ISDIR(info.st_mode):
                require(name not in REQUIRED | {"assets/gamecontrollerdb.txt"}, "Expected a file, found directory: " + name)
                visit(child, name + "/")
            else:
                require(stat.S_ISREG(info.st_mode) and allowed(name), "Forbidden package entry: " + name)
                files.append((name, child, info.st_size))
                require(len(files) <= MAX_FILES, "Too many package files")

    visit(bundle)
    files.sort()
    names = {name for name, _, _ in files}
    require(REQUIRED <= names, "Bundle is missing cartridge, game-library.py or registry.json")
    require(any(n.startswith("assets/fonts/") and n.endswith(".ttf") for n in names), "Bundle is missing a .ttf font")
    require(sum(size for _, _, size in files) <= MAX_UNPACKED, "Unpacked package exceeds 384 MiB")
    return files


def check_build_version(version, build_version=None, cargo_manifest=None):
    if build_version is None:
        manifest = cargo_manifest or ROOT / "Cargo.toml"
        text = manifest.read_text(encoding="utf-8")
        section = re.search(r"(?ms)^\[workspace\.package\]\s*$(.*?)(?=^\[|\Z)", text)
        match = re.search(r'^version\s*=\s*"([^"]+)"', section[1], re.M) if section else None
        require(match is not None, "Cannot determine Cargo version; supply --build-version from build provenance")
        build_version = match[1]
    require(semver(build_version) == version, "Build version does not match requested version")


class HashingReader:
    def __init__(self, stream):
        self.stream = stream
        self.digest = hashlib.sha256()

    def read(self, size):
        data = self.stream.read(size)
        self.digest.update(data)
        return data


def sha256_file(path):
    digest = hashlib.sha256()
    with regular_open(path) as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def archive_name(version):
    return "cartridgeos-r36s-plus-" + version + ".tar.gz"


def archive_url(version):
    return "https://github.com/Strizzo/Cartridge/releases/download/v" + version + "/" + archive_name(version)


def write_archive(path, files):
    manifest = []
    with path.open("xb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=9) as zipped:
            with tarfile.open(fileobj=zipped, mode="w|", format=tarfile.USTAR_FORMAT,
                              encoding="ascii") as tar:
                for name, source, size in files:
                    with regular_open(source) as stream:
                        before = os.fstat(stream.fileno())
                        require(before.st_size == size, "Source size changed during packaging")
                        if name == "cartridge":
                            check_elf(stream.read(64))
                            stream.seek(0)
                        entry = tarfile.TarInfo(name)
                        entry.size = size
                        entry.mode = 0o755 if name == "cartridge" else 0o644
                        reader = HashingReader(stream)
                        tar.addfile(entry, reader)
                        after = os.fstat(stream.fileno())
                        require((before.st_size, before.st_mtime_ns, before.st_ctime_ns)
                                == (after.st_size, after.st_mtime_ns, after.st_ctime_ns),
                                "Source changed during packaging")
                        manifest.append({"path": name, "size": size, "sha256": reader.digest.hexdigest()})
        raw.flush()
        os.fsync(raw.fileno())
    require(path.stat().st_size <= MAX_ARCHIVE, "Archive exceeds 128 MiB")
    return manifest


def exact_fields(value, keys):
    require(isinstance(value, dict) and set(value) == set(keys.split()), "Invalid manifest fields")


def integer(value, maximum, minimum=0):
    require(type(value) is int and minimum <= value <= maximum, "Invalid size or integer field")


def digest(value):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value), "Invalid SHA-256")


def validate_payload(payload):
    exact_fields(payload, "schema channel version revision target min_runtime min_supervisor notes archive files")
    require(type(payload["schema"]) is int and payload["schema"] == 1, "Unsupported schema")
    require(payload["channel"] == "stable" and payload["target"] == TARGET, "Unsupported channel or target")
    release_id(payload["version"], payload["revision"])
    require(payload["min_runtime"] == MIN_RUNTIME and type(payload["min_supervisor"]) is int
            and payload["min_supervisor"] == 1, "Unsupported runtime or supervisor requirement")
    require(isinstance(payload["notes"], str) and len(payload["notes"].encode("utf-8")) <= 8000,
            "Release notes exceed 8000 UTF-8 bytes")
    archive = payload["archive"]
    exact_fields(archive, "url size sha256 unpacked_size")
    require(archive["url"] == archive_url(payload["version"]), "Archive URL must be the public versioned GitHub release URL")
    integer(archive["size"], MAX_ARCHIVE, 1)
    integer(archive["unpacked_size"], MAX_UNPACKED, 1)
    digest(archive["sha256"])
    require(isinstance(payload["files"], list) and 0 < len(payload["files"]) <= MAX_FILES, "Invalid file count")
    names = []
    total = 0
    for entry in payload["files"]:
        exact_fields(entry, "path size sha256")
        name = safe_path(entry["path"])
        require(allowed(name), "Forbidden payload path: " + name)
        integer(entry["size"], MAX_UNPACKED)
        digest(entry["sha256"])
        names.append(name)
        total += entry["size"]
    require(names == sorted(set(names)), "File paths must be unique and sorted")
    name_set = set(names)
    require(all("/".join(n.split("/")[:i]) not in name_set
                for n in names for i in range(1, len(n.split("/")))), "File/directory path collision")
    require(REQUIRED <= name_set and any(n.startswith("assets/fonts/") and n.endswith(".ttf") for n in names),
            "Required runtime files are missing")
    require(total == archive["unpacked_size"] <= MAX_UNPACKED, "Unpacked size mismatch")


def verify_archive(path, payload):
    """Stream USTAR ourselves so tarfile cannot silently consume PAX/GNU headers."""
    validate_payload(payload)
    require(path.stat().st_size == payload["archive"]["size"], "Archive size mismatch")
    require(sha256_file(path) == payload["archive"]["sha256"], "Archive SHA-256 mismatch")
    with regular_open(path) as raw, gzip.GzipFile(fileobj=raw, mode="rb") as stream:
        for expected in payload["files"]:
            header = stream.read(512)
            require(len(header) == 512 and header[257:265] == b"ustar\x0000", "Archive must use USTAR headers")
            entry = tarfile.TarInfo.frombuf(header, "ascii", "strict")
            require(entry.type == tarfile.REGTYPE and not entry.linkname, "Only regular USTAR files are permitted")
            require(entry.name == expected["path"] and entry.size == expected["size"], "Archive file list mismatch")
            require(entry.uid == entry.gid == entry.mtime == 0 and entry.uname == entry.gname == ""
                    and entry.mode == (0o755 if entry.name == "cartridge" else 0o644), "Noncanonical archive metadata")
            remaining = entry.size
            checksum = hashlib.sha256()
            first = b""
            while remaining:
                block = stream.read(min(remaining, 1024 * 1024))
                require(block, "Truncated archive file")
                if not first:
                    first = block[:64]
                checksum.update(block)
                remaining -= len(block)
            if entry.name == "cartridge":
                check_elf(first)
            require(checksum.hexdigest() == expected["sha256"], "Per-file SHA-256 mismatch")
            padding = (-entry.size) % 512
            require(stream.read(padding) == b"\0" * padding, "Invalid USTAR file padding")
        # Two end blocks can cross a 10 KiB record boundary. At the final
        # 512-byte slot, tarfile emits 1024 + 9728 = 10752 zero bytes.
        max_tail = 1024 + tarfile.RECORDSIZE - 512
        tail = stream.read(max_tail + 1)
        require(1024 <= len(tail) <= max_tail and len(tail) % 512 == 0
                and not tail.strip(b"\0"), "Extra entries or invalid USTAR end padding")


def strict_json(text):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, "Duplicate JSON field")
            result[key] = value
        return result
    def reject_constant(_value):
        raise ValueError("Nonfinite JSON value")
    return json.loads(text, object_pairs_hook=pairs, parse_constant=reject_constant)


def openssl(arguments):
    executable = os.environ.get("OPENSSL", "openssl")
    version = subprocess.run([executable, "version"], check=True, capture_output=True).stdout
    require(version.startswith(b"OpenSSL 3."), "OpenSSL 3 is required; set OPENSSL to its executable")
    result = subprocess.run([executable] + arguments, capture_output=True)
    require(result.returncode == 0, "OpenSSL signing/verification failed (diagnostics suppressed)")
    return result.stdout


def signature_operation(payload_text, key, signature=None):
    with tempfile.TemporaryDirectory(prefix="cartridge-signature-") as tmp:
        path = Path(tmp)
        (path / "payload").write_bytes(payload_text.encode("utf-8"))
        arguments = ["pkeyutl", "-rawin", "-inkey", str(key), "-in", str(path / "payload")]
        if signature is None:
            result = openssl(arguments + ["-sign"])
            require(len(result) == 64, "Expected a 64-byte Ed25519 signature")
            return result.hex()
        require(isinstance(signature, str) and re.fullmatch(r"[0-9a-fA-F]{128}", signature), "Invalid Ed25519 signature")
        (path / "signature").write_bytes(bytes.fromhex(signature))
        openssl(arguments + ["-verify", "-pubin", "-sigfile", str(path / "signature")])


def read_metadata(path):
    with regular_open(path) as stream:
        data = stream.read(MAX_MANIFEST + 1)
    require(len(data) <= MAX_MANIFEST, "Manifest exceeds size limit")
    return data.decode("utf-8")


def verify_package(manifest, public_key):
    text = read_metadata(manifest)
    value = strict_json(text)
    signed = isinstance(value, dict) and "payload" in value
    if signed:
        exact_fields(value, "key_id payload signature")
        require(value["key_id"] == KEY_ID and isinstance(value["payload"], str), "Invalid signed envelope")
        text = value["payload"]
        signature_operation(text, public_key, value["signature"])
        value = strict_json(text)
    else:
        require(manifest.name.endswith(".unsigned.json"), "Unsigned payload must be named *.unsigned.json")
    validate_payload(value)
    verify_archive(manifest.parent / archive_name(value["version"]), value)
    return text, signed


def atomic_write(path, data):
    require(not path.exists() and not path.is_symlink(), "Output already exists: " + str(path))
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        try:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)
    sync_directory(path.parent)


def sync_directory(path):
    fd = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def sign_payload(text, key, output, public_key=None):
    signature = signature_operation(text, key)
    signature_operation(text, public_key or ROOT / "scripts/system-update-public-key.pem", signature)
    envelope = {"key_id": KEY_ID, "payload": text, "signature": signature}
    atomic_write(output / "system-update.json", (json.dumps(envelope, ensure_ascii=True, separators=(",", ":")) + "\n").encode())


def build(bundle, output, version, revision, notes="", build_version=None, cargo_manifest=None, sign_key=None, public_key=None):
    release_id(version, revision)
    check_build_version(version, build_version, cargo_manifest)
    require(isinstance(notes, str) and len(notes.encode("utf-8")) <= 8000, "Release notes exceed 8000 UTF-8 bytes")
    require(bundle.resolve() not in (output.resolve(), *output.resolve().parents), "Output must be outside the bundle")
    require(not output.is_symlink(), "Output must not be a symlink")
    files = collect(bundle)
    output.mkdir(parents=True, exist_ok=True)
    for name in (archive_name(version), "system-update.unsigned.json", "system-update.json"):
        require(not (output / name).exists() and not (output / name).is_symlink(), "Output already exists: " + name)
    with tempfile.TemporaryDirectory(prefix=".package-", dir=output) as tmp:
        archive = Path(tmp) / archive_name(version)
        entries = write_archive(archive, files)
        payload = {"schema": 1, "channel": "stable", "version": version, "revision": revision,
                   "target": TARGET, "min_runtime": MIN_RUNTIME, "min_supervisor": 1, "notes": notes,
                   "archive": {"url": archive_url(version), "size": archive.stat().st_size,
                               "sha256": sha256_file(archive), "unpacked_size": sum(f["size"] for f in entries)},
                   "files": entries}
        verify_archive(archive, payload)
        text = json.dumps(payload, ensure_ascii=True, sort_keys=True, separators=(",", ":"))
        require(len(text.encode()) <= MAX_MANIFEST, "Manifest exceeds size limit")
        os.replace(archive, output / archive.name)
        atomic_write(output / "system-update.unsigned.json", text.encode("utf-8"))
        if sign_key:
            sign_payload(text, sign_key, output, public_key)
    return payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, help="Device bundle's Cartridge directory")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--version")
    parser.add_argument("--revision", help="40-character lowercase Git revision")
    parser.add_argument("--notes-file", type=Path)
    provenance = parser.add_mutually_exclusive_group()
    provenance.add_argument("--build-version", help="Version from trusted binary build provenance (never executes ARM code)")
    provenance.add_argument("--cargo-manifest", type=Path, help="Defaults to this checkout's Cargo.toml")
    parser.add_argument("--sign-key", type=Path, help="Local Ed25519 private key passed directly to OpenSSL; never used by CI")
    parser.add_argument("--verify", type=Path, metavar="MANIFEST", help="Verify the neighboring archive; signing an unsigned artifact is optional")
    parser.add_argument("--public-key", type=Path, default=ROOT / "scripts/system-update-public-key.pem")
    args = parser.parse_args()
    try:
        if args.verify:
            require(not any((args.bundle, args.output, args.version, args.revision, args.notes_file,
                             args.build_version, args.cargo_manifest)), "Build options cannot be combined with --verify")
            text, signed = verify_package(args.verify, args.public_key)
            if args.sign_key:
                require(not signed, "Artifact is already signed")
                sign_payload(text, args.sign_key, args.verify.parent, args.public_key)
                print("Verified archive and wrote local signed system-update.json; nothing published.")
            else:
                print("Verified signed package." if signed else "Verified unsigned package integrity; NOT an authenticated release.")
        else:
            require(all((args.bundle, args.output, args.version, args.revision)), "Build requires --bundle, --output, --version and --revision")
            notes = read_metadata(args.notes_file) if args.notes_file else ""
            build(args.bundle, args.output, args.version, args.revision, notes,
                  args.build_version, args.cargo_manifest, args.sign_key, args.public_key)
            print("Wrote local OTA artifacts; " + ("signed envelope included." if args.sign_key else "payload is UNSIGNED and cannot authorize an update."))
    except (OSError, ValueError, UnicodeError, tarfile.TarError, EOFError, subprocess.SubprocessError) as exc:
        parser.exit(1, "Packaging failed: " + str(exc) + "\n")


if __name__ == "__main__":
    main()
