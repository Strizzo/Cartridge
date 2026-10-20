import importlib.util
import io
from pathlib import Path
import tarfile
import unittest
spec=importlib.util.spec_from_file_location('store_sync',Path(__file__).resolve().parents[1]/'scripts/sync_store_apps.py')
sync=importlib.util.module_from_spec(spec);spec.loader.exec_module(sync)
class StoreSyncTests(unittest.TestCase):
    def test_unsafe_paths(self):
        for name in ['/root','../other','a/../../x','a\\b','./main.lua','a//b']:
            with self.assertRaises(ValueError):sync.safe_name(name)
        self.assertEqual(sync.safe_name('assets/map.png'),'assets/map.png')
    def test_archive_link_is_rejected(self):
        data=io.BytesIO()
        with tarfile.open(fileobj=data,mode='w:gz') as tar:
            item=tarfile.TarInfo('main.lua');item.type=tarfile.SYMTYPE;item.linkname='/etc/passwd';tar.addfile(item)
        with self.assertRaises(ValueError):sync.files_from_archive(data.getvalue())
    def test_bundled_release_snapshots_match(self):
        sync.check()
