# Security

Please report suspected vulnerabilities privately through
[GitHub's vulnerability reporting form](https://github.com/Strizzo/Cartridge/security/advisories/new).
Include the runtime version, affected app or component and a minimal reproduction.
Do not include private keys, passwords or personal data in public issues or logs.

CartridgeOS is experimental software. Use the latest patched stable release
and keep a separate backup of your card, games and saves.

Lua apps receive selected APIs according to their manifest permissions. The
runtime restricts Lua libraries and app storage paths, but apps run in the
runtime process without OS-level isolation or per-app CPU and memory limits.
Install only apps you trust, and review network and SSH permissions. A signed
catalog or update verifies its publisher and integrity; it does not make an
app's behavior safe.

Cartridge updates cover the launcher and its bundled apps. The underlying
Linux system, kernel, drivers and separately installed components need their
own security maintenance. See [installation and recovery](INSTALL.md) and the
[update guide](docs/online-updates.md).
