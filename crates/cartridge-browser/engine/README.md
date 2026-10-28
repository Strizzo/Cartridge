# Private ARM64 engine component

This component supplies a recent Chromium and its libraries without changing the
handheld's kernel, system libraries or boot configuration. It is a development
component, not included in the stable OS or Store catalogue.

Build in a disposable ARM64 Linux environment as root (not on a mounted card):

```sh
# Debian/Ubuntu build tools: mmdebstrap debian-archive-keyring meson ninja-build
# build-essential libcap-dev curl xz-utils python3
sudo ./build-arm64.sh /opt/new-browser-engine
```

Debian APT validates signed bookworm/security packages. Bubblewrap source is
versioned and its SHA-256 is pinned in the builder. The static ARM64 helper keeps
upstream's old-kernel fallbacks. `prepare_component.py` strips privileged mode
bits, removes build-time /dev nodes and machine DNS/host data, rebases chroot
symlinks for safe extraction, and writes a complete file hash inventory plus
package versions and copyright notices. Do not run the finalizer on an unrelated
filesystem; its build marker is required. `hardware_validated` remains false.

The resulting directory contains `chromium`, `bubblewrap`, `rootfs/`, `notices/`,
`packages.tsv` and `component.json`. Keep the whole directory together on a Linux filesystem that supports symlinks
(such as the handheld root partition); the exFAT games partition is unsuitable. The wrapper
runs as a normal user, mounts its root read-only, exposes only a private profile
and temporary storage for writes, and uses existing network connectivity.
Chromium's namespace and seccomp sandboxes remain active. Root execution and
sandbox/certificate/security bypass flags are refused.

A generic ARM VM verifies userspace execution, not RK3326 drivers or performance.
The actual kernel must support unprivileged user namespaces; do not work around a
failed check with `--no-sandbox`. Ubuntu 24's AppArmor may require a scoped `userns`
permission for this exact root-owned bubblewrap path in the disposable VM. Do not
disable global AppArmor or unprivileged-userns restrictions for this test.

The engine is about 0.94 GiB on ext4 with hardlinks, about 1.8 GiB as independent
regular files. It needs a separate, verified component install/update mechanism;
the runtime OTA packager intentionally excludes it. A public engine release also
needs retained notices and corresponding-source distribution for covered code.

Host-independent packaging invariants:

```sh
python3 -m unittest discover -s crates/cartridge-browser/engine -p 'test_*.py'
```

Before copying an engine to a handheld, `python3 preflight.py` reports its kernel,
RAM, free profile storage and unprivileged namespace support without changing
system configuration. Run it as the actual Cartridge user. A VM report is marked
as a simulation. A successful preflight is not a device performance result.
