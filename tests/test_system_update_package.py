import copy
import gzip
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/package_system_update.py"
spec = importlib.util.spec_from_file_location("system_update_package", SCRIPT)
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
VERSION = "0.6.1"
REVISION = "abcdef0123456789" * 2 + "abcdef01"


def arm_elf():
    header = bytearray(64)
    header[:7] = b"\x7fELF\x02\x01\x01"
    struct.pack_into("<HHI", header, 16, 3, 183, 1)
    struct.pack_into("<H", header, 52, 64)
    return bytes(header) + b"test binary\n"


class SystemUpdatePackageTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.bundle = self.root / "bundle"
        self.bundle.mkdir()
        self.source = {
            "cartridge": arm_elf(), "game-library.py": b"# helper\n",
            "registry.json": b'{"apps":[]}\n', "assets/fonts/font.ttf": b"font",
            "assets/brand/mark.png": b"image", "assets/icons/home.png": b"icon",
            "assets/gamecontrollerdb.txt": b"controller mappings", "lua_cartridges/clock/main.lua": b"return {}\n",
        }
        for name, body in self.source.items():
            self.put(name, body)

    def put(self, name, body=b"must not ship"):
        path = self.bundle / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(body)
        return path

    def build(self, directory="output", **kwargs):
        output = self.root / directory
        payload = package.build(self.bundle, output, VERSION, REVISION,
                                build_version=VERSION, **kwargs)
        return output, payload

    def test_round_trip_and_manifest_hashes(self):
        output, payload = self.build(notes="Reviewed changes — Wi-Fi")
        self.assertEqual(payload["schema"], 1)
        self.assertEqual(payload["target"], "r36s-plus-aarch64")
        self.assertEqual(payload["min_runtime"], VERSION)
        self.assertEqual(payload["min_supervisor"], 1)
        self.assertEqual(payload["archive"]["url"],
                         "https://github.com/Strizzo/Cartridge/releases/download/v0.6.1/cartridgeos-r36s-plus-0.6.1.tar.gz")
        archive = output / package.archive_name(VERSION)
        self.assertEqual(payload["archive"]["size"], archive.stat().st_size)
        self.assertEqual(payload["archive"]["sha256"], hashlib.sha256(archive.read_bytes()).hexdigest())
        self.assertEqual(payload["archive"]["unpacked_size"], sum(map(len, self.source.values())))
        self.assertEqual(payload["files"], [
            {"path": name, "size": len(body), "sha256": hashlib.sha256(body).hexdigest()}
            for name, body in sorted(self.source.items())])
        text, signed = package.verify_package(output / "system-update.unsigned.json", None)
        self.assertEqual(json.loads(text), payload)
        self.assertFalse(signed)
        self.assertFalse((output / "system-update.json").exists())
        with tarfile.open(archive) as tar:
            self.assertEqual(tar.getnames(), sorted(self.source))
            for member in tar:
                self.assertTrue(member.isfile())
                self.assertEqual(member.uid, 0)
                self.assertEqual(member.gid, 0)
                self.assertEqual(member.mtime, 0)
                self.assertEqual(member.mode, 0o755 if member.name == "cartridge" else 0o644)
                self.assertEqual(tar.extractfile(member).read(), self.source[member.name])

    def test_reproducible_despite_source_mtime_mode_and_creation_order(self):
        first, _ = self.build("first")
        for name in reversed(list(self.source)):
            path = self.bundle / name
            path.unlink()
            path.write_bytes(self.source[name])
            os.utime(path, (1700000000, 1700000000))
            path.chmod(0o777)
        second, _ = self.build("second")
        for name in (package.archive_name(VERSION), "system-update.unsigned.json"):
            self.assertEqual((first / name).read_bytes(), (second / name).read_bytes())

    def test_bundle_exclusions_are_not_read_or_archived(self):
        excluded = [
            "cartridge-boot", "cartridge-session.py", "setup-primary.py", "autosetup.sh",
            "cartridge-boot.sh", "cartridge-boot.service", "tools/Setup.sh", "dev/build-revision",
            "boot/kernel", "session/state.json", "secrets/password", ".env", "private.pem",
            "assets/logo.bmp", "assets/boot_logo.png", "assets/secrets/token.json",
            "assets/fonts/credentials.json", "assets/arbitrary.txt", "assets/diagnostics/report", "assets/private.key",
            "assets/.ssh/id_ed25519", "lua_cartridges/clock/.env", "lua_cartridges/clock/run.log",
            "lua_cartridges/clock/__pycache__/main.pyc", "system-update.json", "system-release.json",
        ]
        for name in excluded:
            self.put(name)
        original = package.regular_open
        opened = []
        def track(path):
            opened.append(path)
            return original(path)
        with mock.patch.object(package, "regular_open", side_effect=track):
            _, payload = self.build()
        self.assertEqual({entry["path"] for entry in payload["files"]}, set(self.source))
        self.assertFalse({self.bundle / n for n in excluded} & set(opened))

    def test_rejects_unsafe_paths(self):
        for name in ("", ".", "..", "/cartridge", "../cartridge", "assets/../x", "assets//x",
                     "assets/./x", "assets\\x", "assets/a\0b", "assets/é.png", "assets/x\ny",
                     "assets/a:", "assets/a+b", "assets/x.", "assets/" + "x" * 101, "a/" * 12 + "z", "a" * 110 + "/" + "b" * 110):
            with self.subTest(name=name), self.assertRaises((ValueError, UnicodeError)):
                package.safe_path(name)

    def test_malformed_source_name_rejected(self):
        self.put("assets/fonts/bad\\name")
        with self.assertRaises(ValueError):
            self.build()

    def test_symlink_files_and_directories_rejected(self):
        for name, target in (("assets/fonts/link", "font.ttf"), ("assets/fonts/link", "../fonts"),
                             ("assets/fonts/link", "/no/such/path")):
            link = self.bundle / name
            link.symlink_to(target)
            with self.subTest(target=target), self.assertRaises(ValueError):
                self.build()
            link.unlink()
        link = self.root / "linked-bundle"
        link.symlink_to(self.bundle, target_is_directory=True)
        with self.assertRaises(ValueError):
            package.collect(link)

    def test_special_file_rejected(self):
        os.mkfifo(self.bundle / "assets/fonts/fifo")
        with self.assertRaises(ValueError):
            self.build()

    def test_missing_required_files(self):
        for name in ("cartridge", "game-library.py", "registry.json", "assets/fonts/font.ttf"):
            path = self.bundle / name
            path.unlink()
            with self.subTest(name=name), self.assertRaises(ValueError):
                package.collect(self.bundle)
            path.write_bytes(self.source[name])

    def test_wrong_architecture_or_malformed_elf(self):
        valid = arm_elf()
        for index, value in ((4, 1), (5, 2), (6, 0), (7, 9), (16, 1), (18, 62), (20, 0), (52, 0)):
            bad = bytearray(valid)
            bad[index] = value
            with self.subTest(index=index), self.assertRaises(ValueError):
                package.check_elf(bad)
        for data in (b"", b"\x7fELF", b"#!/bin/sh\n", b"\xcf\xfa\xed\xfe" + b"x" * 64):
            with self.assertRaises(ValueError):
                package.check_elf(data)
        self.put("cartridge", b"not an ELF")
        with self.assertRaises(ValueError):
            self.build()

    def test_build_provenance_must_match(self):
        cargo = self.root / "Cargo.toml"
        cargo.write_text('[workspace.package]\nversion = "0.6.1"\n[dependencies]\n')
        package.check_build_version(VERSION, cargo_manifest=cargo)
        package.check_build_version(VERSION, build_version=VERSION)
        with self.assertRaisesRegex(ValueError, "version"):
            package.check_build_version("0.6.2", cargo_manifest=cargo)
        with self.assertRaisesRegex(ValueError, "version"):
            package.check_build_version("0.6.2", build_version=VERSION)
        cargo.write_text("[workspace]\n")
        with self.assertRaises(ValueError):
            package.check_build_version(VERSION, cargo_manifest=cargo)

    def test_release_identity_validation(self):
        self.assertEqual(package.release_id(VERSION, REVISION), "0.6.1-abcdef012345")
        for version in ("v0.6.1", "01.6.1", "0.6", "0.6.1-01", "1.2.3-rc.1", "../0.6.1", "0.6.1+x", "0.6.1-" + "a" * 100, "18446744073709551616.0.0"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                package.release_id(version, REVISION)
        for revision in ("f" * 39, "z" * 40, "f" * 41, "../" + "f" * 37):
            with self.assertRaises(ValueError):
                package.release_id(VERSION, revision)

    def test_notes_file_count_and_size_limits(self):
        with self.assertRaises(ValueError):
            self.build(notes="é" * 4001)
        with mock.patch.object(package, "MAX_FILES", 2), self.assertRaises(ValueError):
            self.build()
        with mock.patch.object(package, "MAX_UNPACKED", 8), self.assertRaises(ValueError):
            self.build()
        with mock.patch.object(package, "MAX_ARCHIVE", 8), self.assertRaises(ValueError):
            self.build()

    def test_output_cannot_be_in_bundle_or_overwrite_artifacts(self):
        with self.assertRaises(ValueError):
            package.build(self.bundle, self.bundle / "out", VERSION, REVISION, build_version=VERSION)
        output, _ = self.build()
        before = (output / "system-update.unsigned.json").read_bytes()
        with self.assertRaises(ValueError):
            self.build()
        self.assertEqual((output / "system-update.unsigned.json").read_bytes(), before)

    def test_manifest_validation_rejects_tampering(self):
        _, payload = self.build()
        for key, value in (("schema", 2), ("schema", True), ("channel", "beta"), ("target", "x86_64"),
                           ("min_runtime", "0.5.0"), ("min_supervisor", True), ("notes", "x" * 8001)):
            changed = copy.deepcopy(payload)
            changed[key] = value
            with self.subTest(key=key, value=str(value)[:20]), self.assertRaises(ValueError):
                package.validate_payload(changed)
        for key, value in (("url", "http://github.com/Strizzo/Cartridge/file"),
                           ("url", payload["archive"]["url"].replace("Strizzo", "someone")),
                           ("size", package.MAX_ARCHIVE + 1), ("size", True),
                           ("unpacked_size", package.MAX_UNPACKED + 1), ("sha256", "g" * 64)):
            changed = copy.deepcopy(payload)
            changed["archive"][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                package.validate_payload(changed)
        for name in ("../escape", "cartridge-session.py", "assets/boot_logo.png", "system-release.json"):
            changed = copy.deepcopy(payload)
            changed["files"][0]["path"] = name
            with self.assertRaises(ValueError):
                package.validate_payload(changed)
        changed = copy.deepcopy(payload)
        changed["files"].append(changed["files"][0])
        with self.assertRaises(ValueError):
            package.validate_payload(changed)

    def test_archive_and_file_hash_corruption(self):
        output, payload = self.build()
        archive = output / package.archive_name(VERSION)
        changed = copy.deepcopy(payload)
        changed["files"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "Per-file"):
            package.verify_archive(archive, changed)
        archive.write_bytes(archive.read_bytes()[:-1] + b"x")
        with self.assertRaisesRegex(ValueError, "Archive SHA-256"):
            package.verify_archive(archive, payload)

    def test_links_pax_duplicates_and_extra_entries_in_archive_rejected(self):
        _, original = self.build()
        for mode in ("symlink", "hardlink", "pax", "duplicate", "extra"):
            output = io.BytesIO()
            with tarfile.open(fileobj=output, mode="w", format=tarfile.PAX_FORMAT if mode == "pax" else tarfile.USTAR_FORMAT) as tar:
                for i, (name, body) in enumerate(sorted(self.source.items())):
                    entry = tarfile.TarInfo(name)
                    entry.size = len(body)
                    entry.mode = 0o755 if name == "cartridge" else 0o644
                    if i == 0 and mode in ("symlink", "hardlink"):
                        entry.type = tarfile.SYMTYPE if mode == "symlink" else tarfile.LNKTYPE
                        entry.linkname = "/etc/passwd"
                    if i == 0 and mode == "pax":
                        entry.pax_headers = {"comment": "must reject extension"}
                    tar.addfile(entry, io.BytesIO(body))
                    if i == 0 and mode == "duplicate":
                        tar.addfile(entry, io.BytesIO(body))
                if mode == "extra":
                    tar.addfile(tarfile.TarInfo("system-release.json"))
            archive = self.root / (mode + ".gz")
            archive.write_bytes(gzip.compress(output.getvalue(), mtime=0))
            payload = copy.deepcopy(original)
            payload["archive"].update(size=archive.stat().st_size, sha256=package.sha256_file(archive))
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                package.verify_archive(archive, payload)

    def test_unsigned_cannot_masquerade_as_trusted_endpoint(self):
        output, _ = self.build()
        manifest = output / "system-update.json"
        manifest.write_bytes((output / "system-update.unsigned.json").read_bytes())
        with self.assertRaisesRegex(ValueError, "Unsigned payload"):
            package.verify_package(manifest, None)

    def test_duplicate_json_fields_rejected(self):
        with self.assertRaises(ValueError):
            package.strict_json('{"schema":1,"schema":2}')
        with self.assertRaises(ValueError):
            package.strict_json('{"size":NaN}')

    def test_signing_preserves_exact_payload_and_never_reads_key(self):
        output, _ = self.build()
        original = (output / "system-update.unsigned.json").read_text()
        # No real private keys are generated or read in the test suite.
        key = self.root / "nonexistent-private-key.pem"
        with mock.patch.object(package, "signature_operation", return_value="ab" * 64) as operation:
            package.sign_payload(original, key, output)
            operation.assert_has_calls([
                mock.call(original, key),
                mock.call(original, package.ROOT / "scripts/system-update-public-key.pem", "ab" * 64)])
        envelope = json.loads((output / "system-update.json").read_text())
        self.assertEqual(envelope, {"key_id": "cartridge-os-v1", "payload": original, "signature": "ab" * 64})
        with mock.patch.object(package, "signature_operation") as operation:
            text, signed = package.verify_package(output / "system-update.json", self.root / "public.pem")
            operation.assert_called_once_with(original, self.root / "public.pem", "ab" * 64)
        self.assertTrue(signed)
        self.assertEqual(text, original)

    def test_signature_failure_stops_before_archive_processing(self):
        output, _ = self.build()
        manifest = output / "system-update.json"
        manifest.write_text(json.dumps({"key_id": package.KEY_ID, "payload": "not JSON", "signature": "ab" * 64}))
        with mock.patch.object(package, "signature_operation", side_effect=ValueError("bad signature")), \
                mock.patch.object(package, "verify_archive") as archive, self.assertRaisesRegex(ValueError, "bad signature"):
            package.verify_package(manifest, self.root / "public.pem")
        archive.assert_not_called()

    def test_openssl_env_ed25519_length_and_suppressed_diagnostics(self):
        calls = []
        def run(command, **kwargs):
            calls.append(command)
            if command[-1] == "version":
                return subprocess.CompletedProcess(command, 0, b"OpenSSL 3.5.0\n")
            return subprocess.CompletedProcess(command, 0, b"s" * 64)
        with mock.patch.dict(os.environ, {"OPENSSL": "/test/openssl3"}), \
                mock.patch.object(package.subprocess, "run", side_effect=run):
            result = package.signature_operation("exact\nJSON", Path("unread-private.pem"))
        self.assertEqual(result, "73" * 64)
        self.assertEqual(calls[0], ["/test/openssl3", "version"])
        self.assertIn("unread-private.pem", calls[1])
        self.assertIn("-rawin", calls[1])
        self.assertIn("-sign", calls[1])
        with mock.patch.object(package, "openssl", return_value=b"short"), self.assertRaises(ValueError):
            package.signature_operation("text", Path("unread-private.pem"))
        with mock.patch.object(package.subprocess, "run", side_effect=[
                subprocess.CompletedProcess([], 0, b"OpenSSL 3.5.0\n"),
                subprocess.CompletedProcess([], 1, b"", b"private diagnostic")]):
            with self.assertRaisesRegex(ValueError, "diagnostics suppressed") as error:
                package.openssl(["pkeyutl"])
        self.assertNotIn("private diagnostic", str(error.exception))

    def test_wrong_signing_key_never_produces_trusted_manifest(self):
        output, _ = self.build()
        text = (output / "system-update.unsigned.json").read_text()
        with mock.patch.object(package, "signature_operation", side_effect=["ab" * 64, ValueError("wrong public key")]):
            with self.assertRaisesRegex(ValueError, "wrong public key"):
                package.sign_payload(text, self.root / "unread-private.pem", output)
        self.assertFalse((output / "system-update.json").exists())

    def test_manifest_size_limit_and_font_requirement(self):
        output, payload = self.build()
        with mock.patch.object(package, "MAX_MANIFEST", 8), self.assertRaises(ValueError):
            package.verify_package(output / "system-update.unsigned.json", None)
        (self.bundle / "assets/fonts/font.ttf").rename(self.bundle / "assets/fonts/LICENSE")
        with self.assertRaisesRegex(ValueError, ".ttf font"):
            package.collect(self.bundle)
        changed = copy.deepcopy(payload)
        for entry in changed["files"]:
            if entry["path"] == "assets/fonts/font.ttf":
                entry["path"] = "assets/fonts/readme.txt"
        with self.assertRaisesRegex(ValueError, "Required runtime"):
            package.validate_payload(changed)

    def test_cli_unsigned_build_and_verify(self):
        output = self.root / "cli"
        built = subprocess.run([sys.executable, str(SCRIPT), "--bundle", str(self.bundle),
                                "--output", str(output), "--version", VERSION, "--revision", REVISION,
                                "--build-version", VERSION], capture_output=True, text=True)
        self.assertEqual(built.returncode, 0, built.stderr)
        self.assertIn("UNSIGNED", built.stdout)
        verified = subprocess.run([sys.executable, str(SCRIPT), "--verify", str(output / "system-update.unsigned.json")],
                                   capture_output=True, text=True)
        self.assertEqual(verified.returncode, 0, verified.stderr)
        self.assertIn("NOT an authenticated release", verified.stdout)


if __name__ == "__main__":
    unittest.main()
