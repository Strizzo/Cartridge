# CartridgeOS 0.6.2

Apps can now read both analog sticks independently through the optional
`on_stick(stick, x, y)` lifecycle callback. An 18% radial dead zone prevents
drift; movement stops on recenter, disconnect, focus loss and keyboard capture.
Older apps and the launcher retain left-stick navigation, and the physical
D-pad, face buttons, shoulder buttons and volume triggers remain available.

Frequency 1.2.0 uses left-stick map panning and right-stick up/down zoom, with
one level per deliberate tilt. Existing map buttons remain supported. Install
the OS update before the Frequency update, which declares runtime 0.6.2.

On an already bootstrapped CartridgeOS 0.6.1 handheld, use Settings → System
Update to check and download the signed release, then restart Cartridge to
activate it. Afterward, refresh the Store and update Frequency. The runtime
update does not modify the Linux kernel, Wi-Fi drivers, boot graphics, games,
saves or session supervisor. Existing app preferences and installed overrides
remain separate from the runtime release.

The native Mac simulator supports I/J/K/L for the left stick and T/F/G/H for
the right stick. `app-check` also accepts frame-numbered stick deflections and
a fixed update timestep, so pan speed, zoom latching and neutral idle behavior
can be reproduced without the handheld. SDL-mapped pads use named stick axes;
unmapped handheld pads default to raw axes 0/1 and 2/3. Hardware wiring still
needs a physical check after updating.
