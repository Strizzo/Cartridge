# Browser development checks — 27–28 October 2026

This is the 0.8 development branch, not a public stable release. Main and the
released 0.7 runtime remain unchanged. These notes record staged development
observations; the PR description tracks later candidate activation. The screenshot-safe
0.8.1 patch (`bdb28e4f934c974a9bf97a8be447f63cb3268d52`) was installed and
reboot-verified with stable 0.7 retained as the direct rollback release. Subsequent
0.8.2 candidates change Linux browser startup.

The final screenshot-safe candidate is **0.8.1**, source
`4cafb388f1bad52f42d1cc6dd527c9dfdc7d1977`, CI merge
`bdb28e4f934c974a9bf97a8be447f63cb3268d52`. Its ARM binary SHA-256 is
`d21ce87d4b283e13d21bc10f4cb410e20785832b7e2944682faab979b3cf5718`.
[CI 40589298315](https://github.com/Strizzo/Cartridge/actions/runs/40589298315)
passes native and ARM jobs on its second attempt. The first attempt timed out
waiting 15 seconds for CI Chrome's initial reply; the fresh-runner retry passed
without disabling any sandbox. Mac real-Chrome checks also pass.

**0.8.1 is installed and reboot-verified.** After the user reconnected Wi-Fi
and charging (34%), the installed bootstrap authenticated the signed candidate.
The original supervisor promoted it after first-frame health, with verified
0.7 retained as previous and no pending/trial state. A real SIGUSR1 screenshot
landed in `/home/ark/.cartridges/screenshots`; the entire signed release still
verified afterwards. A full reboot changed the boot ID and returned to the exact
signed 0.8.1 binary, permanent engine and existing browser profile. Bootstrap
and supervisor hashes remain unchanged. The earlier low-power hold is resolved.

## Automated behaviour

The Mac checks pass 20 browser unit/visual cases and four real Chromium fixtures.
They exercise actual HTML/CSS/JavaScript, pixels, navigation, text/forms, prompt
entry, explicit permissions, uploaded/downloaded bytes, cancellation, two tabs
with frozen backgrounds, audio continuity through menus and cookies,
localStorage and IndexedDB across separate Chromium processes. They use isolated
loopback fixtures and profiles, never a personal browser session.

The integrated launcher passes 73 tests; three additional visual fixtures cover
its native settings/update screens. The component verifier passes 14 tests and
its shared CLI passes four. The Python checks include 25 trusted-bootstrap
staging tests, eight component-packaging tests, seven component invariants and
16 source-collection cases. Tests retain production signature, path, filesystem
and power restrictions; their fixtures use the appropriate private home storage.

[CI run 40586017730](https://github.com/Strizzo/Cartridge/actions/runs/40586017730)
built the ARM candidate and passed Rust and real-browser checks. Its Python
staging fixtures incorrectly used `/tmp`, outside the required Linux home
filesystem. The fixture-only correction passes all 25 tests on ARM Linux.
[The final rerun](https://github.com/Strizzo/Cartridge/actions/runs/40587335141)
passed both native and ARM jobs, including that correction, the four real-browser
fixtures, 201 Python tests and the simulator smoke scenarios.

## Follow-up Mac startup checks — 28 October

Two native UI captures stalled at the initial `Browser.getVersion` command with
Chrome 156.0.8078.48. The unchanged source then passed three native runs with the
installed Chrome 156.0.8078.73: two Slashdot captures and one Wikipedia capture,
each 720×720, exiting normally with an initial reply after roughly 1–2 seconds.
One run used `./sim.sh browser` with the parent checkout's isolated home; another
used the direct browser binary. A separate restricted-tool attempt failed before
engine startup because SDL had no display, which is a different failure.

The earlier successful baseline also used Chrome .48, so the version change does
not establish the cause of the intermittent stalls. No startup fix is claimed,
and neither the timeouts nor Chromium safeguards changed. The current captures
establish that native startup works again; they do not prove the earlier failure
cannot recur. Blog images use genuine earlier successful captures and distinguish
the Mac simulator from the handheld.

## Actual handheld

The device is an R36S Plus/RG351MP profile: RK3326, Linux 4.4.189/aarch64,
919,240 KiB RAM and no swap. Tests run as normal user `ark` over Wi-Fi. The Mac
simulator and 1 GiB ARM VM are useful compatibility tools, not RK3326 performance
or graphics emulators.

The user opened Slashdot with the physical controller, returned to Cartridge,
reopened Browser and confirmed the improved readability. New sites start at
100%, with 60–200% zoom remembered for up to 128 origins. The web surface remains
720×576 inside the 720×720 native interface, with JPEG quality 90. Its frame
backpressure remains 8 idle/24 active; inactive tabs are frozen and native menus
pause frame transfer while retaining active-page audio.

| Task | Mac | ARM VM | Handheld |
| --- | --- | --- | --- |
| Slashdot article and Back | Pass | Pass | Pass |
| OpenStreetMap drag, zoom, search | Pass | Pass | Pass |
| CNN article and Back | Pass | Fail | Fail |

The actual CNN response redirects to `edition.cnn.com`, then returns HTTP 200
with a 13-byte `text/plain` body, `Unknown Error`, over HTTP/2. CDP reports a
Varnish CDN cache hit, no service worker and no disk-cache response. Separate curl
requests on the same handheld obtain the full HTML. This remains a failing
browser compatibility case for installed 0.8.1.

A controlled follow-up reproduced the 13-byte response with the engine's actual
HeadlessChrome request identity, while ordinary HTTP returned full HTML. Running
the same signed Chromium in normal browser mode with its documented
[Ozone offscreen display backend](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/ozone_overview.md)
then rendered CNN on both ARM VM and handheld. No user-agent override, automation
concealment, consent bypass or security relaxation was used; `webdriver` remains
true. The VM also passed CNN/Slashdot article and Back, map drag/zoom/search and
namespace/seccomp/TSYNC diagnostics through this backend. On the handheld, three
bounded CNN navigations produced actual page captures and about 483 links, with
at least 510,548 KiB available in one live sample. Heavy page resources had not
finished loading at the 20-second observation limit, although content rendered.
The 0.8.2 source integrates this Linux-only backend; macOS keeps its existing
headless startup. A rebuilt ARM runtime and final integration test are still
required before claiming the production path fixed.

A 126.5-second native map session exited normally and restored the launcher,
keeping at least 557,632 KiB (about 545 MiB) of system memory available. The SoC
started near 77°C after repeated tests, briefly peaked at 82.7°C while loading,
and settled near 70°C. CPU frequency samples span 1.008–1.296 GHz. These are short,
charging-device observations, not battery-life or sustained-throttling evidence.
The native interface uses accelerated SDL/KMSDRM.

Chromium's own sandbox report passes on the handheld, including after audio
bindings: namespace layer 1, PID/network namespaces, seccomp-BPF and TSYNC, with
“You are adequately sandboxed.” No global security policy or unsafe flags changed.

The RK817 playback path uses a private ALSA plug/dmix configuration with one
48 kHz hardware stream. Actual WebAudio and HTML audio at both 44.1/48 kHz passed,
with the physical PCM running across rate changes. Only playback, control and
playback timer nodes are bound; microphone and sequencer nodes are excluded.
This establishes delivery to the codec, not an independent listening-quality test.

Web rendering remains software. The installed Mali GBM library lacks eight strong
Chromium imports; the [static ABI report and guarded experiment](../crates/cartridge-browser/engine/gpu/README.md)
record that blocker. No invented planes/modifiers or unverified GPU adapter ships
in engine 1.0.0. Hardware GPU/WebGL acceleration and long battery tests remain open.

## Signed delivery and recovery

Engine 1.0.0 is independently signed with the existing trusted production key.
It contains 19,875 entries, about 934 MB of stored regular bodies and a 348 MB
compressed archive; hardlinks avoid duplicating roughly another gigabyte. The
case-sensitive Linux finalizer retains package/copyright notices and certificate
bytes. It preserves legitimate UTF-8/Unix names and materializes three long CA
aliases as identical regular files, keeping strict USTAR without extensions.

The source sidecar contains 576 verified artifacts, about 1.69 GB, covering all
176 Debian source packages, the Ubuntu static-helper dependencies and bubblewrap.
Its lock SHA-256 is
`ccf36b71bd206cd40daa1bfed3f10ddbc9dfd2d758f45e9906c66e9c59d5b61e`.
A copied Mac sidecar also passes full verification. These are local artifacts;
public delivery and license/relinking review remain separate release work.

The exact signed ARM runtime passes a VM rehearsal of real SDL first-frame
promotion, crash rollback, interrupted-trial recovery and refusal of altered
release files, without calling ES or changing fixture game saves. The actual
installed 0.6.1 bootstrap also authenticates the candidate in the VM and on the
handheld. Linux VM checks additionally pass zero-copy engine adoption, a real
signed streaming update with 16,626 reusable files, inode reuse and verified
rollback. No rehearsal-only engine fixture was installed on the handheld.

Settings > Browser Tools provides asynchronous installed-engine verification,
signed updates and rollback with persistent errors. The dedicated
`browser-engine-stable` channel is separate from the runtime's latest release.
It is not published yet and reports unavailable honestly. The same verified
commands are embedded in the signed runtime as `cartridge browser-engine`.

## Subsequent 0.8.2 checks

The normal-browser/Ozone backend passed native and ARM CI, plus exact ARM
promotion, screenshot verification and recovery in the VM. A real accelerated
KMSDRM handheld session rendered CNN and captured it with SIGUSR1, then restored
0.8.1. Automatic capture initially waited for every background resource; the
capture-only follow-up records the available frame at its requested delay. A
real Mac fixture with an intentionally pending image verified that behaviour.

Guarded CNN checks exposed two distinct limits. The 1 GiB VM reached the unchanged
220,000 KiB memory floor around consent handling. The actual handheld retained
at least 415,592 KiB but its check waited for `readyState === complete` even though
the page was interactive. The live checker now uses parsed content, revalidates
rendered links after scrolling, checks the intended article's path/site and
requires a rendered listing after Back. All three strict Mac cases passed.
Critical memory notifications and forced low-end mode were tested in the VM;
neither prevented its guard, and neither change is adopted in production.

A separate loopback regression proved that a companion popup could replace a
normal article navigation. The corrected engine queues one popup and checks its
owning target/navigation generation across an asynchronous renderer round trip.
Immediate and about:blank companion cases retain the article and working Back;
legitimate new-window links and the existing tab bounds still pass. No new
popup delay, timeout extension, site blocklist or security bypass is used.
These Mac fixture results precede the final rebuilt ARM checks recorded in the PR.

## Exact 0.8.2 installation and visible-site checks

The signed payload from [CI 40612661758](https://github.com/Strizzo/Cartridge/actions/runs/40612661758)
passed all seven ARM promotion/recovery checks, then installed over Wi-Fi as
`0.8.2-fbdbb1b5a1bd`. A screenshot in user storage was followed by whole-release
signature verification. A real reboot returned to that exact binary; the signed
engine and persistent browser profile verified afterward. `0.8.1-bdb28e4f934c`
is the previous release. Bootstrap and supervisor hashes remain unchanged.

The subsequent checker-only [CI 40616261690](https://github.com/Strizzo/Cartridge/actions/runs/40616261690)
also passed native and ARM checks. It tries all visible consent choices first,
including Show Purposes, and inspects a closed shadow root only when a visible
overlay blocks the intended article. Ordinary clickable pages need no DOM search.
Trusted input, exact intended article path/site, rendered heading and real Back
checks remain mandatory, with the same zoom, time bounds and memory guard.

All three cases pass on the Mac and ARM VM. The VM run took 13.03 seconds and
retained at least 277,048 KiB available. On the actual handheld, Slashdot article
and Back passed in 25.81 seconds; OpenStreetMap pan, zoom and search passed in
21.14 seconds. CNN rendered but reached the guard at 218,532 KiB during late
consent, with a sampled minimum of 215,036 KiB. There was no closed-shadow DOM
search in this run. The earlier hidden-DOM test overhead therefore does not
explain away this remaining physical-device limit. CNN is still a failing
handheld acceptance case.

A timed native CNN image in the VM also contains the low-memory error panel.
Successful capture/exit confirms the screenshot path, not continued page usability.
The earlier actual-device CNN capture demonstrates rendering, not long-session
stability. Neither capture is counted as a full CNN interaction pass.

## 0.8.3 transport regression

A valid fragmented 7 MiB reply reproduced a stack overflow in the old CDP reader.
Each 32 KiB read recursively entered the reader again. A measured real closed
shadow-tree inspection had also produced a 7.42 MB unsolicited DOM event, below
the existing 8 MiB transport limit.

The reader now accumulates ready chunks iteratively and scans only newly received
bytes for the frame delimiter. Partial data survives an idle pipe; only the first
poll may wait. The memory and cancellation guards run between chunks, with the
same message cap and command timeout. Deterministic inherited-pipe tests cover a
7 MiB reply, a 7 MiB unsolicited event followed by its reply, an interrupted
partial frame and an oversized message that must still fail. All pass; the first
case aborts the unchanged reader. The genuine Chromium fixture also passes HTML,
CSS pixels, JavaScript, navigation, companion popups, forms and cancellation.

## Direct hardware regression and live-check follow-up

The CI now exports exact ARM `real_engine`, `capabilities` and `transport` test
executables as `browser-tests-aarch64`, plus a smaller `browser-check-aarch64`
artifact for iteration. All eight regression tests passed as the normal user on
the actual Linux 4.4.189 handheld: transport framing/limits, rendered HTML/CSS/JS,
forms, navigation/popups, prompts, permission handling, bounded uploads/downloads
and cancellation, tab freezing/audio continuity, and cookies/localStorage/IndexedDB
across separate processes. Geolocation in the fixture is explicitly emulated;
this does not assert a physical GPS service. The normal signed 0.8.3 launcher was
restored with the identical active/previous state afterward. The corresponding
production sources are unchanged from the installed 0.8.3 build.

The live checker no longer forces a PNG between hit-testing an article link and
clicking it. It also matches the native input sequence and its 24/8-fps active/idle
transfer budget. Inspection is not treated as user activity. Visible-control
probing now rejects unrelated labels before requesting ancestor styles, retaining
all visibility/hit-test checks for matching candidates. No timeout, memory floor,
site identity, permission or consent protection is weakened.

All three ordinary site checks passed on the handheld with immediate link input:
Slashdot 23.98 seconds, CNN article/Back 91.50 seconds, and OpenStreetMap 19.77
seconds, with a sampled minimum of 299,964 KiB available. This CNN run navigated
before a privacy dialog appeared, so it does not prove the later consent path.
The explicit `--site cnn-consent` case waits for the site's own visible controls
and requires optional-cookie rejection before article navigation. That harder
case passes on the Mac but still fails on the handheld with a memory guard or
renderer-command timeout. Keep it as an open acceptance case.

Isolated startup-option experiments tried disabling BackForwardCache, a 128 MiB
V8 old-space limit, and one raster thread. Some VM cases passed, but a combined
cache/heap case failed, and all three individual hardware cases failed. Their
profiles, command lines and memory reports are retained. These results do not
establish a reliable fix; none of those options is adopted in the runtime. The
next investigation should measure per-process memory/heap during cold consent
rather than lower the safety floor or relabel an ordinary-navigation pass.

## Remaining public-release scope

The handheld promoted the signed runtime to healthy active 0.8 and retained
`0.7.0-2c115912ff49` as its previous release. Engine 1.0.0 lives in permanent
Linux home storage; browser data was moved from the preview into its normal
location. A real reboot returned to the exact signed 0.8 runtime, with the engine and
profile present, no transient preview service, and unchanged bootstrap,
supervisor and primary-boot configuration. A final audit found that screenshot
output used the release working directory; the 0.8.1 follow-up moves captures
to user storage and adds a native-screenshot-plus-signature regression.

The 0.8.1 ARM VM additionally captured a real native frame into user storage and
then reverified the entire signed release successfully, followed by crash,
interrupted-trial and tamper recovery. These screenshot/signature checks and a
full reboot have now also passed on the actual handheld over Wi-Fi, without an
SD-card swap.

The installed engine also passes its sandbox report after migration in the ARM
VM. Its AppArmor allowance names only the verified helper's exact installed path;
global namespace policy remains unchanged. The VM is stopped after testing.

Keep the browser PR in draft and blog additions unpublished pending review.
Wider-device compatibility, CNN reliability on the handheld, hardware web graphics,
long battery/thermal testing and public engine/source distribution remain explicit
limitations. Camera/microphone devices are not exposed; blob/data downloads,
pause/resume and a general file manager are not implemented. Permission approval
cannot add missing hardware or services.

See [browser usage](../crates/cartridge-browser/README.md),
[component delivery](browser-component-delivery.md), and
[source provenance](../crates/cartridge-browser/engine/sources.md).
