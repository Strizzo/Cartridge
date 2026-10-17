# WPA package recovery on the R36S Plus

The 2026-09-26 ARM VM test reproduced the handheld's WPA segmentation fault
without its USB adapter. The saved `wpa_supplicant` starts its D-Bus service,
then crashes on `fi.w1.wpa_supplicant1.CreateInterface`. GDB stops in
`wpas_dbus_getter_bss_expire_age`. Disassembly contains invalid instructions,
uninitialized register accesses and branches beyond executable code. A nearby
getter and the debug information are damaged too.

This is a damaged executable in the recovered firmware image, not merely a
failed scan. The old card had previously returned inconsistent reads and
contained damaged systemd files. Copying that image onto a new card also copied
this damage. Filesystem integrity checks do not verify executable contents.
The physical card's binary and dependencies still need comparison with that
image before applying the prepared repair.

## Verified replacement

The firmware's package database records `wpasupplicant` version
`2:2.9-1ubuntu2`, while its damaged executable reports a custom 2.10 build.
The original Ubuntu package's binaries match both MD5 values in the firmware's
package database. This repair restores those original binaries; it does not
modernize the underlying distribution or add WPA3 capabilities.

Provenance:

- [Original package](https://old-releases.ubuntu.com/ubuntu/pool/main/w/wpa/wpasupplicant_2.9-1ubuntu2_arm64.deb)
- [Signed Eoan Release metadata](https://old-releases.ubuntu.com/ubuntu/dists/eoan/InRelease)
- [ARM64 package index](https://old-releases.ubuntu.com/ubuntu/dists/eoan/main/binary-arm64/Packages.xz)

The Release signatures were verified with Ubuntu's 2012 and 2018 archive keys.
The compressed index matches its signed SHA256. The package SHA256 matches
that index: `fc1aa77412b3deb9fbe3cfd8f47b1e267a81bb0f38f6cc5bdb41371ea630c508`.

| File | Known damaged SHA256 | Original package SHA256 |
| --- | --- | --- |
| wpa_supplicant | a8352f9830fe588fd36363db66b215c48c457f1db7981e5507a1f3339189cb0a | 5cf389efbcd8dbab03522d3b916852f3e5efc94e049a8878ec62632dc90c0a9c |
| wpa_cli | 4e218046e63994cb7c6ea128a2d52c7845a2b01371304bc4939b0a31068459bf | f90c690ada84e973f2b7e632fdafcf62e952a9050d234867011c4097f0e6d6cf |

The clean WPA binary, using the image's original loader and shared libraries,
passes D-Bus interface creation, access-point scanning, and a WPA2 handshake
against a virtual AP. The unchanged damaged binary fails the same check with
SIGSEGV. This proves the software failure and replacement behavior; the real
RTL8188EU radio, home-network connection, and subsequent reboot remain to test.

The preceding service trial installed no persistent override. Its standard
command included `-s`, which the custom damaged build does not support; it
prints help and exits 0. Its exit was not proof of a second service failure.

## Device repair

Prepare the payload on the developer computer:

```sh
python3 deploy/wifi/fetch-wpa-package.py --output /tmp/cartridge-wpa-payload
```

Stage `restore-wpa-package.py` and `payload/{wpa_supplicant,wpa_cli}` under
`Cartridge/wifi-package-repair/` on EASYROMS. Copy `wpa-package-repair.sh` into
`tools/Cartridge WiFi Package Repair.sh`. The tool runs the helper with sudo
on the handheld, retains a log, and waits on a result dialog before returning.
Do not use macOS debugfs to write the live Linux filesystem.

The helper accepts only the recorded damaged hashes or the exact clean hashes,
checks the original package database and payload checksums before changes,
and verifies that the clean executable loads with the device's libraries.
It backs up changed binaries with owner/mode metadata on the Linux partition,
then replaces them atomically, checks the installed hashes, and restores both
originals on a write or verification failure. Wi-Fi profiles, services, drivers,
kernel, bootloader, and games are outside its write paths. Restart is requested
only when explicitly selected by the device tool. Restoring binaries is not
reported as a successful hardware connection.

For an offline rehearsal, `--root DIR` applies the file operation to a fixture
root and never runs host services. `--verify-only` performs no writes.
Guard, backup, rollback, symlink, idempotence and preservation tests are in
`tests/test_wpa_package_repair.py`.

## Reproduce without card swaps

Inside the isolated ARM Linux VM, install `hostapd`, `iw`, `libglib2.0-bin`,
and the matching kernel's modules-extra package. Create virtual radios with
`sudo modprobe mac80211_hwsim radios=2`. `scripts/wpa_vm_check.py` refuses radios
whose sysfs paths are not virtual `mac80211_hwsim` devices.

Use the read-only card inspection's `runtime` folder, containing WPA and its
recursive dependencies, or equivalent files extracted read-only from the
saved root image. Then run in the guest:

```sh
sudo python3 wpa_vm_check.py --wpa runtime/wpa_supplicant \
  --loader runtime/ld-linux-aarch64.so.1 --library-path runtime \
  --output /tmp/wpa-damaged --expect-crash
sudo python3 wpa_vm_check.py --wpa payload/wpa_supplicant \
  --loader runtime/ld-linux-aarch64.so.1 --library-path runtime \
  --output /tmp/wpa-clean
```

The script uses a private D-Bus instance and a virtual WPA2 AP with a dummy
password. It saves the transcript/results and terminates its processes even
on failure. This tests the actual ARM executable and libraries, without
pretending to emulate the handheld's RF hardware or kernel driver timing.
