"""Component archive invariants; no root, device, network or ARM VM needed."""
import importlib.util
from importlib.machinery import SourceFileLoader
from unittest.mock import patch
from types import SimpleNamespace
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

class WrapperTests(unittest.TestCase):
    def invoke(self, args, pipe=True):
        wrapper_path=Path(__file__).with_name("chromium")
        spec=importlib.util.spec_from_loader("engine_wrapper",SourceFileLoader("engine_wrapper",str(wrapper_path)))
        wrapper=importlib.util.module_from_spec(spec)
        spec.loader.exec_module(wrapper)
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root / "rootfs/usr/lib/chromium").mkdir(parents=True)
            (root / "rootfs/usr/lib/chromium/chromium").touch()
            (root / "bubblewrap").touch()
            (root / "profile").mkdir()
            with patch.object(wrapper,"__file__",str(root / "chromium")), \
                patch.object(wrapper.platform,"system",return_value="Linux"), \
                patch.object(wrapper.platform,"machine",return_value="aarch64"), \
                patch.object(wrapper.os,"geteuid",return_value=1000), \
                patch.object(wrapper.sys,"argv",["chromium","--user-data-dir="+str(root / "profile"),*args]), \
                patch.object(wrapper.os,"fstat",return_value=SimpleNamespace(st_mode=stat.S_IFIFO if pipe else stat.S_IFREG)), \
                patch.object(wrapper.os,"set_inheritable") as inherited, \
                patch.object(wrapper.os,"execv") as execute:
                wrapper.main()
                return execute.call_args.args[1],inherited.call_args_list

    def test_private_pipes_are_inherited_without_nonexistent_helper_options(self):
        command,inherited=self.invoke(["--remote-debugging-pipe","--headless=new"])
        self.assertNotIn("--preserve-fds",command)
        self.assertIn("--unshare-user",command)
        self.assertIn("--unshare-pid",command)
        self.assertIn("--remote-debugging-pipe",command)
        self.assertEqual([c.args for c in inherited],[(3,True),(4,True)])

    def test_sandbox_bypass_and_nonpipe_transport_are_refused(self):
        for flag in ["--no-sandbox","--disable-seccomp-filter-sandbox","--disable-namespace-sandbox", "--remote-debugging-port=9222"]:
            with self.subTest(flag=flag),self.assertRaisesRegex(RuntimeError,"Unsafe engine option"):
                self.invoke([flag])
        with self.assertRaisesRegex(RuntimeError,"private pipes"):
            self.invoke(["--remote-debugging-pipe"],pipe=False)

if __name__ == "__main__":
    unittest.main()
