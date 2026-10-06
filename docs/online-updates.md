# CartridgeOS updates over Wi-Fi

CartridgeOS 0.6.1 introduces signed CartridgeOS runtime updates for the R36S Plus
(`r36s-plus-aarch64`). These update Cartridge, its game-library helper, registry,
fonts, icons, controller mappings and bundled Lua apps. They do not update the
Linux OS, kernel, drivers, BOOT graphics, session supervisor or setup scripts.
Independent Store app updates retain their [separate signed protocol](app-store.md).

## First installation and everyday updates

The first bootstrap is **offline**: install a reviewed, trusted 0.6.1-or-newer
device bundle and its updated session supervisor using the existing installation
procedure. An older launcher or supervisor cannot bootstrap this protocol through
OTA. The base `Cartridge/cartridge` embeds the public verification key and remains
the trusted verifier for later releases. Do not replace that base with a downloaded
candidate. A future change to the verifier, key or supervisor requires another
trusted offline bootstrap.

After bootstrap, use Settings to check for an update, review its version, download
size and notes, and confirm staging. Confirmation authorizes the download and
staging; it does not replace the running application. Restart Cartridge to try the
pending release. Checking alone does not authorize staging or a surprise restart.
An unsigned CI artifact is not an installable update and must never be presented
as one. Only a release with a reviewed archive and its locally signed
`system-update.json` is an installable update. Creating unsigned CI artifacts
does not enable device downloads. The 0.6.3 release adds independent stick input
and uses the existing 0.6.1 trust and staging protocol; it does not require
replacing the working offline bootstrap or session supervisor.

Use reliable Wi-Fi and external power or at least 30% battery. Staging and
restart require charging or a known battery level of at least 30%; an unknown
battery level requires the charger. Leave
room for the compressed download, the unpacked candidate, the active and previous
releases, and temporary files. Do not remove the card or cut power during staging
or the first restart. If Settings reports unsuitable power conditions, insufficient
space, a network failure or a verification error, resolve it before retrying.
The archive cap is 128 MiB and the unpacked file cap is 384 MiB; these are separate
limits, not a promise that 384 MiB of free space is sufficient.

This update path does not run boot handover or setup, change a system service,
rewrite a partition or modify ROMs and saves. Boot/system updates require their
own recovery process outside this protocol.

## Release and trust contract

The public archive URL is fixed by the version:

```text
https://github.com/Strizzo/Cartridge/releases/download/vVERSION/cartridgeos-r36s-plus-VERSION.tar.gz
```

The reviewed signed manifest is named `system-update.json`. Only a manually
reviewed, signed envelope may be attached as the trusted endpoint:

```text
https://github.com/Strizzo/Cartridge/releases/latest/download/system-update.json
```

Its JSON envelope contains exactly `key_id`, `payload` and `signature`:

```json
{"key_id":"cartridge-os-v1","payload":"EXACT_JSON_STRING","signature":"128 hex characters"}
```

The Ed25519 signature covers the exact UTF-8 bytes of the payload string. It does
not cover a reserialized or pretty-printed interpretation. Public verification
material lives in `scripts/system-update-public-key.pem` and
`scripts/system-update-public-key.hex`; no private key belongs in the repository,
bundle, archive, workflow or uploaded artifact. OpenSSL 3 is required for the
local signing/verification tool; select its executable with `OPENSSL` if needed.

The payload has these fields:

| Field | Contract |
| --- | --- |
| `schema`, `channel` | `1`, `stable` |
| `version` | Stable semantic version, without prerelease or build metadata |
| `revision` | Full 40-character lowercase hexadecimal Git revision |
| `target` | `r36s-plus-aarch64` |
| `min_runtime`, `min_supervisor` | `0.6.1`, `1` |
| `notes` | At most 8000 UTF-8 bytes |
| `archive` | Public versioned URL, compressed `size`, lowercase `sha256`, and sum of file sizes as `unpacked_size` |
| `files` | Sorted entries containing `path`, `size` and lowercase `sha256` for every archived file |

The manifest is limited to 4 MiB and 8192 files. Paths are relative ASCII names
of at most 220 bytes and 12 components, using only letters, digits, `-`, `_` and
`.` within components. No component starts with a dot; absolute paths, empty
components, `.`/`..`, backslashes and symlinks are forbidden. The packager also
rejects trailing dots and paths USTAR cannot represent without extensions.

Only these paths may appear:

- `cartridge`, `game-library.py`, `registry.json`;
- `assets/gamecontrollerdb.txt`;
- `assets/fonts/**`, `assets/icons/**`, `assets/overlays/**`, `assets/brand/**`;
- `lua_cartridges/**`.

The three root files and at least one `assets/fonts/**.ttf` font are required.
The packager includes the allowed Lua tree when present. It excludes boot logos,
setup and supervisor code, diagnostics, hidden files, known credential locations,
key files, logs and build tooling even if placed below an otherwise allowed
directory. Review the input bundle for sensitive content before packaging; file
names alone cannot identify a secret disguised as an ordinary asset.

The gzip-compressed archive contains only regular USTAR file entries, without
directory entries, links, devices, PAX/GNU extensions or metadata manifests.
Members are sorted; owner IDs, timestamps and owner names are normalized; the
launcher mode is `0755`, other modes are `0644`; the gzip timestamp is zero and
its original filename is absent. Identical input bytes and release metadata
produce identical output with the same packaging toolchain. The ARM binary is
checked as a little-endian ELF64 AArch64 executable and is never executed by the
packager.

## Building and reviewing artifacts

Both CI workflows package OTA files separately from the ordinary offline device
bundle. The bootstrap bundle retains the base launcher, updated supervisor and
setup tools, and now also includes icons and `gamecontrollerdb.txt`. The
`cartridge-system-update-aarch64-unsigned` Actions artifact contains only:

```text
cartridgeos-r36s-plus-VERSION.tar.gz
system-update.unsigned.json
```

CI does not sign or publish the trusted OTA endpoint. The existing tagged-release
workflow prepares a draft with its offline installation zip and the matching
ARM check binaries as a CI artifact. Rehearse that exact signed candidate in
the isolated ARM VM before attaching the trusted endpoint and publishing the
draft. No signing key or
signing secret is configured in either workflow.

To reproduce packaging from a device bundle's `Cartridge` directory:

```sh
python3 scripts/package_system_update.py \
  --bundle bundle/Cartridge --output system-update \
  --version 0.6.1 --revision "$(git rev-parse HEAD)" \
  --notes-file release-notes.txt
python3 scripts/package_system_update.py \
  --verify system-update/system-update.unsigned.json
```

`--notes-file` is optional. The requested version must match the checkout's
`[workspace.package]` version in `Cargo.toml`; `--cargo-manifest PATH` selects a
different build manifest. For a separately obtained binary, `--build-version
VERSION` supplies the version from its trusted build provenance. This is an
explicit provenance assertion, not proof extracted from or by executing the
binary. Inspect the CI revision, build inputs and artifact provenance as part of
release review. Output paths must be outside the bundle, and existing artifacts
are not overwritten.

For release preparation, download and review the CI artifact, verify it locally,
then sign its existing payload with the offline key:

```sh
# Run only on the release signer's machine after review; never in CI.
OPENSSL=/path/to/openssl3 python3 scripts/package_system_update.py \
  --verify system-update/system-update.unsigned.json \
  --sign-key /secure/offline/cartridge-os-v1.pem
OPENSSL=/path/to/openssl3 python3 scripts/package_system_update.py \
  --verify system-update/system-update.json \
  --public-key scripts/system-update-public-key.pem
```

The first command verifies the existing archive and its per-file hashes,
preserves the existing payload string exactly, and verifies the new signature
against the public key before writing `system-update.json`. It does not rebuild
or modify the CI archive and does not need a copy of the original device bundle.
Pass `--public-key PATH` if the signer is using a separate public-key location.
The private key path is passed directly to OpenSSL; the packager does not read or print the private key. `--sign-key` may also be supplied
during a local build. No command above uploads anything. After signed verification
and release-readiness review, attach the archive and signed manifest to the matching
GitHub release. Never rename an unsigned payload to `system-update.json`, upload
the key, or publish an unsigned file as the trusted endpoint. Verification of an
unsigned payload checks consistency only; it does not authenticate a release.

## Activation, state and recovery

The base application root remains stable. Candidates live at
`<root>/releases/<version>-<first-12-revision-characters>`; IDs are ASCII safe and
at most 100 characters. A staged release is immutable. The updater persists its
verified envelope as `<release>/system-release.json` after verification; that
file is never supplied by the archive.

State lives at `<state-dir>/system-update.json` on the Linux home filesystem,
not in the SD application's root. Schema 1 contains `active`, `previous`,
`pending`, `trial` and `last_result`. Release references default to `null`, and
`last_result` to an empty string, when the file is missing. All participants use
the advisory `system-update.lock` flock and atomic tempfile writes with file
fsync, replacement and parent-directory fsync. Unknown/incompatible state schemas
must not be overwritten.

Staging sets `pending` without changing `active`. On the next restart the
supervisor records a trial and verifies the selected directory by running:

```text
<root>/cartridge system-verify --path <release-dir>
```

The trusted base verifies the signed manifest and every file hash. The candidate
is never executed to verify itself. The supervisor exports
`CARTRIDGE_UPDATE_ROOT=<base>`, `CARTRIDGE_UPDATE_STATE=<state-dir>`,
`CARTRIDGE_UPDATE_TARGET=r36s-plus-aarch64` and `CARTRIDGE_UPDATE_SUPERVISOR=1`.
Its child working directory and `CARTRIDGE_ASSETS` match the selected release.
Exit code 40 requests a restart for activation.

Promotion requires a successful first frame **and five further seconds alive**.
A failed startup, a crash during that grace period, or an interrupted trial can
roll back to the previous verified release, with the trusted base and
existing EmulationStation recovery available. First-frame acknowledgement plus
the alive grace period is a startup health check, not proof that every app feature works. Keep both the previous
release and the base installation; this protocol never deletes them.

## Validation status

Packaging tests exercise reproducibility, hashes, architecture/version checks,
path and size limits, exclusions, malformed archives, and the signature interface.
They do not constitute handheld validation. The native Mac simulator supports
previewing the UI and checking a signed release; production staging requires
Linux AArch64. Use the ARM VM for staging/restart integration, including a
candidate built by CI and signed locally with the production release key. A 0.6.1 candidate built from the exact CI artifact can be staged manually
in the VM to exercise the trusted verifier, real SDL first-frame acknowledgement,
the five-second alive grace period and rollback without publishing a GitHub release. This does not enable production
downgrades or bypass device bootstrap. The simulator session keeps state outside
the application root, and its wrapper forwards verification arguments to the trusted base verifier.

`sim/vm/system-update-check.py` is the integration harness for a production-signed
candidate made from the exact CI artifact. It exercises real SDL startup and the
alive grace period, a crash after promotion, an interrupted trial and corrupted
file bytes. Its mutable test state belongs only in the isolated ARM VM. The
presence of that harness does not establish a passing run, and no handheld test
has been completed as part of this packaging work.

Before enabling a public OTA release,
test the complete updated supervisor and verifier in Linux/ARM, then on the actual
handheld: signature rejection, corrupted card data, interrupted downloads,
out-of-space, power loss at state transitions, first-frame timeout, failed trials
and recovery to the previous release. No hardware-validation claim follows from
a successful CI package or local signature check.
