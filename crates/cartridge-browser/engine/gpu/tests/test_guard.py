import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest

GPU = Path(__file__).resolve().parents[1]


class GuardTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        (GPU / 'build').mkdir(exist_ok=True)
        cls.directory = tempfile.TemporaryDirectory(prefix='guard-tests-', dir=GPU / 'build')
        cls.out = Path(cls.directory.name)
        compiler = shlex.split(os.environ.get('CC', 'cc'))
        shared = ['-dynamiclib'] if sys.platform == 'darwin' else ['-shared', '-Wl,-Bsymbolic-functions', '-Wl,-z,defs']
        common = compiler + ['-std=c11', '-O2', '-Wall', '-Wextra', '-Werror', '-fPIC', '-fvisibility=hidden'] + shared
        cls.guard = cls.out / 'guard.so'
        subprocess.run(common + [str(GPU / 'gbm_guard.c'), '-o', str(cls.guard), '-lpthread'] + ([] if sys.platform == 'darwin' else ['-ldl']), check=True)
        cls.backends = {}
        for name, flags in [('legacy', []), ('complete', ['-DCOMPLETE_BUFFER_ABI']),
                            ('partial', ['-DCOMPLETE_BUFFER_ABI', '-DOMIT_OFFSET']),
                            ('no-plane-fd', ['-DCOMPLETE_BUFFER_ABI', '-DOMIT_PLANE_FD'])]:
            path = cls.out / (name + '.so')
            subprocess.run(common + flags + [str(GPU / 'tests/fake_backend.c'), '-o', str(path)], check=True)
            cls.backends[name] = path
        cls.backends['mixed'] = cls.out / 'mixed.so'
        subprocess.run(common + ['-DREQUIRE_DEPENDENCY', str(GPU / 'tests/fake_backend.c'),
                                  str(cls.backends['complete']), '-o', str(cls.backends['mixed'])], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.directory.cleanup()

    def case(self, case, backend):
        env = dict(os.environ)
        env.pop('CARTRIDGE_GBM_BACKEND', None)
        env.pop('PYTHONOPTIMIZE', None)
        result = subprocess.run([sys.executable, str(GPU / 'tests/guard_cases.py'), str(self.guard), str(backend), case],
                                env=env, capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_legacy_never_allocates_or_fabricates_planes(self):
        self.case('legacy', self.backends['legacy'])

    def test_one_missing_required_function_rejects_all_buffers(self):
        self.case('partial', self.backends['partial'])

    def test_complete_backend_values_errors_and_callbacks_are_forwarded(self):
        self.case('complete', self.backends['complete'])

    def test_optional_plane_fd_can_fail_for_chromiums_real_drm_fallback(self):
        self.case('no-plane-fd', self.backends['no-plane-fd'])

    def test_no_implicit_backend(self):
        self.case('unset', self.backends['legacy'])

    def test_relative_backend_refused(self):
        self.case('relative', 'legacy.so')

    def test_missing_backend_fails_without_fallback_search(self):
        self.case('missing', self.out / 'does-not-exist.so')

    def test_self_recursion_refused(self):
        self.case('self', self.guard)

    def test_concurrent_initialization_is_consistent(self):
        self.case('concurrent', self.backends['legacy'])

    def test_dependency_symbols_cannot_mix_opaque_buffer_implementations(self):
        self.case('legacy', self.backends['mixed'])
