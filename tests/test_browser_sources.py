"""Offline source fixtures: no real archives, host APT, signing or device access."""
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("browser_sources", ROOT / "scripts/collect_browser_sources.py")
sources = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sources)


def digest(body):
    return hashlib.sha256(body).hexdigest()


def stanza(version="1:2.0-3", extra=True):
    payloads = {"example_2.0-3.dsc": b"descriptor", "example_2.0.orig.tar.xz": b"upstream",
                "example_2.0-3.debian.tar.xz": b"patches"}
    if extra:
        payloads["example_2.0.orig-pre-gen.tar.xz"] = b"pre-generated"
        payloads["example_2.0.orig.tar.xz.asc"] = b"signature"
    text = f"Package: example\nVersion: {version}\nDirectory: pool/main/e/example\nChecksums-Sha256:\n"
    text += "".join(f" {digest(body)} {len(body)} {name}\n" for name, body in payloads.items())
    uris = "".join(f"'https://deb.debian.org/debian/pool/main/e/example/{name}' {name} {len(body)} SHA256:{digest(body)}\n"
                   for name, body in payloads.items())
    return text, uris, payloads


class SourceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="browser-source-test-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.addCleanup(mock.patch.stopall)
        mock.patch.object(sources, "print", create=True).start()
        mock.patch.object(sources, "BWRAP_SIZE", 5).start()
        mock.patch.object(sources, "BWRAP_SHA256", digest(b"bwrap")).start()

    def fixture(self, downloaded=True):
        text, uris, payloads = stanza()
        files = {"evidence/binary-sources.tsv": b"example-bin:arm64\t2.0-3+b1\texample\t1:2.0-3\n",
                 "evidence/packages.tsv": b"example-bin:arm64\t2.0-3+b1\n",
                 "evidence/component.json": b'{"schema":1}\n',
                 "evidence/build-arm64.sh": b"fixture build recipe\n",
                 "evidence/static-inputs.json": b"[]\n",
                 "evidence/static-binary-sources.tsv": b"libc6\t2.39-1\tglibc\t2.39-1\nlibcap2\t1:2.66-1\tlibcap2\t1:2.66-1\n",
                 "evidence/source-packages.tsv": b"example\t1:2.0-3\n",
                 "evidence/notices/COPYING": b"preserved notice",
                 "records/debian/example/Sources": text.encode(), "records/debian/example/uris.txt": uris.encode()}
        mapping = [dict(distribution="debian", package="example", version="1:2.0-3",
                        binaries=[dict(package="example-bin:arm64", version="2.0-3+b1")],
                        record="records/debian/example/Sources", uris="records/debian/example/uris.txt",
                        artifacts=["artifacts/debian/" + n for n in payloads])]
        artifacts = [dict(path="artifacts/debian/" + n, size=len(body), sha256=digest(body), distribution="debian",
                          url="https://deb.debian.org/debian/pool/main/e/example/" + n, authentication="apt-signed-index")
                     for n, body in payloads.items()]
        # Ubuntu source records need their own origins even for the same upstream names.
        for package, version, binary in (("glibc", "2.39-1", "libc6"), ("libcap2", "1:2.66-1", "libcap2")):
            t = text.replace("Package: example", "Package: " + package).replace("Version: 1:2.0-3", "Version: " + version)
            u = uris.replace("https://deb.debian.org/debian", "https://ports.ubuntu.com/ubuntu-ports")
            record = "records/ubuntu/" + package
            files[record + "/Sources"] = t.encode()
            files[record + "/uris.txt"] = u.encode()
            mapping.append(dict(distribution="ubuntu", package=package, version=version,
                                binaries=[dict(package=binary, version=version)], record=record + "/Sources",
                                uris=record + "/uris.txt", artifacts=["artifacts/ubuntu/" + n for n in payloads]))
        artifacts += [dict(path="artifacts/ubuntu/" + n, size=len(body), sha256=digest(body), distribution="ubuntu",
                           url="https://ports.ubuntu.com/ubuntu-ports/pool/main/e/example/" + n, authentication="apt-signed-index")
                      for n, body in payloads.items()]
        artifacts.append(dict(path="artifacts/upstream/bubblewrap-0.13.1.tar.xz", size=5, sha256=digest(b"bwrap"),
                              distribution="upstream", url=sources.BWRAP_URL, authentication="builder-pinned-sha256"))
        for name, body in files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(body)
        evidence = [sources.file_record(self.root, self.root / name) for name in files]
        value = dict(schema=1, kind="browser-source-collection", sources=mapping, artifacts=artifacts,
                     evidence=evidence, source_bytes=sum(a["size"] for a in artifacts),
                     evidence_bytes=sum(e["size"] for e in evidence), reserve_bytes=sources.RESERVE)
        sources.write_json(self.root / sources.LOCK, value)
        if downloaded:
            for a in artifacts:
                path = self.root / a["path"]
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"bwrap" if a["distribution"] == "upstream" else payloads[path.name])
        return value

    def test_exact_epoch_version_and_all_orig_patch_signature_files_retained(self):
        text, uris, payloads = stanza()
        fields, _ = sources.exact_source(stanza("2.0-4")[0] + "\n" + text, "example", "1:2.0-3")
        self.assertEqual(sources.source_files(fields).keys(), payloads.keys())
        self.assertEqual(sources.parse_uris(uris, "debian", fields).keys(), payloads.keys())
        for version in ("2.0-3", "1:2.0-4", "1:2.0-3+b1"):
            with self.subTest(version=version), self.assertRaisesRegex(ValueError, "Exact source unavailable"):
                sources.exact_source(text, "example", version)

    def test_missing_descriptor_duplicate_and_unsafe_source_files_fail(self):
        text, _, _ = stanza()
        for bad in (text.replace("example_2.0-3.dsc", "../escape.dsc"),
                    text.replace("example_2.0-3.dsc", "/tmp/escape.dsc"),
                    text.replace("example_2.0-3.dsc", "no-descriptor.txt"),
                    text + text.split("Checksums-Sha256:\n")[1].splitlines()[0] + "\n"):
            with self.subTest(text=bad), self.assertRaises(ValueError):
                sources.exact_source(bad, "example", "1:2.0-3")

    def test_uri_origin_checksum_size_and_completeness(self):
        text, uris, _ = stanza()
        fields, _ = sources.exact_source(text, "example", "1:2.0-3")
        for bad in (uris.replace("https://", "http://"), uris.replace("deb.debian.org", "evil.example"),
                    uris.replace("deb.debian.org", "deb.debian.org@evil.example"),
                    uris.replace("pool/main/e/example", "pool/../escape"),
                    uris.replace(" 10 SHA256:", " 11 SHA256:"), uris.replace("SHA256:", "MD5Sum:"),
                    "\n".join(uris.splitlines()[:-1]), uris + uris.splitlines()[0] + "\n"):
            with self.subTest(uris=bad), self.assertRaises(ValueError):
                sources.parse_uris(bad, "debian", fields)

    def test_identical_main_security_uploads_and_ubuntu_sha512(self):
        text, uris, payloads = stanza()
        security = text.replace("Directory: pool/main/", "Directory: pool/updates/main/")
        security_uris = uris.replace("https://deb.debian.org/debian/pool/main/",
                                     "https://security.debian.org/debian-security/pool/updates/main/")
        fields, _ = sources.exact_source(text + "\n" + security, "example", "1:2.0-3", security_uris, "debian")
        self.assertEqual(fields["Directory"], "pool/updates/main/e/example")
        with self.assertRaisesRegex(ValueError, "Conflicting exact source checksums"):
            sources.exact_source(text + "\n" + security.replace(digest(b"upstream"), "a" * 64), "example", "1:2.0-3")
        fields, _ = sources.exact_source(text, "example", "1:2.0-3")
        fields["Checksums-Sha512"] = "\n".join(f"{hashlib.sha512(b).hexdigest()} {len(b)} {n}" for n, b in payloads.items())
        ubuntu_uris = uris.replace("https://deb.debian.org/debian", "https://ports.ubuntu.com/ubuntu-ports")
        for body in payloads.values():
            ubuntu_uris = ubuntu_uris.replace("SHA256:" + digest(body), "SHA512:" + hashlib.sha512(body).hexdigest())
        self.assertEqual(set(sources.parse_uris(ubuntu_uris, "ubuntu", fields)), set(payloads))
        with self.assertRaisesRegex(ValueError, "checksum"):
            sources.parse_uris(ubuntu_uris.replace(hashlib.sha512(b"upstream").hexdigest(), "a" * 128), "ubuntu", fields)

    def test_complete_index_verifies_and_preserves_binary_source_distinction(self):
        value = self.fixture()
        sources.validate_index(self.root, value)
        result = sources.verify(self.root, complete=False)
        self.assertEqual(result["distro_sources"], 3)
        self.assertEqual(value["sources"][0]["version"], "1:2.0-3")
        self.assertEqual(value["sources"][0]["binaries"][0]["version"], "2.0-3+b1")
        self.assertIn("not asserted", result["status"])

    def test_index_mapping_tampering_and_dropped_sidecars_fail(self):
        original = self.fixture()
        for attack in ("version", "binary", "source", "missing-orig", "size", "digest", "url", "duplicate", "bwrap"):
            value = copy.deepcopy(original)
            if attack == "version":
                value["sources"][0]["version"] = "2.0-3"
            elif attack == "binary":
                value["sources"][0]["binaries"][0]["version"] = "2.0-3"
            elif attack == "source":
                value["sources"].pop()
            elif attack == "missing-orig":
                value["sources"][0]["artifacts"].pop()
            elif attack in ("size", "digest", "url"):
                key = {"size": "size", "digest": "sha256", "url": "url"}[attack]
                value["artifacts"][0][key] = {"size": 1, "digest": "0" * 64, "url": "https://evil.example/x"}[attack]
            elif attack == "duplicate":
                value["artifacts"].append(value["artifacts"][0])
            else:
                value["artifacts"][-1]["sha256"] = "a" * 64
            with self.subTest(attack=attack), self.assertRaises(ValueError):
                sources.validate_index(self.root, value)

    def test_checksum_missing_extra_and_symlink_artifacts_fail(self):
        value = self.fixture()
        item = self.root / value["artifacts"][0]["path"]
        original = item.read_bytes()
        item.write_bytes(b"corrupted")
        with self.assertRaisesRegex(ValueError, "checksum"):
            sources.verify(self.root, complete=False)
        item.unlink()
        with self.assertRaises(OSError):
            sources.verify(self.root, complete=False)
        external = self.root / "external"
        external.write_bytes(original)
        item.symlink_to(external)
        with self.assertRaisesRegex(ValueError, "Symlink"):
            sources.verify(self.root, complete=False)
        item.unlink()
        item.write_bytes(original)
        with self.assertRaisesRegex(ValueError, "Unlisted"):
            sources.verify(self.root, complete=False)

    def test_path_policy_rejects_traversal_absolute_and_symlink_parent(self):
        for path in ("/tmp/file", "../file", "a/../b", "a//b", "./a", "a\\b", "a\nb", "a%2fb", "a:b"):
            with self.subTest(path=path), self.assertRaises(ValueError):
                sources.within(self.root, path)
        (self.root / "alias").symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "Symlink"):
            sources.within(self.root, "alias/file")

    def test_scratch_output_cannot_inject_apt_configuration_or_overlap_engine(self):
        for path in (Path('/tmp/unsafe"path'), Path("/tmp/unsafe\npath"), Path("/opt/engine")):
            with self.subTest(path=path), self.assertRaises(ValueError):
                sources.scratch_path(path)
        output = self.root / "engine/sources"
        output.parent.mkdir()
        with mock.patch.object(sources, "check_vm"), mock.patch.object(sources, "scratch_path"), \
             mock.patch.object(sources, "snapshot") as snapshot:
            with self.assertRaisesRegex(ValueError, "disjoint"):
                sources.plan(output.parent, output, self.root / "recipe")
            snapshot.assert_not_called()
        self.assertFalse(output.exists())

    def test_insufficient_space_refuses_before_opening_network(self):
        self.fixture(downloaded=False)
        with mock.patch.object(sources, "check_vm"), mock.patch.object(sources, "scratch_path"), \
             mock.patch.object(sources.shutil, "disk_usage", return_value=type("Usage", (), {"free": sources.RESERVE})()), \
             mock.patch.object(sources, "download") as download:
            with self.assertRaisesRegex(ValueError, "Insufficient space"):
                sources.collect(self.root, None)
            download.assert_not_called()
        self.assertEqual(sources.ensure_space(self.root, 10, free=sources.RESERVE + 10), sources.RESERVE + 10)

    def test_download_bounds_hashes_and_partial_cleanup(self):
        item = dict(path="artifacts/debian/a.tar.xz", distribution="debian", size=4, sha256=digest(b"data"),
                    url="https://deb.debian.org/debian/pool/main/a/a.tar.xz")
        for body in (b"wrong", b"bad", b"evil"):
            opener = mock.Mock()
            opener.open.return_value = io.BytesIO(body)
            with mock.patch.object(sources.urllib.request, "build_opener", return_value=opener):
                with self.assertRaises(ValueError):
                    sources.download(self.root, item, None)
                self.assertFalse((self.root / item["path"]).exists())
                self.assertFalse((self.root / (item["path"] + ".partial")).exists())

    def test_collect_resumes_verified_files_and_detects_completion_tampering(self):
        self.fixture()
        with mock.patch.object(sources, "check_vm"), mock.patch.object(sources, "scratch_path"), \
             mock.patch.object(sources, "download") as download:
            sources.collect(self.root, None)
            sources.collect(self.root, None)
            download.assert_not_called()
        self.assertTrue((self.root / "SHA256SUMS").is_file())
        (self.root / "SHA256SUMS").write_text("bad\n")
        with self.assertRaisesRegex(ValueError, "Completion"):
            sources.verify(self.root)

    def test_apt_isolated_configuration_and_signature_verification(self):
        calls = []
        def fake_run(args, env=None):
            calls.append((args, env))
            if args == ["apt-get", "update"]:
                apt = self.root / "apt/debian"
                for n in ("bookworm", "security"):
                    (apt / ("state/lists/" + n + "_InRelease")).write_text("signed fixture")
                return "authenticated fixture"
            return "[GNUPG:] VALIDSIG fixture\n"
        with mock.patch.object(sources.shutil, "copyfile"), mock.patch.object(sources, "run", side_effect=fake_run):
            env = sources.isolated_apt(self.root, "debian")
        conf = (self.root / "apt/debian/apt.conf").read_text()
        for directive in ("Dir::Etc", "Dir::State", "Dir::Cache", "Dir::Log", "APT::Update::Error-Mode"):
            self.assertIn(directive, conf)
        self.assertEqual(env["APT_CONFIG"], str(self.root / "apt/debian/apt.conf"))
        self.assertEqual(sum(args[0] == "gpgv" for args, _ in calls), 2)
        self.assertNotIn("trusted=yes", (self.root / "apt/debian/etc/sources.list").read_text())
        self.assertIn("signed-by=", (self.root / "apt/debian/etc/sources.list").read_text())

    def test_duplicate_json_keys_and_literal_backslash_inventory_rejected(self):
        path = self.root / "index"
        path.write_text('{"schema":1,"schema":2}')
        with self.assertRaisesRegex(ValueError, "Duplicate JSON"):
            sources.strict_json(path)
        with self.assertRaisesRegex(ValueError, "tab-separated"):
            sources.parse_tsv("example\\t1.0\n", 2)

    def test_plan_unavailable_version_retains_report_without_payload_downloads(self):
        output = self.root / "out"
        component = self.root / "engine"
        component.mkdir()
        mapping = {"debian": [("example-bin:arm64", "2.0-3+b1", "example", "1:2.0-3")]}
        calls = []
        def fake_run(args, env=None):
            calls.append(args)
            return stanza(version="1:2.0-4")[0] if args[0] == "apt-cache" else stanza()[1]
        with mock.patch.object(sources, "check_vm"), mock.patch.object(sources, "scratch_path"), \
             mock.patch.object(sources, "snapshot", return_value=mapping), \
             mock.patch.object(sources, "isolated_apt", return_value={}), \
             mock.patch.object(sources, "run", side_effect=fake_run), \
             mock.patch.object(sources, "download") as download:
            with self.assertRaisesRegex(ValueError, "no archive downloads or fallback"):
                sources.plan(component, output, self.root / "recipe")
            download.assert_not_called()
        self.assertTrue((output / "unresolved.json").exists())
        self.assertFalse((output / sources.LOCK).exists())
        self.assertFalse((output / "artifacts").exists())
        self.assertEqual(calls[-1][-1], "example=1:2.0-3")

    def test_unknown_rollover_signature_requires_valid_trusted_signature(self):
        def setup_run(status):
            def fake(args, env=None):
                if args == ["apt-get", "update"]:
                    for n in ("bookworm", "security"):
                        (self.root / ("apt/debian/state/lists/" + n + "_InRelease")).write_text("signed fixture")
                    return "authenticated fixture"
                raise sources.subprocess.CalledProcessError(2, args, output=status)
            return fake
        for n, status in enumerate(("[GNUPG:] NO_PUBKEY unknown\n",
                                     "[GNUPG:] VALIDSIG trusted\n[GNUPG:] NO_PUBKEY unknown\n[GNUPG:] BADSIG bad\n",
                                     "[GNUPG:] VALIDSIG trusted\n[GNUPG:] NO_PUBKEY unknown\n")):
            case = self.root / str(n)
            case.mkdir()
            # Generate the two diagnostic release fixtures relative to this case.
            original_root = self.root
            self.root = case
            try:
                with mock.patch.object(sources.shutil, "copyfile"), mock.patch.object(sources, "run", side_effect=setup_run(status)):
                    if n == 2:
                        sources.isolated_apt(case, "debian")
                    else:
                        with self.assertRaisesRegex(ValueError, "No valid archive"):
                            sources.isolated_apt(case, "debian")
            finally:
                self.root = original_root


if __name__ == "__main__":
    unittest.main()
