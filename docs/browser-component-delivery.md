# Verified browser component delivery

The engine has an independent signed schema and limits. System OTA formats,
limits, key files, browser wrapper and engine code are unchanged. No command
executes Chromium, modifies OS libraries/boot/games, or disables a sandbox.

## Artifact and provenance contract

`browser-component.json` is the existing exact UTF-8 Ed25519 envelope:
`{"key_id":"cartridge-os-v1","payload":"<JSON text>","signature":"<128 lowercase hex>"}`.
The production CLI/library pin `scripts/system-update-public-key.hex`. There is no
runtime alternate-key, unsigned-engine, or sandbox-bypass option.

Payload fields (all required; unknown/duplicate fields rejected):

```json
{
  "schema": 1,
  "component": "browser-engine",
  "target": "r36s-plus-aarch64",
  "version": "1.0.0",
  "revision": "<40 lowercase hex build revision>",
  "min_runtime": "0.8.0",
  "chromium_version": "<exact Debian binary version>",
  "source_distribution": "metadata-only; corresponding-source distribution not asserted",
  "provenance": {
    "package_list": {"path":"packages.tsv","size":123,"sha256":"<64 hex>"},
    "build_inventory": {"path":"component.json","size":456,"sha256":"<64 hex>"},
    "sources": [
      {"role":"index","url":"https://example.org/source-index.json","size":789,"sha256":"<64 hex>"},
      {"role":"archive","url":"https://example.org/bubblewrap.tar.xz","size":987,"sha256":"<64 hex>"}
    ]
  },
  "archive": {
    "url":"https://example.org/browser-engine-r36s-plus-1.0.0.tar.gz",
    "size":1234,"sha256":"<64 hex>","stored_bytes":1000,"logical_bytes":1800
  },
  "entries": [
    {"path":"bubblewrap","type":"file","size":123,"sha256":"<64 hex>","mode":365},
    {"path":"rootfs","type":"directory"},
    {"path":"rootfs/link","type":"symlink","target":"usr/lib/chromium/chromium"}
  ]
}
```

The example omits required files; use the packager, not handwritten JSON.
`entries` contains the entire sorted tree, including empty directories, wrapper,
helper, `component.json`, build marker, packages, notices and rootfs copyright
files. File modes are decimal 292 (0444) or 365 (0555); directories are 0555.
A hardlink entry is `{"path":"...","type":"hardlink","target":"<earlier regular path>"}`.
Its bytes count toward `logical_bytes`, not `stored_bytes`. Paths use printable UTF-8 Unix names, at most 255 bytes and 32 components.
Literal backslashes remain filename bytes, as in Debian systemd units; only `/`
separates components. USTAR paths and link targets must be
representable without GNU/PAX extensions (link targets at most 100 bytes).
Special files, privileged files, absolute/escaping/cyclic links, symlink parents,
duplicates and extra archive/tree entries are rejected. Safe dangling links are
allowed for private-rootfs mount points.

Package versions are preserved in the signed `packages.tsv` and original build
inventory. The two provenance file references must match their entry hashes and
sizes. `sources` may be empty. It may reference an exact package-to-source index
and source archives with HTTPS URLs, sizes and SHA-256. These are signed
references, **not evidence of availability or distribution compliance**. Install
never fetches or executes source artifacts. Collect and publish corresponding
sources separately before claiming that obligation is fulfilled.

## Build and sign (parent-owned signer)

After the parent updates the wrapper and regenerates `component.json`:

```sh
python3 scripts/package_browser_engine.py \
  --component /path/to/prepared-engine --output /path/to/artifacts \
  --version 1.0.0 --revision <40-hex-build-revision> \
  --url https://HOST/browser-engine-r36s-plus-1.0.0.tar.gz \
  --sources-json /path/to/source-references.json
```

`--sources-json` is an optional JSON array of the `sources` records above. The
packager verifies the prepared hashes/modes, preserves hardlinks, retains all
notices, writes a deterministic USTAR gzip and `browser-component.unsigned.json`.
This is not installable until signed. No signing secret is needed to build.

```sh
OPENSSL=/path/to/openssl3 python3 scripts/package_browser_engine.py \
  --sign /path/to/artifacts/browser-component.unsigned.json \
  --sign-key /parent/controlled/key.pem
python3 scripts/package_browser_engine.py --verify /path/to/artifacts/browser-component.json
```

Signing checks the result against `system-update-public-key.pem` before creating
the trusted manifest. `--public-key` exists only in the host packaging tool for
test fixtures; production installation never accepts it. Tests generate separate
test keys. A local artifact and HTTPS artifact require identical signatures.

## CLI and activation contract

Build `cargo build --release --bin browser-component`. Read-only artifact
inspection and verification work on macOS; persistence requires local Linux
ext4/XFS/Btrfs/overlay semantics, not exFAT/FAT, network storage or tmpfs.

Run persistent commands as the owning Cartridge user and stop the browser first.
`USER_HOME` must already exist and be owned by that effective user; storage is
always `<USER_HOME>/.cartridges/components/browser-engine`. The default is
`CARTRIDGE_HOME`, then `HOME`. OS/boot/games/mounted-media homes are refused, and
symlinked storage directories are refused. An arbitrary destination directory is
not accepted. Parent activation scripts should run as that user, not change the
user's HOME to a boot or games directory.

```sh
browser-component inspect --manifest /artifacts/browser-component.json
browser-component verify --manifest /artifacts/browser-component.json \
  --archive /artifacts/browser-engine-r36s-plus-1.0.0.tar.gz
browser-component verify --manifest /artifacts/browser-component.json --source /private/prepared-engine
browser-component adopt --home /home/USER \
  --manifest /artifacts/browser-component.json --source /private/prepared-engine
browser-component status --home /home/USER
browser-component verify --home /home/USER
browser-component install --home /home/USER \
  --manifest /artifacts/browser-component.json --archive /artifacts/browser-engine-r36s-plus-1.0.0.tar.gz
browser-component install --home /home/USER --url https://HOST/browser-component.json
browser-component rollback --home /home/USER
```

`adopt` requires only the signed manifest and complete prepared tree on the same
filesystem as storage. It verifies every file/link and refuses external regular
hardlinks, seals the tree read-only, records the signature, and **renames**, never
copies, it into `releases/<full-signed-payload-sha256>`. Ownership must already
match the invoking user throughout the tree. Adoption makes the source root
private before verification; validation failure leaves the source in place.
If the old tree occupies the stable discovery directory itself, first rename
that directory to a sibling on the same filesystem, then use that sibling as
`--source`; adopting a directory containing the store is refused.
The original build inventory is retained unchanged even though installed file
modes are normalized read-only. About manifest size plus 4 MiB of metadata
reserve is required, rather than another copy of the 0.94 GiB engine. No archive
needs to be placed on the handheld. A cross-filesystem source fails explicitly.

Reissuing the same adopt command after a move/activation interruption recovers
and verifies an already-moved release even if the original source path no longer
exists. If the source still exists and the release is already installed, the
verified installed release is reused; no source is deleted.

`install` streams a bounded, hashed compressed archive directly into private
`.incoming`, hashes every file body, preserves hardlinks and reuses immutable
identical files from verified current/previous releases. It never stages a second
compressed copy on the root partition. It reserves space for new unique files,
manifest, entry metadata and 32 MiB before extraction. If a large update exceeds
available space, it fails without changing current/previous; 889 MiB free cannot
guarantee space for an entirely different 0.94 GiB engine plus rollback.

Install/adopt requires a newer version than current, or the identical signed
payload for an idempotent retry. Downgrades use explicit rollback.

A synced generation holds both release pointers. Atomic rename of one `state`
symlink commits both together; the stable links are:

```text
chromium -> current/chromium
current  -> state/current
previous -> state/previous
state    -> generations/<state-hash>
generations/<state-hash>/current  -> ../../releases/<payload-hash>
generations/<state-hash>/previous -> ../../releases/<previous-payload-hash>
```

There is no trial/health promise: activation verifies bytes, not device audio,
GPU, namespace support or performance. Parent owns those checks and can call
rollback. Rollback fully verifies the previous tree before switching; it swaps
current/previous when both are good. A damaged current is not retained as a
rollback candidate. Shared files are read-only, but the store is not a security
boundary against its owning user. Parent should keep the source tree quiescent
during adoption and must not edit an installed release in place.

Abandoned staging is cleaned on the next install. Unreferenced committed releases
and generation records are retained for explicit recovery, not automatically
deleted. This can consume space after many updates; parent may remove retired
releases only while the browser is stopped, under the operation lock, and after
resolving current/previous. There is no automatic garbage collection in this API.

## Library surface and tests

`cartridge_net::browser_component` exports `verify_manifest`, `inspect_local`,
`inspect_https`, `verify_archive`, `VerifiedComponent::{release,id,signed_manifest,
verify_directory}` and `BrowserComponents::{from_environment,at_home,root,inspect,
status,verify_current,install_local,install_https,adopt,rollback}`. Installs accept
a progress callback. Public verified objects cannot be forged by mutating display
fields. Mutation operations serialize with a private advisory file lock. Errors
after a committed state switch may mean activation completed; inspect status and
retry the same command. No code auto-starts a browser.

```sh
cargo test -p cartridge-net browser_component --offline
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests -p test_browser_engine_package.py -v
```

Tests cover generated test-key interoperability between Python and Rust,
signature/hash tampering, archive extensions/special files/trailers, path and link
escapes, external hardlinks, inventory scale above 17,772 entries, preservation
and reuse of immutable inodes, rollback, serialized writes and interruptions
before/after rename/activation. macOS unit tests exercise filesystem algorithms
with an internal test-only platform allowance; production persistence has no
such bypass. Device filesystem/power-loss behavior requires parent validation.
