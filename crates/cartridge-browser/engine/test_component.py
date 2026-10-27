"""Component archive invariants; no root, device, network or ARM VM needed."""
import importlib.util
import os
from pathlib import Path
import stat
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("component", Path(__file__).with_name("prepare_component.py"))
component = importlib.util.module_from_spec(spec)
spec.loader.exec_module(component)

class ComponentTests(unittest.TestCase):
    def test_chroot_links_remain_internal_and_keep_their_meaning(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "etc/alternatives").mkdir(parents=True)
            (root / "usr/bin").mkdir(parents=True)
            (root / "usr/bin/tool").write_text("binary")
            (root / "etc/alternatives/tool").symlink_to("/usr/bin/tool")
            (root / "usr/bin/alias").symlink_to("/etc/alternatives/tool")
            (root / "etc/dangling").symlink_to("../../../run/not-present")
            component.normalize(root)
            self.assertEqual((root / "usr/bin/alias").read_text(), "binary")
            for path in [root / "usr/bin/alias", root / "etc/dangling"]:
                self.assertFalse(os.readlink(path).startswith("/"))
                self.assertTrue(path.resolve().is_relative_to(root.resolve()))

    def test_runtime_devices_are_removed_and_special_files_elsewhere_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "dev").mkdir()
            os.mkfifo(root / "dev/random")
            (root / "dev/fd").symlink_to("/proc/self/fd")
            component.clear_runtime_devices(root)
            self.assertEqual(list((root / "dev").iterdir()), [])
            os.mkfifo(root / "unexpected-fifo")
            with self.assertRaisesRegex(ValueError, "Special filesystem"):
                component.normalize(root)

    def test_archive_never_contains_setuid_or_setgid_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "helper"
            target.write_text("not privileged")
            target.chmod(0o6755)
            component.normalize(root)
            self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o755)
            self.assertEqual(component.inventory(root)[0]["size"], 14)

if __name__ == "__main__":
    unittest.main()
