#!/usr/bin/env python3
"""Package a prepared browser tree; no publication, OS changes or engine execution.

A separate component schema uses the existing exact-payload Ed25519 envelope.
Unsigned builds remain *.unsigned.json. Sources/notices are retained; this tool
DOES NOT assert that corresponding-source distribution has been provided.
"""
import argparse
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import sys
import tarfile
import tempfile

_spec = importlib.util.spec_from_file_location("component_ota_helpers", Path(__file__).with_name("package_system_update.py"))
ota = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ota)
ROOT = Path(__file__).resolve().parents[1]
KEY_ID = "cartridge-os-v1"
TARGET = "r36s-plus-aarch64"
SOURCE_STATUS = "metadata-only; corresponding-source distribution not asserted"
MANIFEST = "browser-component.json"
RECEIPT = ".browser-release.json"
MAX_MANIFEST = 32 * 1024 * 1024
MAX_ENTRIES = 65536
MAX_ARCHIVE = MAX_STORED = 4 * 1024**3
MAX_LOGICAL = 8 * 1024**3
require = ota.require


def safe_path(path):
    require(isinstance(path, str) and 0 < len(path) <= 255 and path.isascii()
            and all(32 <= ord(c) < 127 and c != "\\" for c in path), "Unsafe component path")
    parts = path.split("/")
    require(len(parts) <= 32 and all(p not in ("", ".", "..") and len(p) <= 100 for p in parts), "Unsafe component path")
    tarfile.TarInfo(path).tobuf(format=tarfile.USTAR_FORMAT, encoding="ascii")
    return path


def https_url(url):
    require(isinstance(url, str) and url.startswith("https://") and len(url) <= 4096
            and url.isascii() and all(32 < ord(c) < 127 for c in url)
            and not any(c in url for c in "@#\\"), "Component URL must be HTTPS without credentials or fragment")
    host = url[8:].split("/")[0]
    require(host and not host.startswith(":"), "Missing HTTPS host")


def resolve_link(path, entries):
    todo = path.split("/")
    resolved = []
    hops = 0
    while todo:
        part, *todo = todo
        if part in ("", "."):
            continue
        if part == "..":
            require(resolved, "Symbolic link escapes component")
            resolved.pop()
        else:
            resolved.append(part)
            found = entries.get("/".join(resolved), {})
            if found.get("type") == "symlink":
                hops += 1
                require(hops <= 40, "Symbolic link cycle or excessive chain")
                resolved.pop()
                todo = found["target"].split("/") + todo


def file_record(entry, entries):
    return entries[entry["target"]] if entry["type"] == "hardlink" else entry


def validate_payload(value):
    ota.exact_fields(value, "schema component target version revision min_runtime chromium_version source_distribution provenance archive entries")
    require(type(value["schema"]) is int and value["schema"] == 1
            and value["component"] == "browser-engine" and value["target"] == TARGET, "Unsupported component")
    ota.release_id(value["version"], value["revision"])
    ota.semver(value["min_runtime"])
    require(isinstance(value["chromium_version"], str) and 0 < len(value["chromium_version"].encode()) <= 128
            and value["source_distribution"] == SOURCE_STATUS, "Invalid source/build metadata")
    archive = value["archive"]
    ota.exact_fields(archive, "url size sha256 stored_bytes logical_bytes")
    https_url(archive["url"])
    ota.integer(archive["size"], MAX_ARCHIVE, 1)
    ota.integer(archive["stored_bytes"], MAX_STORED)
    ota.integer(archive["logical_bytes"], MAX_LOGICAL)
    ota.digest(archive["sha256"])
    require(isinstance(value["entries"], list) and 0 < len(value["entries"]) <= MAX_ENTRIES, "Invalid entry count")
    entries = {}
    stored = logical = 0
    last = ""
    for entry in value["entries"]:
        require(isinstance(entry, dict), "Invalid entry")
        kind = entry.get("type")
        fields = {"directory": "path type", "file": "path type size sha256 mode",
                  "symlink": "path type target", "hardlink": "path type target"}
        require(kind in fields, "Special or unsupported component entry")
        ota.exact_fields(entry, fields[kind])
        path = safe_path(entry["path"])
        require(path > last and path != RECEIPT, "Entries must be unique, sorted and not installer metadata")
        last = path
        if "/" in path:
            require(entries.get(path.rsplit("/", 1)[0], {}).get("type") == "directory", "Missing or non-directory parent")
        if kind == "file":
            ota.integer(entry["size"], MAX_STORED)
            ota.digest(entry["sha256"])
            require(type(entry["mode"]) is int and entry["mode"] in (0o444, 0o555), "File must be read-only/unprivileged")
            stored += entry["size"]
            logical += entry["size"]
        elif kind == "hardlink":
            safe_path(entry["target"])
            require(len(entry["target"]) <= 100 and entries.get(entry["target"], {}).get("type") == "file", "Hardlink must reference earlier regular file")
            logical += entries[entry["target"]]["size"]
        elif kind == "symlink":
            target = entry["target"]
            require(isinstance(target, str) and 0 < len(target) <= 100 and target.isascii()
                    and all(32 <= ord(c) < 127 and c != "\\" for c in target)
                    and not target.startswith("/"), "Unsafe symbolic link")
        entries[path] = entry
    require(stored == archive["stored_bytes"] and logical == archive["logical_bytes"], "Size accounting mismatch")
    for path, entry in entries.items():
        if entry["type"] == "symlink":
            resolve_link(path, entries)
    executables = {"chromium", "bubblewrap", "rootfs/usr/lib/chromium/chromium"}
    required = executables | {"component.json", "packages.tsv", "notices/bubblewrap.txt",
        "notices/static-libc.txt", "notices/static-libcap.txt", "notices/sources.txt"}
    for path in required:
        require(path in entries, "Required browser/notice file missing: " + path)
        entry = file_record(entries[path], entries)
        require(entry["type"] == "file" and entry["size"] > 0 and (path not in executables or entry["mode"] == 0o555),
                "Required file must be regular/executable: " + path)
    require(any(p.startswith("rootfs/usr/share/doc/") and p.endswith("/copyright") for p in entries), "Debian copyright notices required")

    provenance = value["provenance"]
    ota.exact_fields(provenance, "package_list build_inventory sources")
    for field, path in (("package_list", "packages.tsv"), ("build_inventory", "component.json")):
        ref = provenance[field]
        ota.exact_fields(ref, "path size sha256")
        entry = file_record(entries[path], entries)
        require(ref == dict(path=path, size=entry["size"], sha256=entry["sha256"]), "Provenance reference differs from signed file")
    require(isinstance(provenance["sources"], list) and len(provenance["sources"]) <= 4096, "Too many source references")
    urls = set()
    for source in provenance["sources"]:
        ota.exact_fields(source, "role url size sha256")
        require(source["role"] in ("index", "archive") and source["url"] not in urls, "Invalid/duplicate source artifact")
        https_url(source["url"])
        ota.integer(source["size"], 1024**4, 1)
        ota.digest(source["sha256"])
        urls.add(source["url"])


def collect(component):
    require(component.is_dir() and not component.is_symlink(), "Component must be a real directory")
    require(not (component / RECEIPT).exists(), "Package the prepared build tree, not an installed release")
    inventory = ota.strict_json((component / "component.json").read_text())
    require(inventory.get("schema") == 1 and inventory.get("target") == TARGET, "Expected prepare_component.py inventory")
    source_files = {}
    for entry in inventory["files"]:
        path = safe_path(entry["path"])
        require(path not in source_files and path != "component.json", "Duplicate inventory path")
        source_files[path] = entry
    paths = []
    for directory, dirs, files in os.walk(component, followlinks=False):
        paths.extend(Path(directory) / name for name in dirs + files)
    paths.sort(key=lambda p: p.relative_to(component).as_posix())
    require(len(paths) <= MAX_ENTRIES, "Too many component entries")
    entries = []
    inodes = {}
    inventoried = set()
    for path in paths:
        name = safe_path(path.relative_to(component).as_posix())
        info = path.lstat()
        if stat.S_ISDIR(info.st_mode):
            entries.append(dict(path=name, type="directory"))
            continue
        prior = source_files.get(name)
        require(prior is not None or name == "component.json", "Unlisted prepared file: " + name)
        if prior is not None:
            inventoried.add(name)
        if stat.S_ISLNK(info.st_mode):
            target = os.readlink(path)
            require(prior == dict(path=name, type="symlink", target=target), "Prepared symlink changed: " + name)
            entries.append(dict(path=name, type="symlink", target=target))
        elif stat.S_ISREG(info.st_mode):
            require(not info.st_mode & 0o7000, "Privileged component file: " + name)
            checksum = ota.sha256_file(path)
            if prior is not None:
                require(prior == dict(path=name, type="file", size=info.st_size, sha256=checksum, mode=stat.S_IMODE(info.st_mode)),
                        "Prepared file hash/mode changed: " + name)
            inode = (info.st_dev, info.st_ino)
            if inode in inodes:
                entries.append(dict(path=name, type="hardlink", target=inodes[inode]))
            else:
                inodes[inode] = name
                entries.append(dict(path=name, type="file", size=info.st_size, sha256=checksum,
                                    mode=0o555 if info.st_mode & 0o111 else 0o444))
        else:
            raise ValueError("Special component file: " + name)
    require(inventoried == set(source_files), "Prepared inventory is incomplete")
    return inventory, entries


def archive_name(version):
    return "browser-engine-r36s-plus-" + ota.semver(version) + ".tar.gz"


def write_archive(component, archive, entries):
    records = {e["path"]: e for e in entries}
    with archive.open("xb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0, compresslevel=6) as zipped:
            with tarfile.open(fileobj=zipped, mode="w|", format=tarfile.USTAR_FORMAT, encoding="ascii") as tar:
                for entry in entries:
                    header = tarfile.TarInfo(entry["path"])
                    kind = entry["type"]
                    if kind == "directory":
                        header.type, header.mode = tarfile.DIRTYPE, 0o555
                    elif kind in ("hardlink", "symlink"):
                        header.type = tarfile.LNKTYPE if kind == "hardlink" else tarfile.SYMTYPE
                        header.linkname = entry["target"]
                        header.mode = records[entry["target"]]["mode"] if kind == "hardlink" else 0o777
                    else:
                        header.size, header.mode = entry["size"], entry["mode"]
                    if kind == "file":
                        with ota.regular_open(component / entry["path"]) as source:
                            reader = ota.HashingReader(source)
                            tar.addfile(header, reader)
                            require(reader.digest.hexdigest() == entry["sha256"], "Source changed during packaging")
                    else:
                        tar.addfile(header)
        raw.flush()
        os.fsync(raw.fileno())


def verify_archive(archive, payload):
    validate_payload(payload)
    require(archive.stat().st_size == payload["archive"]["size"] and ota.sha256_file(archive) == payload["archive"]["sha256"], "Archive hash/size mismatch")
    records = {e["path"]: e for e in payload["entries"]}
    with ota.regular_open(archive) as raw, gzip.GzipFile(fileobj=raw, mode="rb") as stream:
        for expected in payload["entries"]:
            block = stream.read(512)
            require(len(block) == 512 and block[257:265] == b"ustar\x0000", "Expected USTAR header")
            entry = tarfile.TarInfo.frombuf(block, "ascii", "strict")
            kind = expected["type"]
            require(entry.name == expected["path"] and entry.type == {"file": tarfile.REGTYPE,
                "directory": tarfile.DIRTYPE, "symlink": tarfile.SYMTYPE, "hardlink": tarfile.LNKTYPE}[kind], "Archive path/type mismatch")
            mode = {"directory": 0o555, "symlink": 0o777}.get(kind)
            if mode is None:
                mode = file_record(expected, records)["mode"]
            require(entry.mode == mode and entry.uid == entry.gid == entry.mtime == 0
                    and entry.uname == entry.gname == "" and entry.devmajor == entry.devminor == 0
                    and entry.linkname == expected.get("target", "") and entry.size == expected.get("size", 0), "Noncanonical archive metadata")
            checksum = hashlib.sha256()
            remaining = entry.size
            while remaining:
                data = stream.read(min(remaining, 1024 * 1024))
                require(data, "Truncated component file")
                checksum.update(data)
                remaining -= len(data)
            if kind == "file":
                require(checksum.hexdigest() == expected["sha256"], "Per-file SHA-256 mismatch")
            pad = (-entry.size) % 512
            require(stream.read(pad) == b"\0" * pad, "Invalid file padding")
        tail = stream.read(10753)
        require(1024 <= len(tail) <= 10752 and len(tail) % 512 == 0 and not tail.strip(b"\0"), "Extra entries or invalid USTAR trailer")


def read_manifest(path, public_key):
    with ota.regular_open(path) as source:
        body = source.read(MAX_MANIFEST + 1)
    require(len(body) <= MAX_MANIFEST, "Component manifest exceeds 32 MiB")
    value = ota.strict_json(body.decode())
    if isinstance(value, dict) and "payload" in value:
        ota.exact_fields(value, "key_id payload signature")
        require(value["key_id"] == KEY_ID and isinstance(value["payload"], str), "Unknown component signing key")
        ota.signature_operation(value["payload"], public_key, value["signature"])
        text, signed = value["payload"], True
        value = ota.strict_json(text)
    else:
        require(path.name.endswith(".unsigned.json"), "Unsigned payload must be named *.unsigned.json")
        text, signed = body.decode(), False
    validate_payload(value)
    return text, value, signed


def build(component, output, version, revision, url, sign_key=None, public_key=None, sources=None):
    ota.release_id(version, revision)
    https_url(url)
    component = component.absolute()
    require(component.resolve() not in (output.resolve(), *output.resolve().parents) and not output.is_symlink(), "Output must be outside component")
    inventory, entries = collect(component)
    stored = sum(e.get("size", 0) for e in entries)
    records = {e["path"]: e for e in entries}
    logical = sum(file_record(e, records).get("size", 0) for e in entries if e["type"] in ("file", "hardlink"))
    output.mkdir(parents=True, exist_ok=True)
    name = archive_name(version)
    for filename in (name, MANIFEST, "browser-component.unsigned.json"):
        require(not os.path.lexists(output / filename), "Output already exists: " + filename)
    with tempfile.TemporaryDirectory(prefix=".browser-package-", dir=output) as temporary:
        archive = Path(temporary) / name
        write_archive(component, archive, entries)
        payload = dict(schema=1, component="browser-engine", target=TARGET, version=version,
            revision=revision, min_runtime=inventory["min_runtime"], chromium_version=inventory["chromium_version"],
            source_distribution=SOURCE_STATUS,
            provenance=dict(package_list=dict(path="packages.tsv", size=file_record(records["packages.tsv"], records)["size"], sha256=file_record(records["packages.tsv"], records)["sha256"]),
                build_inventory=dict(path="component.json", size=records["component.json"]["size"], sha256=records["component.json"]["sha256"]),
                sources=[] if sources is None else sources), archive=dict(url=url, size=archive.stat().st_size,
                sha256=ota.sha256_file(archive), stored_bytes=stored, logical_bytes=logical), entries=entries)
        validate_payload(payload)
        verify_archive(archive, payload)
        text = json.dumps(payload, ensure_ascii=True, separators=(",", ":")) + "\n"
        require(len(text.encode()) <= MAX_MANIFEST // 2, "Payload too large for signed envelope")
        os.rename(archive, output / name)
        ota.sync_directory(output)
        ota.atomic_write(output / "browser-component.unsigned.json", text.encode())
        if sign_key is not None:
            sign(text, sign_key, output, public_key)
    return payload


def sign(text, key, output, public_key=None):
    validate_payload(ota.strict_json(text))
    signature = ota.signature_operation(text, key)
    ota.signature_operation(text, public_key or ROOT / "scripts/system-update-public-key.pem", signature)
    envelope = dict(key_id=KEY_ID, payload=text, signature=signature)
    body = (json.dumps(envelope, ensure_ascii=True, separators=(",", ":")) + "\n").encode()
    require(len(body) <= MAX_MANIFEST, "Signed manifest too large")
    ota.atomic_write(output / MANIFEST, body)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--component", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--version")
    parser.add_argument("--revision")
    parser.add_argument("--url", help="Signed HTTPS archive URL (local installation still requires this metadata)")
    parser.add_argument("--sources-json", type=Path, help="Optional JSON array of source index/archive references: role,url,size,sha256; no availability claim")
    parser.add_argument("--sign-key", type=Path, help="Optional OpenSSL 3 Ed25519 private key; Python never reads it")
    parser.add_argument("--public-key", type=Path, default=ROOT / "scripts/system-update-public-key.pem", help="Packaging verification key; device CLI always pins the production key")
    parser.add_argument("--verify", type=Path, help="Verify signed artifact, or explicitly named unsigned build")
    parser.add_argument("--archive", type=Path, help="Archive for --verify; default: sibling versioned tar.gz")
    parser.add_argument("--sign", type=Path, help="Sign an existing *.unsigned.json, after archive verification")
    args = parser.parse_args()
    if args.verify or args.sign:
        path = args.verify or args.sign
        text, payload, signed = read_manifest(path, args.public_key)
        verify_archive(args.archive or path.parent / archive_name(payload["version"]), payload)
        if args.sign:
            require(not signed and args.sign_key is not None, "--sign needs an unsigned build and --sign-key")
            sign(text, args.sign_key, args.output or path.parent, args.public_key)
            print("Signed", (args.output or path.parent) / MANIFEST)
        else:
            print("Verified signed component" if signed else "UNSIGNED build verified; NOT an authenticated release")
    else:
        require(all((args.component, args.output, args.version, args.revision, args.url)), "Build needs --component --output --version --revision --url")
        build(args.component, args.output, args.version, args.revision, args.url, args.sign_key, args.public_key, ota.strict_json(args.sources_json.read_text()) if args.sources_json else None)
        print("Signed component" if args.sign_key else "UNSIGNED build; not installable until signed", args.output)
    print("Source metadata retained; corresponding-source distribution is not asserted.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, tarfile.TarError, EOFError) as exc:
        sys.exit(str(exc))
