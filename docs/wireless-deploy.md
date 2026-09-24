# Wireless Deploy

Build on your machine, push to the handheld over WiFi, restart the launcher,
tail its logs, and pull a screenshot from the real panel — without ever taking
the SD card out.

```bash
deploy/wifi/deploy-wifi.sh --host 192.168.1.42 --save   # first time
deploy/wifi/deploy-wifi.sh                              # afterwards
deploy/wifi/device-logs.sh                              # follow the journal
deploy/wifi/device-shot.sh                              # screenshot -> screenshots/
```

## One-time setup

**1. Enable SSH on the device.** In ArkOS: **Options → Enable Remote Services**
(some builds call it "Enable SSH"). The default login is `ark` / `ark`.

**2. Find its address.** On the device, open Cartridge **Settings → WiFi** — the
current IP is shown there. A DHCP reservation in your router keeps it stable;
otherwise re-run with `--host <new ip> --save` when it changes.

**3. Install your key.**

```bash
ssh-copy-id ark@192.168.1.42
```

The scripts use `BatchMode=yes`, so key auth is required — they will not prompt
for a password.

**4. A container runtime for ARM builds.** `cross` needs Docker or Podman.
On macOS without Docker Desktop:

```bash
brew install colima docker
colima start
```

Then `deploy-wifi.sh` builds normally. Without a runtime, pass `--skip-build` to
push the binaries you already have (useful when you only changed Lua
cartridges, assets or icons).

## What gets pushed

The same payload the SD-card installer writes, to `<roms>/Cartridge/`:

- `cartridge`, `cartridge-boot`
- `cartridge-boot.sh`, `cartridge-boot.service`, `autosetup.sh`
- `registry.json`
- `assets/fonts/`, `assets/overlays/`, `boot_logo.png`, `gamecontrollerdb.txt`
- `lua_cartridges/`
- the Tools menu scripts, to `<roms>/tools/`

`<roms>` is `/roms2` when ArkOS's "Switch to main SD for Roms" option is
active, `/roms` otherwise. The scripts probe for this on the device rather than
assuming. Because the roms partition is exFAT and cannot store the execute bit,
everything is `chmod +x`-ed after the copy.

## Restarting

When the primary-session override is installed, deployment refreshes its supervisor
and restarts `emulationstation.service`, whose override starts Cartridge directly.
Use `--payload-only` when a game is running or you want to defer the restart.
Legacy boot-selector installations still use their existing service. Manual-only
installations require relaunching Cartridge from Tools.

To enable primary startup once, after validating the build:

```bash
ssh ark@DEVICE 'sudo python3 /roms/Cartridge/setup-primary.py enable --cartridge-dir /roms/Cartridge'
```

The next normal boot starts Cartridge. Substitute `/roms2` when appropriate.
See [primary session](primary-session.md) for recovery and undo.

## Logs

The systemd unit now sets
`RUST_LOG=cartridge=info,cartridge_launcher=info,cartridge_core=info,cartridge_lua=info`,
so `info`-level messages reach the journal. Before this, the device defaulted to
`error` and every breadcrumb in the code was invisible.

```bash
deploy/wifi/device-logs.sh          # journalctl -u cartridge-boot -f
deploy/wifi/device-logs.sh --dump   # crash.log, setup.log, wifi log, recent journal
```

Other logs worth knowing: `<roms>/Cartridge/crash.log` (written when a cartridge
errors), `setup.log` (boot-service installation), `/tmp/cartridge_wifi.log`
(connection failures from the WiFi screen), and
`/home/ark/.cartridges/wifi-scan.log` (radio, interface, and rfkill diagnostics
when a scan returns no networks).

## Screenshots from the device

`device-shot.sh` sends `SIGUSR1` to the running launcher, waits for it to write
the next frame to `<roms>/Cartridge/screenshots/`, copies the newest PNG back
into `screenshots/`, and opens it. Use `--keep` to leave the copy on the device.

## Gotchas

- **Primary sessions run as `ark`.** Their settings and session logs are in
  `/home/ark/.cartridges/`. Legacy root-run installations used `/root/.cartridges/`;
  preserve that data if migrating. The current manual ES Tools launch also runs
  as `ark`, so enabling primary startup keeps its existing app state.
- **Bundled apps beat installed ones.** A cartridge in
  `<roms>/Cartridge/lua_cartridges/` shadows the same app installed from the
  store into `/root/.cartridges/apps/`.
- **Version check.** `deploy-wifi.sh` runs `./cartridge --version` on the device
  after pushing and prints the result, so you can confirm what's actually
  running.
- The device must be awake and on WiFi. It does not wake on LAN.

## Options

| Flag | Scripts | Meaning |
| --- | --- | --- |
| `--host <ip\|name>` | all | Device address. Falls back to `$CARTRIDGE_HOST`, then `deploy/wifi/.device`. |
| `--user <name>` | all | Login user, default `ark`. |
| `--save` | all | Remember host/user in `deploy/wifi/.device` (gitignored). |
| `--skip-build` | deploy | Push without cross-compiling. |
| `--payload-only` | deploy | Push without restarting the launcher. |
| `--dump` | logs | Print the log files once instead of following the journal. |
| `--keep` | shot | Leave the screenshot on the device as well. |
