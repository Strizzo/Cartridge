"""Exercise the ES entry point without allowing it to alter host boot services."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'deploy/tools/Cartridge.sh'


class ToolsLauncherTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='cartridge-launch-test-')
        self.addCleanup(self.tmp.cleanup)
        self.base = Path(self.tmp.name)
        self.app = self.base / 'Cartridge with spaces'
        self.app.mkdir()
        (self.app / 'assets').mkdir()
        self.commands = self.base / 'bin'
        self.commands.mkdir()
        self.forbidden = self.base / 'forbidden'
        self.probe = self.base / 'probe'
        trap = '#!/bin/bash\nprintf "%s\\n" "$0 $*" >> "$FORBIDDEN"\nexit 99\n'
        for name in ('systemctl', 'sudo', 'reboot', 'poweroff', 'shutdown'):
            self.executable(self.commands / name, trap)
        self.executable(self.app / 'autosetup.sh', trap)
        self.executable(self.commands / 'sleep', '#!/bin/bash\nexit 0\n')
        self.env = dict(os.environ, CARTRIDGE_DIR=str(self.app),
                        FORBIDDEN=str(self.forbidden), PROBE=str(self.probe),
                        PATH=str(self.commands) + os.pathsep + os.environ['PATH'])
        for name in ('SDL_VIDEODRIVER', 'SDL_AUDIODRIVER', 'CARTRIDGE_ASSETS'):
            self.env.pop(name, None)

    def executable(self, path, content):
        path.write_text(content)
        path.chmod(0o755)

    def launch(self, status=0):
        self.executable(self.app / 'cartridge', '''#!/bin/bash
printf '%s\\n' "$PWD" "$SDL_VIDEODRIVER" "$CARTRIDGE_ASSETS" "$*" > "$PROBE"
echo 'mock launcher output'
echo 'mock diagnostic' >&2
exit "$TEST_EXIT"
''')
        result = subprocess.run(['/bin/bash', str(SCRIPT), 'test argument'],
                                env=dict(self.env, TEST_EXIT=str(status)),
                                capture_output=True, text=True, timeout=10)
        self.assertFalse(self.forbidden.exists(), 'Manual launch altered boot services')
        return result

    def test_manual_launch_preserves_boot_and_captures_output(self):
        result = self.launch()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.probe.read_text().splitlines(),
                         [str(self.app), 'kmsdrm', str(self.app / 'assets'), 'test argument'])
        log = (self.app / 'launch.log').read_text()
        self.assertIn('mock launcher output', log)
        self.assertIn('mock diagnostic', log)

    def test_failure_returns_status_and_displays_saved_error(self):
        result = self.launch(127)
        self.assertEqual(result.returncode, 127)
        self.assertIn('Returning to EmulationStation', result.stdout)
        self.assertIn('mock diagnostic', result.stdout)
        self.assertIn('mock diagnostic', (self.app / 'launch.log').read_text())

    def test_intentional_session_handoffs_return_to_parent_cleanly(self):
        for status in (20, 30):
            result = self.launch(status)
            self.assertEqual(result.returncode, 0)
            self.assertNotIn('exited with code', result.stdout)

    def test_missing_binary_does_not_install_or_change_services(self):
        result = subprocess.run(['/bin/bash', str(SCRIPT)], env=self.env,
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertFalse(self.forbidden.exists())
        self.assertIn('CartridgeOS not found', result.stdout)


if __name__ == '__main__':
    unittest.main()
