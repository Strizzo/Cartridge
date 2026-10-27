# Browser development checks — 27 October 2026

This records the browser development branch, not a stable release. Main and the
released v0.7.0 runtime are unchanged. The tested code revision is
`b66ad0cfeaa3d762a8391944636289b820bc5c26`.

## Verified

[CI run 40521376312](https://github.com/Strizzo/Cartridge/actions/runs/40521376312)
passed both native and ARM jobs. Linux CI ran 198 workspace tests, the explicit
real-browser test, 152 installer/update Python tests and five engine component
invariant tests. The ARM binaries use the existing focal/glibc 2.31 toolchain.

The real-engine test uses an isolated profile and loopback HTTP fixture. It checks
actual CSS/JavaScript and pixels, cookies, links, Back/Forward, text replacement
and form submission, new-window links, confirmation dialogs, main-frame address
tracking, HTTP error status, fresh field-focus replies, full-resolution zoomed
pixels/click coordinates, and Stop while a server deliberately withholds its
response. Controller tests cover modal button capture, held-button repeats and
stale field focus. No personal browser session is used.

The native ARM frontend rendered OpenStreetMap at the default 150% reading zoom
inside a 720×720 SDL UI, including its 720×576 web surface. This used the private
Chromium component and inherited pipes in the isolated ARM VM. The VM is configured
with 1 GiB RAM, no swap, two virtual CPUs and Linux 6.8; it is not an RK3326 timing
or graphics emulator. The browser processes exited after the checks.

## Live acceptance cases

| Task | Mac simulator | ARM VM |
| --- | --- | --- |
| Slashdot: reject optional consent, open a story, Back | Pass | Pass |
| CNN: reject optional consent, open an article, Back | Pass | **Fail: server response** |
| OpenStreetMap: drag, zoom and search for Luxembourg | Pass | Pass |

The ARM CNN response is a literal 13-byte `Unknown Error` body with HTTP 200 from
the site's CDN, confirmed separately in the engine network log. It remains a
failing acceptance case. No browser identity, authentication, certificate or
site-access bypass was added to hide it. Live-site checks are opt-in because
availability and site layouts change.

Engines checked: Mac Chrome 156.0.8078.48, CI Chrome 155.0.8059.39 and private
Debian ARM Chromium 156.0.8078.47. Chromium's own `chrome://sandbox` report in the
private ARM component shows namespace, PID/network namespace and seccomp-BPF
sandboxes active. The scoped VM helper permission does not relax global AppArmor.

## Gates before a handheld/browser release

- Run the read-only [preflight](../crates/cartridge-browser/engine/preflight.py)
  as the real Cartridge user on the handheld. Confirm its kernel supports the
  required unprivileged namespaces, with enough storage for the private engine.
- Test actual display/input, leaving and reopening Browser, cold startup,
  memory pressure, interaction latency, battery and thermal behaviour on RK3326.
  The Mac and ARM VM results do not establish these.
- Resolve the ARM CNN response or retain it as a clearly identified open issue.
- Provide verified engine installation/update delivery and corresponding-source
  distribution before publishing an engine download. The engine is not included
  in runtime OTA or the signed Store catalogue yet.
- Handheld audio output and GPU/WebGL acceleration are not enabled in this
  component. Downloads, file-upload UI, simultaneous tabs, JavaScript prompt text
  entry and native site-permission prompts are also pending.

The engine uses a separate private userspace, not replacements for handheld system
libraries. It needs Linux symlink-capable storage; the exFAT games partition is
unsuitable. All checks above used the Mac or disposable VM; no physical card,
games, boot files or device configuration was written during this browser work.

For the next physical trial, first use the handheld's existing Wi-Fi connection.
Its address/SSH reachability is still needed. Do not change a working card's boot
configuration merely to test this development browser.

See [browser development usage](../crates/cartridge-browser/README.md) and the
[private component build](../crates/cartridge-browser/engine/README.md).
