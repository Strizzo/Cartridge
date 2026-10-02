//! Actual network, decoder and audio-device smoke test. Silent by default.
use cartridge_lua::stream_audio::StreamPlayer;
use std::time::{Duration, Instant};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let url = args
        .get(1)
        .expect("usage: stream_probe HTTP_URL [seconds] [volume 0..1]");
    let seconds = args
        .get(2)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(5.0);
    let volume = args
        .get(3)
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.0);
    let player = StreamPlayer::new();
    player.volume(volume);
    player.play(url).unwrap();
    let deadline = Instant::now() + Duration::from_secs_f64(seconds + 25.0);
    let mut previous = String::new();
    loop {
        let status = player.status();
        if status.state != previous {
            println!("{}: {}", status.state, status.error);
            previous = status.state.clone();
        }
        if status.state == "playing" && status.seconds >= seconds {
            player.pause(true);
            assert_eq!(player.status().state, "paused");
            player.pause(false);
            assert_eq!(player.status().state, "playing");
            player.stop();
            assert_eq!(player.status().state, "stopped");
            println!(
                "PASS: decoded {:.2}s; audio output, pause, resume and stop",
                status.seconds
            );
            return;
        }
        if matches!(status.state.as_str(), "error" | "ended") || Instant::now() >= deadline {
            eprintln!(
                "FAIL: {}: {} (decoded {:.2}s)",
                status.state, status.error, status.seconds
            );
            std::process::exit(1);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
