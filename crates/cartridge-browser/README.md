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
- B or L1: Back. R1: Forward. L2/R2: page zoom out/in.
- X: address/search. Y: bookmarks/history. Start: page tools. Select: leave Browser.
- The native on-screen keyboard uses the same controls as the rest of Cartridge.
  A focused text field can open it; Start > Type also works for embedded fields.
  Start replaces the focused field text; R2 also presses Enter to submit.
  Existing passwords are never read into the native keyboard.
  Page tools also has an explicit Enter action.
- On the Mac: F2 opens address entry, F12 captures the browser. A USB keyboard and
  mouse work alongside controller input. IJKL/TFGH simulate the two sticks.

History and up to 64 bookmarks are stored separately from the engine profile.
History writes are coalesced; map fragment changes do not fill the history list.
Engine page capture stops behind native menus; animated-page frame transfer is
backpressured to 24 frames/second during interaction and 8 while idle. Static browser chrome has no animation.
Navigation is asynchronous, so a slow connection does not hold the input worker.
Queued continuous motion is coalesced while preserving click/key boundaries.
Reduced-motion preferences are sent to sites. A website can still consume CPU on
its own, and the Mac cannot establish handheld CPU, power or memory performance.

## Checks

```sh
cargo test -p cartridge-browser --lib
CARTRIDGE_BROWSER_TEST_OUT=/tmp/browser-check \
  cargo test -p cartridge-browser --test real_engine -- --ignored --nocapture
```

The real-engine check serves a loopback-only fixture and verifies actual CSS grid
layout and pixels, JavaScript, cookies, a mouse link, Back/Forward, native text
insertion, form submission, zoom, JavaScript dialogs, new-window link routing,
main-frame address tracking, and cancellation while a server stalls. It uses throwaway profile storage. Real Mac
captures use a 720x720 UI. The live-site checks have opened articles and returned
with Back on Slashdot and CNN, and dragged, zoomed and searched on OpenStreetMap.
They use 75% page zoom to fit desktop layouts and consent dialogs in the viewport;
normal browsing starts at 100% and L2/R2 change it.

## ARM engine component — work in progress

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
CNN has returned an “Unknown Error” page in an ARM trial and remains an open
compatibility issue, even though its Mac interaction check passes.
The portable helper and finalized private component also pass Chromium's internal
namespace/seccomp sandbox report in the ARM VM. Full ARM frontend tests, physical
old-kernel compatibility and performance are still being verified. The engine component is
not part of the normal runtime OTA payload or signed Store catalogue yet. Do not
install random Chromium packages over the working handheld's Linux libraries.
Downloads, file upload UI, multiple simultaneous tabs, and site-permission prompts
are not implemented in this development build. Handheld audio output is not wired
through the private engine yet; it must be checked separately from rendered video.
GPU/WebGL acceleration is also not enabled in the private headless engine. This is not a release-ready claim.

The component build recipe and isolation details are in [engine/README.md](engine/README.md).
A persistent component can live at
`<CARTRIDGE_HOME>/.cartridges/components/browser-engine/`; a component next to the
runtime binary also works. This is separate from versioned runtime OTA releases.

Opt-in live-site interaction checks (network required):

```sh
cargo run -p cartridge-browser --bin browser-check -- \
  --site all --out /tmp/new-browser-site-check
```

This exercises news article links and Back on Slashdot/CNN, and map dragging,
zoom controls and location search on OpenStreetMap. It saves public-page captures,
inspection metadata and reports using its own profiles. A blocked consent dialog,
changed site layout or unavailable network is reported as a failure. It does not
bypass consent, authentication, certificates or site access restrictions.

For repeatable UI captures, add `--capture /tmp/browser.png --capture-after 8
--seconds 10 --hidden` after the simulator's `--` separator. Delayed capture lets
lazy map tiles/images finish loading; it does not change normal browsing.
