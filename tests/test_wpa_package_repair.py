"""Test guards and rollback for the two-file WPA recovery."""
import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT/'deploy/wifi/restore-wpa-package.py'
spec=importlib.util.spec_from_file_location('wpa_repair',SOURCE)
repair=importlib.util.module_from_spec(spec);spec.loader.exec_module(repair)

class PackageRepairTest(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.root=(Path(self.tmp.name)/'root').resolve();self.payload=(Path(self.tmp.name)/'payload').resolve()
        (self.root/'sbin').mkdir(parents=True);self.payload.mkdir()
        self.originals={};self.clean={};hashes={}
        checks=self.root/'var/lib/dpkg/info/wpasupplicant.md5sums';checks.parent.mkdir(parents=True)
        lines=[]
        for name in repair.FILES:
            self.originals[name]=b'\x7fELFdamaged '+name.encode();self.clean[name]=b'\x7fELFclean '+name.encode()
            hashes[name]={'damaged':repair.digest(self.originals[name]),'clean':repair.digest(self.clean[name]),'md5':hashlib.md5(self.clean[name]).hexdigest()}
            lines.append(hashes[name]['md5']+'  sbin/'+name)
            (self.root/'sbin'/name).write_bytes(self.originals[name]);(self.root/'sbin'/name).chmod(0o700)
            (self.payload/name).write_bytes(self.clean[name])
        checks.write_text('\n'.join(lines)+'\n')
        self.hash_patch=patch.object(repair,'FILES',hashes);self.hash_patch.start();self.addCleanup(self.hash_patch.stop)
        self.save=self.root/'roms/game.srm';self.save.parent.mkdir();self.save.write_bytes(b'game save')
        self.config=self.root/'etc/wpa_supplicant.conf';self.config.parent.mkdir();self.config.write_bytes(b'private configuration')
    def unchanged(self):
        for name in self.originals:self.assertEqual((self.root/'sbin'/name).read_bytes(),self.originals[name])
        self.assertFalse((self.root/'var/lib/cartridge').exists())
    def test_unknown_binary_and_payload_fail_before_any_write(self):
        for kind in ['installed','payload']:
            target=(self.root/'sbin/wpa_cli') if kind=='installed' else self.payload/'wpa_cli'
            saved=target.read_bytes();target.write_bytes(b'unknown')
            with self.assertRaises(RuntimeError):repair.repair(self.root,self.payload)
            target.write_bytes(saved);self.unchanged()
    def test_verify_only_does_not_create_backup_or_change_files(self):
        r=repair.repair(self.root,self.payload,verify_only=True)
        self.assertEqual(len(r['files_needing_repair']),2);self.unchanged()
    def test_restore_retains_backups_settings_games_and_is_idempotent(self):
        r=repair.repair(self.root,self.payload);backup=Path(r['backup'])
        for name in self.originals:
            self.assertEqual((backup/name).read_bytes(),self.originals[name])
            self.assertEqual((self.root/'sbin'/name).read_bytes(),self.clean[name])
        self.assertEqual(self.save.read_bytes(),b'game save');self.assertEqual(self.config.read_bytes(),b'private configuration')
        before=set((self.root/'var/lib/cartridge/wpa-package-repair').iterdir())
        self.assertEqual(repair.repair(self.root,self.payload)['state'],'already_repaired')
        self.assertEqual(before,set((self.root/'var/lib/cartridge/wpa-package-repair').iterdir()))
    def test_failure_after_first_replacement_restores_both_originals(self):
        write=repair.atomic_write
        def failing(path,data,*args,**kwargs):
            if path==self.root/'sbin/wpa_cli' and data==self.clean['wpa_cli']:raise OSError('simulated disk write failure')
            return write(path,data,*args,**kwargs)
        with patch.object(repair,'atomic_write',side_effect=failing):
            with self.assertRaises(OSError):repair.repair(self.root,self.payload)
        for name in self.originals:
            self.assertEqual((self.root/'sbin'/name).read_bytes(),self.originals[name])
            self.assertEqual((self.root/'sbin'/name).stat().st_mode & 0o777,0o700)
    def test_symlink_and_offline_service_restart_are_rejected(self):
        target=self.root/'sbin/wpa_cli';target.unlink();target.symlink_to(self.save)
        with self.assertRaises(RuntimeError):repair.repair(self.root,self.payload)
        self.assertEqual(self.save.read_bytes(),b'game save')
        with self.assertRaises(RuntimeError):repair.repair(self.root,self.payload,restart=True)
        self.assertFalse((self.root/'var/lib/cartridge').exists())

if __name__=='__main__':unittest.main()
