# GitHub-backed Store

CartridgeOS 0.6.0 uses a static official catalogue at:

https://raw.githubusercontent.com/Strizzo/cartridge-apps/main/catalog.json

The catalogue repository is [Strizzo/cartridge-apps](https://github.com/Strizzo/cartridge-apps). Apps have independent source repositories and versioned GitHub Releases:

| App | Repository | Cartridge ID |
|---|---|---|
| Frequency | [frequency-cartridge](https://github.com/Strizzo/frequency-cartridge) | `dev.cartridge.frequency` |
| Mission Control | [mission-control-cartridge](https://github.com/Strizzo/mission-control-cartridge) | `dev.cartridge.mission-control` |
| Outside | [outside-cartridge](https://github.com/Strizzo/outside-cartridge) | `dev.cartridge.outside` |

No device account or API token is needed. The device fetches one catalogue and downloads explicitly versioned release packages over HTTPS. There is no per-app GitHub API polling and no server to operate. GitHub hosting availability and quotas still apply.

## Installing and updating

Open Store and press Y to refresh the catalogue directly from the network. Automatic
refresh on opening Store respects the cache duration in Settings. L1/R1 switches between **Browse**,
**Installed** and **Updates**; X cycles categories within a view. Switching views
resets the category so an old filter cannot silently hide updates. Installed
includes bundled and Store-installed apps. Updates contains only newer versions
of installed apps, and shows the installed and available versions together.
An offline catalogue with no verified download records asks you to check online
rather than claiming everything is up to date. Open an app with A to review its
available release and permissions before installing or updating. Install/update runs on a worker so input and rendering continue. The installed version takes precedence over the OS bundle. Removing an installed override reveals its bundled version; it does not erase app settings.

The catalogue is an Ed25519-signed envelope. The exact UTF-8 `payload` string is signed and the official public key is compiled into `cartridge-net`. A release entry contains an explicit HTTPS URL, SHA-256, exact byte size and minimum runtime version. Installation checks these, checks manifest identity/version/permissions, rejects unsafe archive paths and links, and stages files before replacing the installed copy. A previous version is retained for rollback. App data lives separately under the cartridge's data directory.

Unsigned remote registries are no longer install sources. Local `registry.json` remains the trusted fallback shipped with the OS bundle; legacy bundled apps continue to launch. The old official registry URL migrates to the signed catalogue. Custom URLs are retained but must serve a catalogue signed with the pinned official key; third-party signing keys are not yet supported. The UI retains its current catalogue if a refresh fails.

App API compatibility is explicit. Frequency 1.2.0 requires **CartridgeOS 0.6.2 or newer** for independent stick input; Mission Control and Outside 1.0.0 require 0.6.0. On an already bootstrapped 0.6.1-or-newer device, install the OS update from **Settings → System Update** and restart before updating Frequency. Older installations need the offline updater bootstrap once. App updates never replace the runtime or kernel.

CartridgeOS 0.6.4 checks each package's required runtime before starting a Store download. An incompatible update displays the app name, required and running OS versions, and the System Update steps. Store failures stay in front of old catalogue-refresh successes until dismissed; retrying does not duplicate them. R2 reads additional pages or dismisses the current result. While a task is running, its progress is shown instead of an older notice. After successful installation, the installed version and local library are reconciled.

Signatures authenticate content but this first version does not enforce signed-catalogue expiry or prevent replay of an older valid catalogue. Signing-key rotation requires a runtime update. Native executable packages, payments, accounts, reviews and telemetry are not part of this Store.

## Developing an app

Edit the app's own checkout. The simulator accepts a cartridge directory outside the runtime repository, for example:

```sh
./sim.sh app ../frequency-cartridge --release
./sim.sh app ../mission-control-cartridge --release
./sim.sh app ../outside-cartridge --release
```

Use the runtime's deterministic fixtures and integration checks for device APIs. App repositories contain packaging validation and release automation. Set a new manifest version, test it, and push a matching `vX.Y.Z` tag. The repository workflow produces `<cartridge-id>.tar.gz`, `release.json` and `SHA256SUMS`.

Review and approve the new version, hash, size and permissions in `cartridge-apps/apps.json`. Its workflow verifies the pinned package. Catalogue signing can run locally or, when the maintainer configures `CATALOG_SIGNING_KEY`, through GitHub Actions. App repository workflows never have the catalogue private key.

## Bundled snapshots

`lua_cartridges/frequency`, `mission_control`, and `outside` are pinned release snapshots for first boot and offline installations. Treat the separate app repositories as their source of truth. `store-apps.lock.json` records each release URL/hash and every packaged file's hash. CI detects modifications to those files.

To refresh the bundled copies from a locally downloaded official signed catalogue:

```sh
# On macOS, use Homebrew OpenSSL 3 for Ed25519 support.
OPENSSL=/opt/homebrew/opt/openssl@3/bin/openssl \
  python3 scripts/sync_store_apps.py --refresh ../cartridge-apps/catalog.json
python3 scripts/sync_store_apps.py
```

Changing a bundled snapshot is a separate, deliberate OS-build step. A newer app release in the Store does not require refreshing or rebuilding the OS bundle.

## Verification

```sh
cargo test -p cartridge-net
cargo test -p cartridge-launcher
python3 -m unittest discover -s tests -p 'test_store_sync.py'
cargo run --bin store-check -- --live
```

The live check uses a newly created temporary home, verifies the public catalogue, downloads and installs official packages, checks installed-over-bundled resolution and rejects a deliberately wrong checksum without damaging the working installation. `--keep` retains this isolated home for running `app-check` against the downloaded apps. It does not use the developer's real app data or an SD card. Hardware rendering and timing still require the handheld; the simulator and ARM VM cover the code and install flow.

## Verification record — 2026-10-02

The three public v1.0.0 release workflows and catalogue validation passed. The native live Store check fetched the signed public catalogue and installed all three GitHub packages in a fresh temporary home. It confirmed installed-over-bundled resolution, preserved app data, and rejected wrong-checksum updates without damaging the installed copy. The downloaded packages ran through the 720×720 app simulator with service fixtures.

Workspace tests cover signature tampering and OpenSSL/Rust interoperability, archive traversal and links, oversized/error downloads, manifest and compatibility mismatches, interrupted installation recovery, rollback, asynchronous Store completion and catalogue reordering. Python tests check the pinned bundled snapshots alongside the installer/session regressions. Visual baselines use isolated storage, a fixed simulated clock and the R36S profile.

Automatic catalogue signing is optional. The initial catalogue was signed locally; uploading the private key to GitHub Actions requires the owner's explicit approval. Public app release workflows do not need that key.
