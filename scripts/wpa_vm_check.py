#!/usr/bin/env python3
"""Exercise the handheld WPA executable with two Linux virtual Wi-Fi radios."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time


def run(args):
    if os.geteuid() != 0:
        raise RuntimeError('Run inside the isolated Linux VM with sudo')
    for name in ('wlan0', 'wlan1'):
        device = Path('/sys/class/net')/name
        if not device.exists() or 'mac80211_hwsim' not in str(device.resolve()):
            raise RuntimeError(name+' must be a virtual mac80211_hwsim radio; no test started')
    args.output.mkdir(parents=True, exist_ok=True)
    wpa_log = args.output/'wpa.log'
    ap_log = args.output/'ap.log'
    transcript = []
    ap, wpa, bus_pid = None, None, None
    with tempfile.TemporaryDirectory(prefix='cartridge-wpa-vm-') as tmp, wpa_log.open('w') as wf, ap_log.open('w') as af:
        tmp = Path(tmp)
        try:
            bus = subprocess.check_output(['dbus-daemon', '--session', '--fork',
                                           '--print-address=1', '--print-pid=1'], text=True).splitlines()
            bus_pid = int(bus[1])
            env = dict(os.environ, DBUS_SYSTEM_BUS_ADDRESS=bus[0])
            config = tmp/'ap.conf'
            config.write_text('interface=wlan1\ndriver=nl80211\nssid=CartridgeVMTest\n'
                              'hw_mode=g\nchannel=1\nwpa=2\nwpa_passphrase=CartridgeTestPassword\n'
                              'wpa_key_mgmt=WPA-PSK\nrsn_pairwise=CCMP\n')
            ap = subprocess.Popen(['hostapd', str(config)], stdout=af, stderr=subprocess.STDOUT)
            command = [str(args.wpa.resolve()), '-u', '-d', '-O', str(tmp/'control')]
            if args.loader:
                command = [str(args.loader.resolve()), '--library-path',
                           str(args.library_path.resolve())] + command
            elif args.library_path:
                env['LD_LIBRARY_PATH'] = str(args.library_path.resolve())
            wpa = subprocess.Popen(command, env=env, stdout=wf, stderr=subprocess.STDOUT)
            time.sleep(1)
            if ap.poll() is not None or wpa.poll() is not None:
                raise RuntimeError('AP or WPA exited before the D-Bus test; inspect logs')
            def call(path, method, *values, allow_failure=False):
                result = subprocess.run(['gdbus', 'call', '--address', bus[0],
                                         '--dest', 'fi.w1.wpa_supplicant1', '--object-path', path,
                                         '--method', method, *values], capture_output=True, text=True, timeout=6)
                transcript.append({'method': method, 'exit': result.returncode,
                                   'response': (result.stdout+result.stderr).strip()})
                if result.returncode and not allow_failure:
                    raise RuntimeError(method+' failed; inspect transcript')
                return result
            iface = '/fi/w1/wpa_supplicant1/Interfaces/0'
            created = call('/fi/w1/wpa_supplicant1', 'fi.w1.wpa_supplicant1.CreateInterface',
                           "{'Ifname': <'wlan0'>, 'Driver': <'nl80211'>}", allow_failure=args.expect_crash)
            if args.expect_crash:
                time.sleep(0.2)
                if created.returncode == 0 or wpa.poll() != -signal.SIGSEGV:
                    raise RuntimeError('Expected D-Bus segmentation fault was not reproduced')
                result = {'state': 'expected_segv_reproduced', 'wpa_exit': wpa.poll()}
            else:
                call(iface, 'fi.w1.wpa_supplicant1.Interface.Scan',
                     "{'Type': <'active'>, 'Channels': <[(uint32 2412, uint32 20)]>}")
                time.sleep(3)
                bss = call(iface, 'org.freedesktop.DBus.Properties.Get',
                           'fi.w1.wpa_supplicant1.Interface', 'BSSs')
                if '/BSSs/' not in bss.stdout:
                    raise RuntimeError('No virtual access point discovered')
                call(iface, 'fi.w1.wpa_supplicant1.Interface.AddNetwork',
                     "{'ssid': <'CartridgeVMTest'>, 'psk': <'CartridgeTestPassword'>, 'key_mgmt': <'WPA-PSK'>, 'scan_freq': <'2412'>}")
                call(iface, 'fi.w1.wpa_supplicant1.Interface.SelectNetwork', iface+'/Networks/0')
                connected = False
                for _ in range(12):
                    time.sleep(0.5)
                    state = call(iface, 'org.freedesktop.DBus.Properties.Get',
                                 'fi.w1.wpa_supplicant1.Interface', 'State')
                    if "'completed'" in state.stdout:
                        connected = True
                        break
                if not connected or wpa.poll() is not None:
                    raise RuntimeError('Virtual WPA2 handshake did not complete')
                result = {'state': 'scan_and_wpa2_connection_passed', 'hardware_verified': False}
            result['wpa_sha256'] = hashlib.sha256(args.wpa.read_bytes()).hexdigest()
            print(json.dumps(result, indent=2))
            (args.output/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        finally:
            for process in (wpa, ap):
                if process is not None:
                    if process.poll() is None:
                        process.terminate()
                    try:
                        process.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
            if bus_pid is not None:
                try:
                    os.kill(bus_pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
            (args.output/'transcript.json').write_text(json.dumps(transcript, indent=2)+'\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wpa', type=Path, required=True)
    parser.add_argument('--loader', type=Path)
    parser.add_argument('--library-path', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expect-crash', action='store_true')
    args = parser.parse_args()
    if args.loader and not args.library_path:
        parser.error('--loader requires --library-path')
    try:
        run(args)
    except Exception as exc:
        parser.exit(1, 'VM test failed: '+str(exc)+'\n')
