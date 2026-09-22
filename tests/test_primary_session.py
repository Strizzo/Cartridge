"""Exercise startup failure, fallback, shutdown and reversible service integration."""
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('setup_primary', ROOT/'deploy/setup-primary.py')
setup = importlib.util.module_from_spec(spec); spec.loader.exec_module(setup)


class PrimarySessionTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='cartridge-primary-test-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.app = self.root/'Cartridge'; (self.app/'assets/fonts').mkdir(parents=True)
        self.state = self.root/'state'
        self.fallback = self.root/'es.sh'
        self.es_seen = self.root/'es-seen'
        self.fallback.write_text('#!/bin/bash\nprintf "%s" "${CARTRIDGE_READY_FILE-unset}:${SDL_VIDEODRIVER-unset}" > "$ES_SEEN"\n')
        self.env = dict(os.environ, ES_SEEN=str(self.es_seen))
        self.env.pop('CARTRIDGE_READY_FILE', None); self.env.pop('SDL_VIDEODRIVER', None)

    def binary(self, body):
        path = self.app/'cartridge'
        path.write_text('#!'+sys.executable+'\nimport os,time\nfrom pathlib import Path\n'+body+'\n')
        path.chmod(0o755)

    def launch(self):
        return subprocess.run([sys.executable, str(ROOT/'deploy/cartridge-session.py'),
                               '--cartridge-dir', str(self.app), '--state-dir', str(self.state),
                               '--es-script', str(self.fallback), '--startup-timeout', '0.4', '--desktop'],
                              env=self.env, capture_output=True, text=True, timeout=6)

    def reason(self):return json.loads((self.state/'last-session.json').read_text())['reason']

    def test_ready_then_es_request(self):
        self.binary("Path(os.environ['CARTRIDGE_READY_FILE']).write_text('ready'); raise SystemExit(20)")
        r=self.launch(); self.assertEqual(r.returncode,0,r.stderr)
        self.assertEqual(self.reason(),'emulationstation_requested')
        self.assertEqual(self.es_seen.read_text(),'unset:unset')
        self.assertFalse((self.state/'fallback.json').exists())

    def test_crash_latches_fallback_and_does_not_restart(self):
        self.binary("Path(os.environ['CARTRIDGE_READY_FILE']).write_text('ready'); raise SystemExit(9)")
        self.launch(); self.assertEqual(self.reason(),'crashed')
        self.binary("raise RuntimeError('must not run again')")
        self.launch(); self.assertEqual(self.reason(),'previous_failure')
        self.assertTrue(self.es_seen.exists())

    def test_crash_cleans_emulator_descendant_before_fallback(self):
        marker = self.root/'orphan-emulator-ran'
        pidfile = self.root/'emulator-pid'
        child_code = "import signal,time; from pathlib import Path; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(0.6); Path("+repr(str(marker))+").touch(); time.sleep(10)"
        self.binary("import subprocess,sys\np=subprocess.Popen([sys.executable,'-c',"+repr(child_code)+"])\nPath("+repr(str(pidfile))+").write_text(str(p.pid))\nPath(os.environ['CARTRIDGE_READY_FILE']).write_text('ready')\ntime.sleep(0.1)\nraise SystemExit(9)")
        try:
            self.launch(); self.assertEqual(self.reason(),'crashed')
            time.sleep(0.7)
            self.assertFalse(marker.exists(), 'Orphan emulator survived the frontend crash')
            self.assertTrue(self.es_seen.exists())
        finally:
            if pidfile.exists():
                try: os.kill(int(pidfile.read_text()), signal.SIGKILL)
                except ProcessLookupError: pass

    def test_timeout_kills_hung_startup_and_falls_back(self):
        self.binary('time.sleep(30)')
        start=time.monotonic(); self.launch()
        self.assertLess(time.monotonic()-start,3)
        self.assertEqual(self.reason(),'startup_timeout'); self.assertTrue(self.es_seen.exists())

    def test_missing_binary_keeps_stock_boot(self):
        self.launch(); self.assertEqual(self.reason(),'installation_missing')
        self.assertTrue(self.es_seen.exists())

    def test_recovery_file_skips_cartridge(self):
        self.binary("raise RuntimeError('should never start')")
        (self.app/'boot-emulationstation').touch()
        self.launch(); self.assertEqual(self.reason(),'recovery_requested')

    def test_power_request_does_not_start_es(self):
        self.binary("Path(os.environ['CARTRIDGE_READY_FILE']).write_text('ready'); raise SystemExit(30)")
        self.assertEqual(self.launch().returncode,0)
        self.assertFalse(self.es_seen.exists()); self.assertEqual(self.reason(),'power_requested')

    def test_unusable_state_directory_still_falls_back(self):
        self.state.write_text('not a directory')
        self.assertEqual(self.launch().returncode,0); self.assertTrue(self.es_seen.exists())

    def test_sigterm_does_not_launch_fallback(self):
        self.binary("Path(os.environ['CARTRIDGE_READY_FILE']).write_text('ready'); time.sleep(30)")
        proc=subprocess.Popen([sys.executable,str(ROOT/'deploy/cartridge-session.py'),'--cartridge-dir',str(self.app),'--state-dir',str(self.state),'--es-script',str(self.fallback),'--desktop'],env=self.env)
        try:
            time.sleep(0.25);proc.terminate();proc.wait(timeout=5)
            self.assertFalse(self.es_seen.exists())
        finally:
            if proc.poll() is None:proc.kill();proc.wait()


class SetupTest(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory(prefix='cartridge-root-test-');self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)
        self.service=self.root/'etc/systemd/system/emulationstation.service'
        self.service.parent.mkdir(parents=True)
        self.original=b'[Service]\nUser=ark\nExecStart=/usr/bin/emulationstation/emulationstation.sh\n'
        self.service.write_bytes(self.original)
        wants=self.service.parent/'multi-user.target.wants';wants.mkdir()
        (wants/'emulationstation.service').symlink_to('../emulationstation.service')
        es=self.root/'usr/bin/emulationstation/emulationstation.sh';es.parent.mkdir(parents=True);es.write_text('stock script')
        self.app=self.root/'roms/Cartridge';(self.app/'assets/fonts').mkdir(parents=True);(self.app/'cartridge').touch()
        self.save=self.root/'roms/game.srm';self.save.write_bytes(b'precious save')
        self.recovery=self.service.parent/'emulationstation.service.d/90-cartridge-recovery.conf'
        self.recovery.parent.mkdir();self.recovery.write_text('[Service]\nStandardInput=null\n')

    def test_enable_disable_preserves_stock_unit_recovery_and_games(self):
        setup.configure(self.root,Path('/roms/Cartridge'),'enable')
        conf=(self.root/setup.DROPIN).read_text()
        self.assertIn('ExecStart=\nExecStart=/usr/bin/python3',conf)
        self.assertNotIn('Conflicts=',conf)
        setup.configure(self.root,Path('/roms/Cartridge'),'enable')
        setup.configure(self.root,Path('/roms/Cartridge'),'disable')
        self.assertEqual(self.service.read_bytes(),self.original)
        self.assertEqual(self.recovery.read_text(),'[Service]\nStandardInput=null\n')
        self.assertEqual(self.save.read_bytes(),b'precious save')
        self.assertFalse((self.root/setup.DROPIN).exists())
        self.assertTrue((self.service.parent/'multi-user.target.wants/emulationstation.service').is_symlink())

    def test_refuse_unknown_or_masked_service(self):
        self.service.write_text('different system')
        with self.assertRaisesRegex(RuntimeError,'Unrecognized'):setup.configure(self.root,Path('/roms/Cartridge'),'enable')
        self.assertFalse((self.root/setup.DROPIN).exists())

    def test_refuse_legacy_conflicting_boot_service(self):
        (self.service.parent/'multi-user.target.wants/cartridge-boot.service').symlink_to('../cartridge-boot.service')
        with self.assertRaisesRegex(RuntimeError,'Legacy'):setup.configure(self.root,Path('/roms/Cartridge'),'enable')
        self.assertFalse((self.root/setup.DROPIN).exists())

    def test_refuse_unowned_dropin(self):
        (self.root/setup.DROPIN).write_text('user config')
        with self.assertRaisesRegex(RuntimeError,'not owned'):setup.configure(self.root,Path('/roms/Cartridge'),'disable')
        self.assertEqual((self.root/setup.DROPIN).read_text(),'user config')

if __name__=='__main__':unittest.main()
