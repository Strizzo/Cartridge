# Browser engine source sidecar

`scripts/collect_browser_sources.py` collects the exact source artifacts for the
prepared ARM64 browser component into a separate directory. The engine is read
only. The collector does not extract sources, compile, sign, publish, install,
access a signing key, change system APT configuration, or touch a device/card.

The sidecar is a local provenance collection. It does **not** assert license
compliance or constitute a public source distribution. Prepare and review source
metadata before distributing a public component. This work does not block a
private local installation.

## Exact inputs and authentication

The collector compares `packages.tsv` against the engine's dpkg database and
checks `source-packages.tsv` against dpkg's `source:Package` / `source:Version`
mapping. It retains binary versions separately: a binary NMU suffix such as
`+b1` is not silently substituted into, or stripped from, a source version.
Epochs are retained exactly. Missing exact versions stop planning; there is no
latest-version, upstream-tag, or different-distro fallback.

Debian packages use authenticated `bookworm/main` and
`bookworm-security/main` source indexes from these official archives:

- [Debian archive](https://deb.debian.org/debian/)
- [Debian security archive](https://security.debian.org/debian-security/)

The static bubblewrap helper's installed Ubuntu `libc6`, `libc6-dev`, `libc-bin`,
`libcap2` and `libcap-dev` source identities are queried separately. They use
authenticated `noble`, `noble-updates` and `noble-security` main source indexes
from the [Ubuntu ports archive](https://ports.ubuntu.com/ubuntu-ports/).
`libc.a` and `libcap.a` are hashed as build-input evidence without copying those
archives or rebuilding the helper. These are installed-package provenance;
the collector does not prove historical link inputs or reproduce the binary.

Both distributions get independent APT `Dir`, configuration, source list,
state/status/lists, cache and log directories inside the sidecar. `APT_CONFIG`
points at that isolated configuration before APT reads its usual configuration
fragments. Only `deb-src` entries are used; the VM's `/etc/apt` and global package
state remain untouched. Debian and Ubuntu archive keyrings already installed in
the VM are copied into the corresponding isolated APT configuration. APT must
successfully authenticate **all** indexes. Signed `InRelease`, compressed Sources
indexes, archive keyrings, APT update output and `gpgv` diagnostics are retained.
An additional unknown rollover signature is allowed only alongside a valid
signature from the supplied archive keyring; bad/revoked/expired signatures fail.

Every file in each exact source record's `Checksums-Sha256` is retained, including
the `.dsc`, all `orig` tarballs, additional components such as Chromium's
`orig-pre-gen`, Debian/Ubuntu changes and any detached signatures. APT's exact
`--print-uris --only-source package=version` result must agree on filenames,
sizes, strong hashes and an official archive pool path. Ubuntu's preferred
SHA-512 URI hash is checked against the same authenticated source record; the
lock and download verification always include SHA-256. Identical uploads in
main/security pools may have different URLs but must have identical artifacts.

Bubblewrap uses the original
[upstream 0.13.1 release tarball](https://github.com/containers/bubblewrap/releases/download/v0.13.1/bubblewrap-0.13.1.tar.xz),
129,472 bytes, with the builder's SHA-256 pin:

```text
0e85c8702248b8dd64144f594147553650cdd0e4c951690345faace79c217c4d
```

Supply that local tarball to `collect`; it is checked before any downloads. This
pin records the existing builder's trust decision, not an archive signature.

## Run in the isolated VM

Planning and downloading require Linux/aarch64 and the
`/etc/cartridge-compat-vm` marker. Use an unprivileged account, Python 3.9 or
newer, APT, `dpkg-query`, `gpgv`, HTTPS certificates and installed Debian/Ubuntu
archive keyrings. Output must be a fresh nonsymlink path under `/tmp` in the VM.
Copy the collector, build recipe and small original bubblewrap tarball there;
do not copy or rebuild the engine.

```sh
python3 /tmp/cartridge-browser-source-tools/collect_browser_sources.py plan \
  --component /opt/cartridge-browser-engine-v1 \
  --output /tmp/cartridge-browser-corresponding-sources \
  --build-recipe /tmp/cartridge-browser-source-tools/build-arm64.sh

python3 /tmp/cartridge-browser-source-tools/collect_browser_sources.py collect \
  --output /tmp/cartridge-browser-corresponding-sources \
  --bubblewrap-archive /tmp/cartridge-browser-source-tools/bubblewrap-0.13.1.tar.xz

python3 /tmp/cartridge-browser-source-tools/collect_browser_sources.py verify \
  --output /tmp/cartridge-browser-corresponding-sources
```

Before fetching indexes, the planner requires 512 MiB free for metadata and
reserve. It then totals the exact compressed source sizes before any archive
downloads and refuses if the full set plus a 256 MiB reserve does not fit.
Downloads are sequential and streamed; no source extraction, duplicate staging
copy or large in-memory archive is needed. The lock caps each artifact at 2 GiB
and the total payload at 4 GiB. Collection rechecks remaining space before
starting and while proceeding. Run the parent copy only after verification;
retain the tree directly instead of creating another large tar archive in the VM.

`plan` refuses existing output. On unresolved versions it writes
`unresolved.json` without downloading any source archives. Inspect that report;
do not change requested versions to make a release pass. A failed metadata-only
plan can be removed after confirming its path and rerun into a fresh directory.
`collect` can resume against the unchanged lock: existing artifacts must already
match their size and SHA-256 or it stops. Normal download failures remove only
that download's partial file. An interrupted process may leave a `.partial`;
inspect/remove that specific partial before retrying. Final files are never
replaced.

## Sidecar layout and verification

- `sources.lock.json`: immutable unsigned index of each URL, SHA-256, byte size,
  exact source version, binary/source mapping and provenance evidence.
- `artifacts/debian/`, `artifacts/ubuntu/`, `artifacts/upstream/`: original source
  artifacts, kept compressed.
- `records/`: exact selected Sources paragraphs and APT URI output.
- `apt/`: isolated metadata, copied keyrings and authentication diagnostics.
- `evidence/`: original component/package/source inventories, generated
  binary/source mappings, static archive hashes, build recipe and notices.
  Engine `notices/` and Debian `usr/share/doc/*/copyright` files are preserved;
  notice aliases are materialized only after resolving inside the engine.
- `SHA256SUMS`: written only after every payload and evidence file passes
  verification. It covers the lock itself too.

`verify` checks all bytes, size accounting, complete artifact membership,
binary/source mappings and source-record/URI agreement. It rejects symlinks,
absolute/traversing paths, unlisted files and altered versions. It can run on a
copied bundle on the Mac without APT, network access or the VM marker:

```sh
python3 scripts/collect_browser_sources.py verify --output /absolute/copied/tree
```

Keep the reported lock SHA-256 separately when transferring it. The lock and
`SHA256SUMS` are unsigned integrity records; an attacker able to replace the
entire collection could replace both. Authentication is established during the
APT plan using the VM's preinstalled archive trust, whose evidence is preserved.

Offline policy/checksum tests:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -p test_browser_sources.py
```

## Bounded collection for the finalized local engine

The 2026-10-27 plan covers 176 Debian source packages, Ubuntu
`glibc=2.39-0ubuntu8.9`, Ubuntu `libcap2=1:2.66-5ubuntu2.4`, and bubblewrap 0.13.1.
The 576 source artifacts total **1,686,009,203 bytes**. The Chromium source is
`156.0.8078.47-1~deb12u1`; its four source artifacts total 819,927,341 bytes.
The Ubuntu glibc and libcap2 source sets total 19,022,648 and 208,635 bytes,
respectively. Complete package/source mappings and download URLs live in the
sidecar lock; they are specific to this engine, not a floating release recipe.

The sidecar destination is
`cartridge-arm64:/tmp/cartridge-browser-corresponding-sources`. All 576 artifacts
were downloaded and verified, with zero exact-source misses and zero remaining
downloads. The full collection is **1,721,549,103 bytes**, including 34,964,138
bytes of evidence, the 434,560-byte lock and 141,202-byte `SHA256SUMS`. It retains
267 notice files and maps 265 Debian binary packages to 176 source packages.
All 16 offline tests passed on both macOS Python 3.9 and ARM64 Ubuntu Python.
The parent can copy the verified tree once; no large source artifacts belong in
this Git repository. All of these artifacts remain local and unpublished.

The locked source plan SHA-256 is:

```text
ccf36b71bd206cd40daa1bfed3f10ddbc9dfd2d758f45e9906c66e9c59d5b61e
```

A final read-only check found that the parent refreshed three certificate
aliases from symlinks to regular files while downloads ran. Both package
inventories and the Debian/Ubuntu dpkg source mappings remained byte-identical.
The retained `evidence/component.json` is deliberately the plan-time snapshot
(`b22e8d12c7fd88cd3985e7c113df8b9a1a0c6b095896018e39ab553ed6d70ec8`);
the component inventory observed at handoff was
`49ebbce27de55a941e6cca9a49ed3d46d93f5e574969740885388681a3718066`.
The source collector did not modify the engine. The unchanged exact source
mapping still selects the same complete artifact set.

Before public distribution, review the collected source sets against the exact
shipped component and licenses; retain notices, local modifications and build
instructions; prepare any required relinking material for the static helper;
and arrange the applicable source delivery/availability with actual public URLs.
The collector neither performs that review nor establishes a public delivery
mechanism. No public source URL, legal conclusion, or public distribution is
claimed by this local sidecar.
