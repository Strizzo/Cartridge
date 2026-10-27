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

## Handheld trial

On the actual R36S Plus, Linux 4.4.189/aarch64 and the normal `ark` user passed
both the unprivileged namespace probe and the exact static bubblewrap helper
probe. The engine archive and all 16,664 regular-file hashes were verified in a
new private directory under the user's cache. The trial did not replace system
libraries, the installed runtime, boot files or games.

The same ARM frontend/engine code as the VM trial passed the real Slashdot
article/Back check in about 22 seconds including cold startup, and OpenStreetMap
drag/zoom/search in about 21 seconds. CNN again returned the literal HTTP 200
`Unknown Error` body. These are automated engine interactions, not a claim that
the physical gamepad has been tested.

A separate 35-second native UI trial rendered the actual 720×720 surface through
KMSDRM with the accelerated SDL renderer at the default 150% page zoom. The capture
includes the real site consent dialog. It exited normally and restored the
installed Cartridge launcher. This native trial kept at least 567,136 KiB of
system memory available; the map engine check kept at least 513,116 KiB available.
These short samples do not establish long-session memory, latency or battery use.
The kernel has no memory cgroup controller, so trials used a time limit and a
180,000 KiB available-memory guard. The SDL build lacks the dummy video driver;
headless engine checks work, but native UI tests need a brief display handoff.

Cartridge Settings now includes Remote Access in the development source: service
status, login/address, explicit enable/start and disable/stop confirmation, startup
persistence and background command execution. It uses the existing SSH unit and
existing service-management permissions. Its Mac simulator views and crate tests
pass; the feature is not yet installed on the handheld. Stable v0.7.0 is unchanged.

## Gates before a handheld/browser release

- Repeat the read-only [preflight](../crates/cartridge-browser/engine/preflight.py)
  for other devices/cards. The checked handheld passes; this is not generic
  compatibility evidence for every R36S variant. Verify Chromium’s own sandbox
  report on the handheld as well as the namespace helper.
- Complete the physical-controller trial, leaving/reopening Browser, and measure
  memory pressure, interaction latency, battery and thermal behaviour on RK3326.
  Short native trials and the Mac/ARM VM results do not establish these.
- Resolve the ARM CNN response or retain it as a clearly identified open issue.
- Provide verified engine installation/update delivery and corresponding-source
  distribution before publishing an engine download. The engine is not included
  in runtime OTA or the signed Store catalogue yet.
- Handheld audio output and GPU/WebGL acceleration are not enabled in this
  component. Downloads, file-upload UI, simultaneous tabs, JavaScript prompt text
  entry and native site-permission prompts are also pending.

The engine uses a separate private userspace, not replacements for handheld system
libraries. It needs Linux symlink-capable storage; the exFAT games partition is
unsuitable. The private handheld trial changes only its dedicated cache directory. Installed
Cartridge files, boot configuration, system libraries and games remain unchanged.

The handheld is now reachable over its existing Wi-Fi/SSH connection. Further
trials can use that path; do not change working boot configuration to test this
development browser.

See [browser development usage](../crates/cartridge-browser/README.md) and the
[private component build](../crates/cartridge-browser/engine/README.md).

## Readability follow-up

The user confirmed opening Slashdot through the physical preview. The first
visual comparison reproduced its article column shrinking to roughly 120 CSS
pixels at the old 150% default, versus roughly 333 at 100%. The development
browser now starts new sites at 100%, provides finer 60–200% zoom steps and
remembers the chosen level for each origin (bounded to 128 sites). Back/Forward
and redirects restore the destination site's preference; old library files keep
their bookmarks/history. JPEG transport quality rises from 75 to 90 without
raising the frame-rate caps. SIGUSR1/F12 captures now work in the browser loop.

A Mac native-resolution capture and real-engine checks cover the new baseline.
The new transport quality and preferences still need a handheld preview check;
this follow-up is not yet installed on the device. The original site layout is
retained, including its sidebar and consent controls.
