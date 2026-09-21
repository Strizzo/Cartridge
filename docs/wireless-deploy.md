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

After a push the script touches `/tmp/.cartridge_skip_selector` and restarts
`cartridge-boot.service`. That flag makes `cartridge-boot.sh` skip its 5-second
boot selector once, so you land straight back in the launcher; the next real
boot shows the selector as usual. Pass `--payload-only` to push without
restarting.

If the boot service isn't installed (you launch Cartridge from EmulationStation
→ Tools instead), the script kills the running launcher and tells you to
relaunch from the Tools menu.

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
(nmcli failures from the WiFi screen).

## Screenshots from the device

`device-shot.sh` sends `SIGUSR1` to the running launcher, waits for it to write
the next frame to `<roms>/Cartridge/screenshots/`, copies the newest PNG back
into `screenshots/`, and opens it. Use `--keep` to leave the copy on the device.

## Gotchas

- **`HOME` is `/root`.** The launcher runs as root under systemd, so its
  settings and installed apps live in `/root/.cartridges/…`. An SSH session as
  `ark` has a *different* home — if you poke at settings by hand, use absolute
  paths under `/root/.cartridges`, or you'll edit a file nothing reads.
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
