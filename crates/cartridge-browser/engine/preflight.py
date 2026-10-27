#!/usr/bin/python3
"""Read-only handheld compatibility report. No root, card writes or reconfiguration."""
import argparse
import gzip
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys


def read(path, limit=16384):
    try:
        with open(path, 'rb') as source:
            return source.read(limit).decode(errors='replace').strip()
    except OSError:
        return None


def main():
    if platform.system() != 'Linux':
        raise RuntimeError('Run this check on the Linux handheld or ARM VM, not macOS.')
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--helper', type=Path, help='Exact trusted bubblewrap executable used by the component')
    args=parser.parse_args()
    report = dict(machine=platform.machine(), kernel=platform.release(), uid=os.geteuid(),
                  simulation=Path('/etc/cartridge-compat-vm').is_file(),
                  memory={line.split(':')[0]:line.split(':')[1].strip() for line in (read('/proc/meminfo') or '').splitlines() if line.split(':')[0] in ('MemTotal','MemAvailable','SwapTotal')},
                  userns_clone=read('/proc/sys/kernel/unprivileged_userns_clone'),
                  max_user_namespaces=read('/proc/sys/user/max_user_namespaces'),
                  profile_free_bytes=shutil.disk_usage(Path.home()).free,
                  helper_available=bool(shutil.which('bwrap')))
    try:
        with gzip.open('/proc/config.gz', 'rt') as config:
            report['kernel_features'] = [line.strip() for line in config if
                any(line.startswith('CONFIG_'+key+'=') or line.startswith('# CONFIG_'+key+' ') for key in
                    ['USER_NS','PID_NS','NET_NS','SECCOMP','SECCOMP_FILTER'])]
    except OSError:
        report['kernel_features'] = None
    # Probe only in a disposable child. Mapping the current user tests the same
    # unprivileged mechanism needed by bubblewrap, and changes no host policy.
    probe = r'''
import ctypes,os,sys
uid,gid=os.getuid(),os.getgid()
libc=ctypes.CDLL(None,use_errno=True)
if libc.unshare(0x10000000)!=0:
    code=ctypes.get_errno();print(os.strerror(code));sys.exit(1)
try:
    try: open('/proc/self/setgroups','w').write('deny')
    except FileNotFoundError: pass
    open('/proc/self/uid_map','w').write('1000 %d 1' % uid)
    open('/proc/self/gid_map','w').write('1000 %d 1' % gid)
    print('Unprivileged namespace mapped successfully')
except OSError as e: print(str(e));sys.exit(1)
'''
    helper=str(args.helper) if args.helper else shutil.which('bwrap')
    if helper:
        command=[helper, '--die-with-parent', '--unshare-user', '--unshare-pid',
            '--unshare-ipc', '--unshare-uts', '--uid', '1000', '--gid', '1000',
            '--ro-bind', '/', '/', '--proc', '/proc', '--dev', '/dev', '--', '/bin/true']
        report['probe_method']='bubblewrap with the exact helper: '+helper
    else:
        command=[sys.executable,'-c',probe]
        report['probe_method']='Python child; AppArmor may require a scoped permission on the actual helper'
    result = subprocess.run(command,capture_output=True,text=True,timeout=10)
    report['user_namespace_probe'] = dict(ok=result.returncode==0,
        detail=(result.stdout+result.stderr).strip()[:2000])
    report['ready_for_engine_trial'] = (report['machine'] in ('aarch64','arm64')
        and report['uid'] != 0 and result.returncode==0)
    report['note'] = 'Preflight only. This does not establish graphics, browser memory/latency, audio or battery performance.'
    print(json.dumps(report,indent=2))
    return 0 if report['ready_for_engine_trial'] else 1

if __name__ == '__main__':
    try:
        sys.exit(main())
    except (RuntimeError, OSError, subprocess.TimeoutExpired) as error:
        print(str(error),file=sys.stderr)
        sys.exit(1)
