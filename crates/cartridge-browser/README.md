# Cartridge Browser (0.8 development)

A native controller-first frontend with a **local Chromium engine**. Websites
retain their actual HTML, CSS, JavaScript, cookies and forms. No companion server,
text extraction service, or embedding in an iframe is involved. Chromium and its
renderer processes communicate with the SDL frontend through inherited CDP pipes;
there is no debugging TCP listener and no privileged bridge exposed to websites.

## Run on the Mac

Google Chrome or Chromium must be installed. The application uses its own private
profile; it does not attach to your personal browser session.

```sh
./sim.sh browser
./sim.sh browser -- --url https://slashdot.org/
./sim.sh browser -- --url https://www.cnn.com/
./sim.sh browser -- --url https://www.openstreetmap.org/
./sim.sh app lua_cartridges/browser
```

`CARTRIDGE_BROWSER_ENGINE` selects a specific engine executable. Browser is also
listed in the 0.8 launcher. The normal simulator provides isolated state under
`.sim/home`; `--profile DIRECTORY` selects an explicit standalone test directory.

## Controls

- Left stick: pointer. A: click; hold A and move to drag.
- Right stick: scroll in either direction. D-pad Up/Down: scroll vertically.
- D-pad Left/Right: previous/next keyboard-focusable link or control. A activates it.
- B or L1: Back. R1: Forward. L2/R2: page zoom out/in (60–200%, with finer steps).
  New sites start at 100%; the browser remembers your chosen zoom for each site.
  Bookmarks/history from earlier development builds are preserved.
- X: address/search. Y: bookmarks/history. Start: page tools. Select: leave Browser.
- The native on-screen keyboard uses the same controls as the rest of Cartridge.
  A focused text field can open it; Start > Type also works for embedded fields.
  Start replaces the focused field text; R2 also presses Enter to submit.
  Existing passwords are never read into the native keyboard.
  Page tools also has an explicit Enter action.
- On the Mac: F2 opens address entry, F12 captures the browser to
  `<CARTRIDGE_HOME>/.cartridges/screenshots` (the simulator uses `.sim/home`). A USB keyboard and
  mouse work alongside controller input. IJKL/TFGH simulate the two sticks.

Links requesting a new window normally follow in the active page. A companion
popup cannot replace a navigation the active page already requested; at most one
new-window destination waits for a decision. Use Start > Tabs for separate tabs.

History and up to 64 bookmarks are stored separately from the engine profile.
History writes are coalesced; map fragment changes do not fill the history list.
Engine page capture stops behind native menus; animated-page frame transfer is
backpressured to 24 frames/second during interaction and 8 while idle. Static browser chrome has no animation.
Navigation is asynchronous, so a slow connection does not hold the input worker.
Queued continuous motion is coalesced while preserving click/key boundaries.
Reduced-motion preferences are sent to sites. A website can still consume CPU on
its own, and the Mac cannot establish handheld CPU, power or memory performance.

## Tabs, files and site permissions

Start > Tabs opens a maximum of two tabs. Inactive tabs are frozen to reduce CPU
use; their timers and audio pause. Opening the native tools menu preserves audio
in the active page. Start also provides JavaScript prompt entry, downloads and
origin-labelled site-permission review. Permissions reset on navigation/tab switch;
approve explicitly, then retry the site's request. Approval cannot provide camera
or microphone devices that the private engine does not expose.

Downloads require confirmation and support HTTP document attachments, one at a
time, at most 32 MiB each with a 64 MiB folder quota. Blob/data downloads,
pause/resume and a general file manager are not implemented. The upload picker
starts on Cancel and selects only regular files from
`<home>/.cartridges/dev.cartridge.browser/profile/files`, up to eight files and
32 MiB total. Host OS files, symlinks and arbitrary paths are excluded.

Cookies, localStorage and IndexedDB have passed separate-process persistence
checks. Library saves are atomic and retried after failure; damaged library JSON
is preserved. The engine gets up to five seconds to flush on exit. On Linux,
sustained available memory below 220,000 KiB (or a single sample below 190,000 KiB)
closes this browser engine and reports the problem while retaining saved data.

## Checks

```sh
cargo test -p cartridge-browser --lib
CARTRIDGE_BROWSER_TEST_OUT=/tmp/browser-check \
  cargo test -p cartridge-browser --test real_engine -- --ignored --nocapture
SDL_VIDEODRIVER=dummy cargo test -p cartridge-browser --test capabilities -- --ignored --test-threads=1
```

The real-engine check serves a loopback-only fixture and verifies actual CSS grid
layout and pixels, JavaScript, cookies, a mouse link, Back/Forward, native text
insertion, form submission, zoom, JavaScript dialogs, new-window link routing,
main-frame address tracking, HTTP error status, full-resolution zoomed pixels
and click coordinates, and cancellation while a server stalls. It uses throwaway profile storage. Real Mac
captures use a 720x720 UI. The live-site checks have opened articles and returned
with Back on Slashdot and CNN, and dragged, zoomed and searched on OpenStreetMap.
They use 75% page zoom to fit desktop layouts and consent dialogs in the viewport;
normal browsing now starts at 100% and L2/R2 change it. The old 150% starting
view squeezed legacy desktop columns too narrowly on sites such as Slashdot.

## Private ARM engine component

`engine/chromium` launches a private ARM64 Linux userspace through a static
bubblewrap helper. The private root has modern browser dependencies without
upgrading the handheld base system. It is mounted read-only, with a dedicated
profile and temporary directories writable. Network access uses the existing
connection. The legacy setuid sandbox is disabled **only because the component
uses unprivileged user namespaces**; Chromium's namespace/seccomp sandbox is kept.
The wrapper refuses root execution, sandbox bypass, certificate bypass, and web
security bypass flags. There is no unsafe fallback when namespaces are unavailable.

The ARM VM has executed Chromium 156.0.8078.47 and real JavaScript in this setup.
The native ARM frontend also renders OpenStreetMap through the private pipes.
ARM interaction checks have passed for Slashdot article/Back and map drag/zoom/search.
Linux now uses normal Chromium with Ozone's offscreen display, which fixes the
13-byte CNN CDN response seen with headless mode. The strict article/Back check
passes on the Mac and 1 GiB ARM VM. CNN still reaches the actual handheld's memory
guard during its late consent flow. Ordinary article/Back checks now pass on the
handheld, but the explicit cold-consent case remains unreliable. Slashdot and
OpenStreetMap pass there.
No browser identity override, consent bypass or lower memory floor is used.
The private component passes Chromium's namespace, PID/network namespace and
seccomp-BPF report on both the ARM VM and the actual Linux 4.4.189 handheld.
The user has opened Slashdot with the physical controller, returned to Cartridge
and accepted the improved 100% reading baseline.

The RK817 engine exposes playback/control/timer nodes, with a private ALSA mixer
at 48 kHz. Actual handheld checks exercised WebAudio and HTML audio at both
44.1/48 kHz. Microphone capture and sequencer devices remain outside the sandbox.
The SDL interface uses the accelerated KMSDRM renderer. Web rendering uses software;
the vendor driver lacks eight GBM APIs required by this Chromium, so hardware
GPU/WebGL acceleration is not enabled. See [the measured ABI result](engine/gpu/README.md).

The engine is independently signed and stored on the Linux home filesystem,
separate from runtime OTA and the Store catalogue. Settings > Browser Tools can
verify it, check the dedicated engine channel, install and roll back in the
background. Public engine artifacts have not been published yet; an unavailable
channel is reported as an error. Do not install arbitrary Chromium packages over
the handheld's working Linux libraries. See [delivery](../../docs/browser-component-delivery.md)
and [corresponding sources](engine/sources.md).

The component build recipe and isolation details are in [engine/README.md](engine/README.md).
A persistent component can live at
`<CARTRIDGE_HOME>/.cartridges/components/browser-engine/`; a component next to the
runtime binary also works. This is separate from versioned runtime OTA releases.

Opt-in live-site interaction checks (network required):

```sh
cargo run -p cartridge-browser --bin browser-check -- \
  --site all --out /tmp/new-browser-site-check
```

For a fresh-profile case that must wait for CNN's visible privacy controls and
reject optional cookies before opening an article, use `--site cnn-consent`.
This is a separate, currently failing handheld acceptance case; an ordinary CNN
navigation pass does not imply it passes.

For isolated transfer-cost experiments, the checker accepts
`CARTRIDGE_BROWSER_CHECK_ACTIVE_FPS` and `CARTRIDGE_BROWSER_CHECK_IDLE_FPS`
(integers 1–30). Defaults remain 24/8, matching the native interface; alternate
budgets appear in `report.json` and do not change the production frontend.

This exercises news article links and Back on Slashdot/CNN, and map dragging,
zoom controls and location search on OpenStreetMap. It saves public-page captures,
inspection metadata and reports using its own profiles. A blocked consent dialog,
changed site layout or unavailable network is reported as a failure. It does not
bypass consent, authentication, certificates or site access restrictions.

For repeatable UI captures, add `--capture /tmp/browser.png --capture-after 8
--seconds 10 --hidden` after the simulator's `--` separator. Delayed capture lets
lazy map tiles/images finish loading; it does not change normal browsing.
Once a frame is available, the capture records the visible UI after the requested
delay even if background page requests are still loading.

For a fixed internal Chromium sandbox diagnostic, run `browser-check --site sandbox
--out /tmp/new-sandbox-check --engine /path/to/chromium`. This checks the engine
report; it does not relax the sandbox or expose internal addresses in normal browsing.

The web surface still renders at native 720×576 resolution. JPEG frame quality is
90 for clearer text, with the same 8/24-fps backpressure limits. F12 and SIGUSR1
request native UI screenshots so a running handheld page can be inspected remotely.
