# Original video regression fixture

`media-pattern.mp4` is generated locally from FFmpeg's synthetic `testsrc2`
source. It contains no external media and no audio: H.264 (`libx264`),
1280 × 720, 24 fps, 48 frames, 2.000 seconds, YUV 4:2:0.
The MP4 metadata is placed first for HTTP playback and seeking.

Exact command, run from the repository root with Homebrew FFmpeg 8.0.1:

~~~sh
/opt/homebrew/bin/ffmpeg -hide_banner -loglevel error \
  -f lavfi -i testsrc2=size=1280x720:rate=24 -t 2 -an \
  -c:v libx264 -preset medium -crf 32 -pix_fmt yuv420p -g 24 \
  -movflags +faststart -map_metadata -1 -y \
  crates/cartridge-browser/tests/fixtures/media-pattern.mp4
~~~

Size: **206,437 bytes** (below 400 KiB).

SHA-256: `ecc00514b007e80cec96351849829a9215d5e418d65d5c616136fc52d383558a`

Different FFmpeg/libx264 builds may produce different encoded bytes.

The ignored `media` integration test serves the fixture over loopback with HTTP
byte ranges. Its actual HTML page has two muted, looping videos and native-click
Play/Pause/Seek controls. It checks media clocks, decoded/presented frames,
color inside both transferred video regions at 720 × 576, stable pause,
seeking while paused, resumed playback, trusted input, and Chromium shutdown.
Failed runs retain their printed temporary profile path and `engine.log`.

~~~sh
cargo test --locked -p cartridge-browser --test media -- --ignored --nocapture
~~~
