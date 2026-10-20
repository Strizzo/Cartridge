//! Bounded internet audio. A single decoder worker owns the audio device; curl
//! transports HTTP(S) into a pipe, so stop/switch can interrupt blocked reads.
//! No shell, no complete-file downloads, and no audio work on the UI thread.
use mlua::prelude::*;
use rodio::{OutputStream, Sink, buffer::SamplesBuffer};
use std::io::{self, Read, Seek, SeekFrom};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use symphonia::core::{
    audio::SampleBuffer,
    errors::Error,
    io::{MediaSource, MediaSourceStream},
    probe::Hint,
};

#[derive(Clone, Debug)]
pub struct Status {
    pub state: String,
    pub error: String,
    pub url: String,
    pub volume: f32,
    pub seconds: f64,
}
struct Inner {
    generation: u64,
    pending: Option<(u64, String)>,
    quit: bool,
    status: Status,
    sink: Option<Arc<Sink>>,
    child: Option<Child>,
}
struct Shared {
    inner: Mutex<Inner>,
    wake: Condvar,
}
/// Dropping the last Lua handle terminates the stream even after a Lua error or
/// hot reload. The worker never owns this handle, only its shared control state.
pub struct StreamPlayer {
    shared: Arc<Shared>,
}
impl Default for StreamPlayer {
    fn default() -> Self {
        Self::new()
    }
}
impl StreamPlayer {
    pub fn new() -> Self {
        let shared = Arc::new(Shared {
            inner: Mutex::new(Inner {
                generation: 0,
                pending: None,
                quit: false,
                status: Status {
                    state: "stopped".into(),
                    error: String::new(),
                    url: String::new(),
                    volume: 0.7,
                    seconds: 0.0,
                },
                sink: None,
                child: None,
            }),
            wake: Condvar::new(),
        });
        let worker = shared.clone();
        std::thread::spawn(move || run_worker(worker));
        Self { shared }
    }
    pub fn play(&self, url: &str) -> Result<(), String> {
        validate_url(url)?;
        let mut s = self.shared.inner.lock().unwrap();
        s.generation = s.generation.wrapping_add(1);
        interrupt(&mut s);
        s.status.state = "connecting".into();
        s.status.error.clear();
        s.status.url = url.to_owned();
        s.status.seconds = 0.0;
        s.pending = Some((s.generation, url.to_owned()));
        self.shared.wake.notify_one();
        Ok(())
    }
    pub fn stop(&self) {
        let mut s = self.shared.inner.lock().unwrap();
        s.generation = s.generation.wrapping_add(1);
        s.pending = None;
        interrupt(&mut s);
        s.status.state = "stopped".into();
        s.status.error.clear();
        self.shared.wake.notify_one();
    }
    pub fn pause(&self, paused: bool) {
        let mut s = self.shared.inner.lock().unwrap();
        if let Some(sink) = &s.sink {
            if paused {
                sink.pause();
            } else {
                sink.play();
            }
            s.status.state = if paused { "paused" } else { "playing" }.into();
        }
    }
    pub fn volume(&self, value: f32) {
        if !value.is_finite() {
            return;
        }
        let mut s = self.shared.inner.lock().unwrap();
        s.status.volume = value.clamp(0.0, 1.0);
        if let Some(sink) = &s.sink {
            sink.set_volume(s.status.volume);
        }
    }
    pub fn status(&self) -> Status {
        let s = self.shared.inner.lock().unwrap();
        let mut status = s.status.clone();
        if status.state == "playing" && s.sink.as_ref().is_some_and(|sink| sink.empty()) {
            status.state = "buffering".into();
        }
        status
    }
}
impl Drop for StreamPlayer {
    fn drop(&mut self) {
        self.stop();
        self.shared.inner.lock().unwrap().quit = true;
        self.shared.wake.notify_one();
    }
}
fn interrupt(s: &mut Inner) {
    if let Some(sink) = s.sink.take() {
        sink.stop();
    }
    // kill is non-blocking; the worker reaps the process outside the UI lock.
    if let Some(child) = &mut s.child {
        let _ = child.kill();
    }
}
fn validate_url(url: &str) -> Result<(), String> {
    if url.len() > 4096 || url.bytes().any(|b| b.is_ascii_control()) {
        return Err("Stream URL is too long or contains control characters".into());
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or("Use an http:// or https:// audio stream")?;
    if rest.is_empty() || rest.starts_with('/') || rest.contains(' ') {
        return Err("Invalid stream URL".into());
    }
    Ok(())
}
fn current(shared: &Shared, id: u64) -> bool {
    let s = shared.inner.lock().unwrap();
    !s.quit && s.generation == id
}
fn run_worker(shared: Arc<Shared>) {
    loop {
        let (id, url) = {
            let mut s = shared.inner.lock().unwrap();
            while s.pending.is_none() && !s.quit {
                s = shared.wake.wait(s).unwrap();
            }
            if s.quit {
                return;
            }
            s.pending.take().unwrap()
        };
        let result = play_job(&shared, id, &url);
        let child = { shared.inner.lock().unwrap().child.take() };
        if let Some(mut child) = child {
            let _ = child.kill();
            let _ = child.wait();
        }
        let mut s = shared.inner.lock().unwrap();
        if s.generation == id {
            s.sink = None;
            s.status.state = if result.is_ok() { "ended" } else { "error" }.into();
            s.status.error = result
                .err()
                .unwrap_or_else(|| "Station ended the stream. Press A to reconnect.".into());
        }
    }
}
struct PipeSource<R: Read + Send + Sync> {
    reader: R,
}
impl<R: Read + Send + Sync> Read for PipeSource<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}
impl<R: Read + Send + Sync> Seek for PipeSource<R> {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "live stream"))
    }
}
impl<R: Read + Send + Sync> MediaSource for PipeSource<R> {
    fn is_seekable(&self) -> bool {
        false
    }
    fn byte_len(&self) -> Option<u64> {
        None
    }
}
fn play_job(shared: &Shared, id: u64, url: &str) -> Result<(), String> {
    let mut child = Command::new("curl")
        .args([
            "--disable",
            "--silent",
            "--fail",
            "--location",
            "--max-redirs",
            "5",
            "--connect-timeout",
            "8",
            "--speed-limit",
            "512",
            "--speed-time",
            "12",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--user-agent",
            "Cartridge-Frequency/1.0",
            "--header",
            "Icy-MetaData: 0",
            "--url",
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot start stream transport (curl is required): {e}"))?;
    let stdout = child.stdout.take().ok_or("Stream pipe unavailable")?;
    {
        let mut s = shared.inner.lock().unwrap();
        if s.quit || s.generation != id {
            let _ = child.kill();
            drop(s);
            let _ = child.wait();
            return Ok(());
        }
        s.child = Some(child);
    }
    let source = Box::new(PipeSource { reader: stdout });
    let stream = MediaSourceStream::new(source, Default::default());
    let metadata = symphonia::core::meta::MetadataOptions {
        limit_metadata_bytes: symphonia::core::meta::Limit::Maximum(128 * 1024),
        limit_visual_bytes: symphonia::core::meta::Limit::Maximum(0),
    };
    let mut format = symphonia::default::get_probe()
        .format(&Hint::new(), stream, &Default::default(), &metadata)
        .map_err(|e| {
            format!("Station unavailable or unsupported stream ({e}). Try another station.")
        })?
        .format;
    let track = format
        .default_track()
        .ok_or("Station has no supported audio track")?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &Default::default())
        .map_err(|e| format!("Unsupported station codec: {e}"))?;
    if !current(shared, id) {
        return Ok(());
    }
    let (_output, handle) =
        OutputStream::try_default().map_err(|e| format!("Audio device: {e}"))?;
    let sink = Arc::new(Sink::try_new(&handle).map_err(|e| format!("Audio output: {e}"))?);
    {
        let mut s = shared.inner.lock().unwrap();
        if s.quit || s.generation != id {
            return Ok(());
        }
        sink.set_volume(s.status.volume);
        s.sink = Some(sink.clone());
        s.status.state = "buffering".into();
    }
    let mut started = false;
    let mut failures = 0;
    loop {
        if !current(shared, id) {
            return Ok(());
        }
        // Bounded decoded queue: at most 12 audio packets; pause never downloads
        // an entire programme into RAM. Streaming backpressure reaches curl.
        if sink.len() >= 12 || sink.is_paused() {
            std::thread::sleep(Duration::from_millis(15));
            continue;
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
                while current(shared, id) && !sink.empty() {
                    std::thread::sleep(Duration::from_millis(20));
                }
                return if started {
                    Ok(())
                } else {
                    Err("Station returned no audio".into())
                };
            }
            Err(e) => return Err(format!("Stream interrupted: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        if packet.data.len() > 1024 * 1024 {
            return Err("Station audio packet exceeds 1 MiB limit".into());
        }
        let decoded = match decoder.decode(&packet) {
            Ok(audio) => {
                failures = 0;
                audio
            }
            Err(Error::DecodeError(_)) if failures < 20 => {
                failures += 1;
                continue;
            }
            Err(e) => return Err(format!("Cannot decode this station: {e}")),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count();
        if !(1..=2).contains(&channels) || spec.rate > 96000 || decoded.capacity() > 65536 {
            return Err("Station audio exceeds supported stereo/96 kHz limits".into());
        }
        let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        samples.copy_interleaved_ref(decoded);
        if !current(shared, id) {
            return Ok(());
        }
        let seconds = samples.len() as f64 / channels as f64 / spec.rate as f64;
        sink.append(SamplesBuffer::new(
            channels as u16,
            spec.rate,
            samples.samples().to_vec(),
        ));
        let mut s = shared.inner.lock().unwrap();
        if s.generation == id {
            if !started {
                s.status.state = "playing".into();
                started = true;
            }
            s.status.seconds += seconds;
        }
    }
}

/// Offline HTTP fixtures must never open a real broadcaster connection.
pub fn register_with_fixture(lua: &Lua, offline: bool) -> LuaResult<()> {
    if !offline {
        return register(lua);
    }
    let audio: LuaTable = lua.globals().get("audio")?;
    let state = std::rc::Rc::new(std::cell::RefCell::new((
        String::from("stopped"),
        String::new(),
        0.7f32,
    )));
    let s = state.clone();
    audio.set(
        "stream",
        lua.create_function(move |_, url: String| {
            validate_url(&url).map_err(LuaError::external)?;
            let mut s = s.borrow_mut();
            s.0 = "error".into();
            s.1 = url;
            Ok(())
        })?,
    )?;
    let s = state.clone();
    audio.set(
        "stop_stream",
        lua.create_function(move |_, ()| {
            s.borrow_mut().0 = "stopped".into();
            Ok(())
        })?,
    )?;
    audio.set("pause_stream", lua.create_function(|_, _: bool| Ok(()))?)?;
    let s = state.clone();
    audio.set(
        "set_stream_volume",
        lua.create_function(move |_, v: f32| {
            if v.is_finite() {
                s.borrow_mut().2 = v.clamp(0.0, 1.0);
            }
            Ok(())
        })?,
    )?;
    audio.set(
        "stream_status",
        lua.create_function(move |lua, ()| {
            let s = state.borrow();
            let t = lua.create_table()?;
            t.set("state", s.0.clone())?;
            t.set("url", s.1.clone())?;
            t.set("volume", s.2)?;
            t.set("seconds", 0)?;
            t.set(
                "error",
                if s.0 == "error" {
                    "Offline fixture: run the live simulator to hear this station."
                } else {
                    ""
                },
            )?;
            Ok(t)
        })?,
    )?;
    Ok(())
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    let audio: LuaTable = lua.globals().get("audio")?;
    let player = std::rc::Rc::new(StreamPlayer::new());
    let p = player.clone();
    audio.set(
        "stream",
        lua.create_function(move |_, url: String| p.play(&url).map_err(LuaError::external))?,
    )?;
    let p = player.clone();
    audio.set(
        "stop_stream",
        lua.create_function(move |_, ()| {
            p.stop();
            Ok(())
        })?,
    )?;
    let p = player.clone();
    audio.set(
        "pause_stream",
        lua.create_function(move |_, paused: bool| {
            p.pause(paused);
            Ok(())
        })?,
    )?;
    let p = player.clone();
    audio.set(
        "set_stream_volume",
        lua.create_function(move |_, volume: f32| {
            p.volume(volume);
            Ok(())
        })?,
    )?;
    audio.set(
        "stream_status",
        lua.create_function(move |lua, ()| {
            let s = player.status();
            let t = lua.create_table()?;
            t.set("state", s.state)?;
            t.set("error", s.error)?;
            t.set("url", s.url)?;
            t.set("volume", s.volume)?;
            t.set("seconds", s.seconds)?;
            Ok(t)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonseekable_radio_formats_decode_real_audio() {
        // Synthesised sine-wave fixtures, not broadcaster recordings.
        for (name, bytes) in [
            (
                "MP3",
                include_bytes!("../tests/audio-fixtures/tone.mp3").as_slice(),
            ),
            (
                "AAC-LC",
                include_bytes!("../tests/audio-fixtures/tone.aac").as_slice(),
            ),
            (
                "Ogg Vorbis",
                include_bytes!("../tests/audio-fixtures/tone.ogg").as_slice(),
            ),
        ] {
            let source = PipeSource {
                reader: std::io::Cursor::new(bytes),
            };
            let mut format = symphonia::default::get_probe()
                .format(
                    &Hint::new(),
                    MediaSourceStream::new(Box::new(source), Default::default()),
                    &Default::default(),
                    &Default::default(),
                )
                .expect(name)
                .format;
            let track = format.default_track().unwrap();
            let mut decoder = symphonia::default::get_codecs()
                .make(&track.codec_params, &Default::default())
                .expect(name);
            let mut frames = 0;
            while let Ok(packet) = format.next_packet() {
                let decoded = decoder.decode(&packet).expect(name);
                assert_eq!(decoded.spec().channels.count(), 2);
                assert_eq!(decoded.spec().rate, 44100);
                frames += decoded.frames();
            }
            assert!(frames >= 12000, "{name}: only {frames} samples");
        }
    }
    #[test]
    fn offline_fixture_rejects_playback_without_transport() {
        let lua = Lua::new();
        lua.globals()
            .set("audio", lua.create_table().unwrap())
            .unwrap();
        register_with_fixture(&lua, true).unwrap();
        lua.load(
            r#"
            audio.stream('https://example.invalid/test.mp3')
            assert(audio.stream_status().state == 'error')
            assert(audio.stream_status().error:find('Offline fixture'))
            audio.stop_stream()
            assert(audio.stream_status().state == 'stopped')
        "#,
        )
        .exec()
        .unwrap();
    }
    #[test]
    fn stream_urls_only_accept_bounded_http_sources() {
        for url in [
            "file:///etc/passwd",
            "--help",
            "ftp://radio",
            "https:///bad",
            "http://a\nX: b",
            "http://",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
        assert!(validate_url("https://radio.example:443/live?quality=128").is_ok());
    }
    #[test]
    fn rapid_station_switch_only_keeps_latest_request() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let player = StreamPlayer::new();
        let base = format!("http://{}", listener.local_addr().unwrap());
        for i in 0..50 {
            player.play(&format!("{base}/{i}")).unwrap();
        }
        assert_eq!(player.status().url, format!("{base}/49"));
        let shared = player.shared.clone();
        player.stop();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while shared.inner.lock().unwrap().child.is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "old transport remained after switch/stop"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(player.status().state, "stopped");
        assert!(shared.inner.lock().unwrap().pending.is_none());
    }
    #[test]
    fn stop_and_drop_interrupt_a_stalled_http_stream() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let player = StreamPlayer::new();
        player
            .play(&format!("http://{}/stream", listener.local_addr().unwrap()))
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let _socket = loop {
            if let Ok((socket, _)) = listener.accept() {
                break socket;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        let start = std::time::Instant::now();
        player.stop();
        assert!(start.elapsed() < Duration::from_millis(100));
        assert_eq!(player.status().state, "stopped");
        let shared = player.shared.clone();
        drop(player);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while shared.inner.lock().unwrap().child.is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "transport did not terminate"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
