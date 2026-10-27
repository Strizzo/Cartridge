# Browser development checks — 27–28 October 2026

This is the 0.8 development branch, not a public stable release. Main and the
released 0.7 runtime remain unchanged. The handheld has permanent, reboot-verified
0.8.0 (`a38d5cedcfe935f01585bd5087895ba1a9a5f0e3`), built from `edcce76`.

The final screenshot-safe candidate is **0.8.1**, source
`4cafb388f1bad52f42d1cc6dd527c9dfdc7d1977`, CI merge
`bdb28e4f934c974a9bf97a8be447f63cb3268d52`. Its ARM binary SHA-256 is
`d21ce87d4b283e13d21bc10f4cb410e20785832b7e2944682faab979b3cf5718`.
[CI 40589298315](https://github.com/Strizzo/Cartridge/actions/runs/40589298315)
passes native and ARM jobs on its second attempt. The first attempt timed out
waiting 15 seconds for CI Chrome's initial reply; the fresh-runner retry passed
without disabling any sandbox. Mac real-Chrome checks also pass.

**The final patch is not installed yet.** The power guard refused staging at
28% and discharging; the last successful power read was 23% and discharging.
The 28 October SSH check timed out, so activation is still pending. Installed
0.8 was last verified healthy, with 0.7 as previous and no pending/trial state.
Reconnect charging and Wi-Fi, then use the already prepared signed kit. Until
0.8.1 is installed, avoid device F12/SIGUSR1 captures: the older screenshot path
writes into the signed release.

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
browser compatibility case; no identity, certificate, consent or access bypass
has been added to hide it.

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
interrupted-trial and tamper recovery. The final device activation/reboot awaits
charging; no SD-card swap is needed.

The installed engine also passes its sandbox report after migration in the ARM
VM. Its AppArmor allowance names only the verified helper's exact installed path;
global namespace policy remains unchanged. The VM is stopped after testing.

Keep the PR and blog unpublished pending review. Wider-device compatibility,
the CNN failure, hardware web graphics,
long battery/thermal testing and public engine/source distribution remain explicit
limitations. Camera/microphone devices are not exposed; blob/data downloads,
pause/resume and a general file manager are not implemented. Permission approval
cannot add missing hardware or services.

See [browser usage](../crates/cartridge-browser/README.md),
[component delivery](browser-component-delivery.md), and
[source provenance](../crates/cartridge-browser/engine/sources.md).
