# Frequency

A controller-operated atlas of internet radio. Browse Radio Browser by country,
genre or station name, navigate reported map locations, save favorites, and play
streams directly from broadcasters. Map pins appear only where the directory
provides coordinates; stations without coordinates remain available in the list.

- D-pad: choose stations or navigate the map/menus.
- A: play, pause or resume. B: stop or go back.
- X: save/remove a favorite. Y: search.
- L1/R1: genre. L2/R2: volume.
- Start: countries, favorites, history, paging, settings and station details.
- Select: exit; when the keyboard is open, cancel editing instead.

The app never autoplays on launch. Favorites and the last directory page are
cached, but listening needs internet access. Failed stations can be retried or
replaced with another station. Settings also accept a custom direct stream URL.

Native decoding supports MP3, AAC-LC, Ogg Vorbis, FLAC and WAV/PCM. HLS, HE-AAC
and Opus are unsupported. Prefer MP3 when a station fails. Radio Browser labels
cannot guarantee codec compatibility or access from every region. The firmware
needs `curl` for streaming transport.

Run from the Cartridge source root:

```sh
./sim.sh app lua_cartridges/frequency
./sim.sh app lua_cartridges/frequency --fixture sim/fixtures/frequency.json
```

Fixture mode disables actual radio playback. The full API, attribution, storage
schema and verification instructions are in `docs/frequency.md` in the source
repository. The map uses public-domain Natural Earth data; station metadata comes
from Radio Browser. Stream content and station marks belong to their owners.
