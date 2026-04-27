//! Minimal launcher sound feedback. Plays short synthesized sine tones
//! on navigation and launch -- no asset files needed.
//!
//! The audio device is initialized lazily on first sound. If init fails
//! (headless test environments, no audio hardware), all subsequent calls
//! become no-ops; the launcher never panics on audio errors.

use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::time::Duration;

pub struct UiSounds {
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    init_attempted: bool,
    init_failed: bool,
    enabled: bool,
}

impl UiSounds {
    pub fn new() -> Self {
        Self {
            _stream: None,
            handle: None,
            sink: None,
            init_attempted: false,
            init_failed: false,
            enabled: true,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        // Stop anything currently playing if we're being muted.
        if !enabled {
            if let Some(s) = &self.sink {
                s.stop();
            }
        }
    }

    /// Quiet click for D-pad / navigation.
    pub fn click(&mut self) {
        self.play_tone(720.0, 18, 0.15);
    }

    /// Confirmation chirp for A / select.
    pub fn confirm(&mut self) {
        self.play_tone(960.0, 28, 0.18);
    }

    /// Lower pop for B / back / cancel.
    pub fn back(&mut self) {
        self.play_tone(440.0, 24, 0.18);
    }

    /// Two-tone ascending chirp for app launch.
    pub fn launch(&mut self) {
        self.play_tone(660.0, 35, 0.20);
        self.play_tone(990.0, 60, 0.22);
    }

    fn play_tone(&mut self, freq_hz: f32, duration_ms: u64, amplitude: f32) {
        if !self.enabled {
            return;
        }
        if !self.ensure_init() {
            return;
        }
        let Some(sink) = &self.sink else { return };
        let source = rodio::source::SineWave::new(freq_hz)
            .take_duration(Duration::from_millis(duration_ms))
            .amplify(amplitude)
            // Quick fade out so the tone doesn't click on stop.
            .fade_in(Duration::from_millis(2));
        sink.append(source);
    }

    fn ensure_init(&mut self) -> bool {
        if self.handle.is_some() {
            return true;
        }
        if self.init_failed || self.init_attempted {
            return self.handle.is_some();
        }
        // Skip audio device init in headless mode (snapshot/perf-bench).
        // OutputStream::try_default() can stall for >1s on macOS the first
        // time; we don't want to penalize benchmarks for that.
        if std::env::var("CARTRIDGE_HIDDEN").as_deref() == Ok("1") {
            self.init_attempted = true;
            self.init_failed = true;
            return false;
        }
        self.init_attempted = true;
        match OutputStream::try_default() {
            Ok((stream, handle)) => {
                let sink = Sink::try_new(&handle).ok();
                self._stream = Some(stream);
                self.handle = Some(handle);
                self.sink = sink;
                true
            }
            Err(e) => {
                log::warn!("UI sounds disabled: {e}");
                self.init_failed = true;
                false
            }
        }
    }
}

impl Default for UiSounds {
    fn default() -> Self {
        Self::new()
    }
}
