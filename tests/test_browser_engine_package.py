"""Small offline fixtures only; generated test keys never use production trust."""
import copy
import gzip
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("browser_package", ROOT / "scripts/package_browser_engine.py")
package = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(package)
PREPARE_SPEC = importlib.util.spec_from_file_location("browser_prepare", ROOT / "crates/cartridge-browser/engine/prepare_component.py")
prepare = importlib.util.module_from_spec(PREPARE_SPEC)
PREPARE_SPEC.loader.exec_module(prepare)


def make_component(root):
    root.mkdir()
    for name, body in {
        ".cartridge-browser-build": "fixture build", "chromium": "fixture wrapper", "bubblewrap": "fixture helper",
        "packages.tsv": "chromium\t1.2.3-1\n", "notices/bubblewrap.txt": "license",
        "notices/static-libc.txt": "libc license", "notices/static-libcap.txt": "libcap license",
        "notices/sources.txt": "metadata only, not a source distribution",
        "rootfs/usr/lib/chromium/chromium": "fixture engine", "rootfs/usr/share/doc/chromium/copyright": "copyright",
    }.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body)
        path.chmod(0o755 if name in ("chromium", "bubblewrap", "rootfs/usr/lib/chromium/chromium") else 0o644)
    (root / "rootfs/proc").mkdir()
    (root / "rootfs/link").symlink_to("usr/lib/chromium/chromium")
    os.link(root / "rootfs/usr/lib/chromium/chromium", root / "rootfs/usr/lib/chromium/copy")
    refresh_inventory(root)


def refresh_inventory(root):
    data = dict(schema=1, target=package.TARGET, min_runtime="0.8.0", chromium_version="1.2.3-1",
                files=prepare.inventory(root), hardware_validated=False)
    (root / "component.json").write_text(json.dumps(data) + "\n")


class PackagingTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="browser-package-test-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.component = self.root / "prepared"
        make_component(self.component)
        self.output = self.root / "output"

    def build(self, **kwargs):
        return package.build(self.component, self.output, "1.0.0", "a" * 40,
                             "https://example.org/browser.tar.gz", **kwargs)

    def test_hardlinks_notices_empty_directories_and_source_provenance_retained(self):
        sources = [dict(role="index", url="https://example.org/sources/index.json", size=123, sha256="a" * 64),
                   dict(role="archive", url="https://example.org/sources/bwrap.tar.xz", size=456, sha256="b" * 64)]
        payload = self.build(sources=sources)
        package.verify_archive(self.output / package.archive_name("1.0.0"), payload)
        entries = {e["path"]: e for e in payload["entries"]}
        self.assertEqual(entries["rootfs/usr/lib/chromium/copy"]["type"], "hardlink")
        self.assertEqual(entries["rootfs/proc"]["type"], "directory")
        self.assertEqual(payload["archive"]["logical_bytes"] - payload["archive"]["stored_bytes"], len("fixture engine"))
        self.assertEqual(payload["provenance"]["sources"], sources)
        self.assertIn("not asserted", payload["source_distribution"])
        self.assertEqual(payload["provenance"]["package_list"]["sha256"], hashlib.sha256((self.component / "packages.tsv").read_bytes()).hexdigest())
        self.assertFalse((self.output / package.MANIFEST).exists())

    def test_prepared_inventory_hash_or_entry_changes_rejected(self):
        for attack in ("extra", "hash", "mode", "symlink", "special"):
            with self.subTest(attack=attack):
                root = self.root / attack
                make_component(root)
                if attack == "extra":
                    (root / "unexpected").write_text("extra")
                elif attack == "hash":
                    (root / "chromium").write_text("changed")
                elif attack == "mode":
                    (root / "chromium").chmod(0o700)
                elif attack == "symlink":
                    (root / "rootfs/link").unlink()
                    (root / "rootfs/link").symlink_to("/etc/passwd")
                else:
                    os.mkfifo(root / "fifo")
                with self.assertRaises(ValueError):
                    package.collect(root)

    def test_link_escapes_and_symlink_parent_aliases_fail_even_if_inventoried(self):
        for target in ("/etc/passwd", "../../escape", "link", "usr/../../../escape"):
            (self.component / "rootfs/link").unlink()
            (self.component / "rootfs/link").symlink_to(target)
            refresh_inventory(self.component)
            with self.subTest(target=target), self.assertRaises(ValueError):
                self.build()

    def test_missing_notices_wrong_target_and_provenance_tampering_fail(self):
        payload = self.build()
        for change in ("notice", "target", "provenance", "source-url", "duplicate"):
            value = copy.deepcopy(payload)
            if change == "notice":
                value["entries"] = [e for e in value["entries"] if e["path"] != "notices/sources.txt"]
            elif change == "target":
                value["target"] = "other"
            elif change == "provenance":
                value["provenance"]["package_list"]["sha256"] = "0" * 64
            elif change == "source-url":
                value["provenance"]["sources"] = [dict(role="index", url="http://example.org/a", size=1, sha256="a" * 64)]
            else:
                value["entries"].append(value["entries"][0])
            with self.subTest(change=change), self.assertRaises(ValueError):
                package.validate_payload(value)

    def test_malicious_archive_headers_hashes_and_trailers_rejected(self):
        payload = self.build()
        archive = self.output / package.archive_name("1.0.0")
        raw = gzip.decompress(archive.read_bytes())
        for attack in ("file-hash", "absolute", "parent", "symlink", "hardlink", "fifo", "pax", "duplicate", "trailer"):
            body = bytearray(raw)
            if attack == "file-hash":
                body[512] ^= 1
            elif attack in ("absolute", "parent"):
                name = b"/etc/passwd" if attack == "absolute" else b"../escape"
                body[:100] = name.ljust(100, b"\0")
            elif attack in ("symlink", "hardlink", "fifo", "pax"):
                body[156] = {"symlink": ord('2'), "hardlink": ord('1'), "fifo": ord('6'), "pax": ord('x')}[attack]
            elif attack == "duplicate":
                body = body[:1024] + body
            else:
                body += b"garbage"
            body[148:156] = b" " * 8
            checksum = sum(body[:512])
            body[148:156] = ("%06o\0 " % checksum).encode()
            changed = self.root / (attack + ".tar.gz")
            changed.write_bytes(gzip.compress(body, mtime=0))
            value = copy.deepcopy(payload)
            value["archive"].update(size=changed.stat().st_size, sha256=package.ota.sha256_file(changed))
            with self.subTest(attack=attack), self.assertRaises((ValueError, tarfile.TarError, gzip.BadGzipFile)):
                package.verify_archive(changed, value)

    def test_output_not_in_source_or_overwritten_and_unsigned_not_trusted(self):
        with self.assertRaises(ValueError):
            package.build(self.component, self.component / "out", "1.0.0", "a" * 40, "https://example.org/a")
        self.build()
        before = (self.output / "browser-component.unsigned.json").read_bytes()
        with self.assertRaises(ValueError):
            self.build()
        self.assertEqual((self.output / "browser-component.unsigned.json").read_bytes(), before)
        trusted_name = self.output / package.MANIFEST
        trusted_name.write_bytes(before)
        with self.assertRaisesRegex(ValueError, "Unsigned"):
            package.read_manifest(trusted_name, ROOT / "scripts/system-update-public-key.pem")

    def test_generated_test_key_signing_and_tampered_signature(self):
        openssl = os.environ.get("OPENSSL") or "/opt/homebrew/opt/openssl@3/bin/openssl"
        if not Path(openssl).exists():
            openssl = shutil.which("openssl")
        if not openssl or not subprocess.check_output([openssl, "version"]).startswith(b"OpenSSL 3."):
            self.skipTest("OpenSSL 3 unavailable")
        private, public = self.root / "test-private.pem", self.root / "test-public.pem"
        subprocess.run([openssl, "genpkey", "-algorithm", "ED25519", "-out", str(private)], check=True, capture_output=True)
        subprocess.run([openssl, "pkey", "-in", str(private), "-pubout", "-out", str(public)], check=True, capture_output=True)
        old = os.environ.get("OPENSSL")
        os.environ["OPENSSL"] = openssl
        self.addCleanup(lambda: os.environ.pop("OPENSSL", None) if old is None else os.environ.__setitem__("OPENSSL", old))
        self.build(sign_key=private, public_key=public)
        manifest = self.output / package.MANIFEST
        _, _, signed = package.read_manifest(manifest, public)
        self.assertTrue(signed)
        with self.assertRaises(ValueError):
            package.read_manifest(manifest, ROOT / "scripts/system-update-public-key.pem")
        envelope = json.loads(manifest.read_text())
        envelope["signature"] = "0" * 128
        manifest.write_text(json.dumps(envelope))
        with self.assertRaises(ValueError):
            package.read_manifest(manifest, public)

    def test_cli_build_verify_unsigned_fixture(self):
        command = [sys.executable, str(ROOT / "scripts/package_browser_engine.py")]
        built = subprocess.run(command + ["--component", str(self.component), "--output", str(self.output), "--version", "1.0.0", "--revision", "a" * 40,
            "--url", "https://example.org/a"], capture_output=True, text=True)
        self.assertEqual(built.returncode, 0, built.stderr)
        checked = subprocess.run(command + ["--verify", str(self.output / "browser-component.unsigned.json")], capture_output=True, text=True)
        self.assertEqual(checked.returncode, 0, checked.stderr)
        self.assertIn("NOT an authenticated release", checked.stdout)


if __name__ == "__main__":
    unittest.main()
