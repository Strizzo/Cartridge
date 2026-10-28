//! Opt-in regression: resource tuning must preserve real Chromium video playback.
use cartridge_browser::engine::{Action, Session, Snapshot, discover};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const VIDEO: &[u8] = include_bytes!("fixtures/media-pattern.mp4");
// Everything except the videos is monochrome; there are no posters or autoplay.
const PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>Two real videos</title>
<style>
body{margin:0;background:#181818;color:#eee;font:20px sans-serif}
nav{position:absolute;left:20px;top:16px;display:flex;gap:16px}
button{font:20px sans-serif;color:#111;background:#ddd;padding:8px 20px}
video{position:absolute;top:80px;left:20px;width:320px;height:180px;background:#000}
video+video{left:360px}
</style>
<nav><button id="play">Play</button><button id="pause">Pause</button><button id="seek">Seek</button></nav>
<video muted loop playsinline preload="auto" src="/media-pattern.mp4"></video>
<video muted loop playsinline preload="auto" src="/media-pattern.mp4"></video>
<script>
const videos = [...document.querySelectorAll('video')];
const presented = [0, 0], seeks = [0, 0], clicks = [], errors = [];
videos.forEach((v, i) => {
  const frame = () => { presented[i]++; v.requestVideoFrameCallback(frame); };
  v.requestVideoFrameCallback(frame);
  v.addEventListener('seeked', () => seeks[i]++);
  v.addEventListener('error', () => errors.push(v.error.message));
});
for (const id of ['play', 'pause', 'seek']) {
  document.getElementById(id).addEventListener('click', e => {
    clicks.push({id, trusted:e.isTrusted});
    for (const v of videos) {
      if (id === 'play') v.play().catch(e => errors.push(String(e)));
      if (id === 'pause') v.pause();
      if (id === 'seek') v.currentTime = v.duration * (v.currentTime < v.duration / 2 ? 0.75 : 0.25);
    }
  });
}
function state() {
  return videos.map((v, i) => ({
    time:v.currentTime, duration:v.duration, paused:v.paused, seeking:v.seeking,
    decoded:v.getVideoPlaybackQuality().totalVideoFrames, presented:presented[i], seeks:seeks[i]
  }));
}
</script>"#;

fn serve(mut stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut request = Vec::new();
    let mut chunk = [0; 1024];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut chunk)?;
        if n == 0 || request.len() + n > 8192 {
            return Ok(());
        }
        request.extend_from_slice(&chunk[..n]);
    }
    let request = String::from_utf8_lossy(&request);
    let path = request.split_whitespace().nth(1).unwrap_or("/");
    let (status, kind, body, range) = match path {
        "/" => (
            "200 OK",
            "text/html; charset=utf-8",
            PAGE.as_bytes(),
            String::new(),
        ),
        "/media-pattern.mp4" => {
            // Chromium requests ordinary single byte ranges, including bytes=0-.
            let range = request.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("range").then_some(value.trim())
            });
            if let Some(range) = range {
                let bounds = range.strip_prefix("bytes=").and_then(|r| r.split_once('-'));
                let (start, end) = bounds
                    .and_then(|(start, end)| {
                        Some((
                            start.parse::<usize>().ok()?,
                            if end.is_empty() {
                                VIDEO.len() - 1
                            } else {
                                end.parse().ok()?
                            },
                        ))
                    })
                    .unwrap_or((VIDEO.len(), 0));
                if start > end || start >= VIDEO.len() {
                    write!(
                        stream,
                        "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        VIDEO.len()
                    )?;
                    return Ok(());
                }
                let end = end.min(VIDEO.len() - 1);
                (
                    "206 Partial Content",
                    "video/mp4",
                    &VIDEO[start..=end],
                    format!("Content-Range: bytes {start}-{end}/{}\r\n", VIDEO.len()),
                )
            } else {
                ("200 OK", "video/mp4", VIDEO, String::new())
            }
        }
        _ => (
            "404 Not Found",
            "text/plain",
            &b"not found"[..],
            String::new(),
        ),
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nAccept-Ranges: bytes\r\n{range}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
}

struct Server {
    base: String,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            let mut requests = Vec::new();
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => requests.push(thread::spawn(move || {
                        // Aborted media/preconnect requests may close early.
                        let _ = serve(stream);
                    })),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(e) => panic!("loopback accept: {e}"),
                }
            }
            for request in requests {
                request.join().unwrap();
            }
        });
        Self {
            base,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.worker.take().unwrap().join();
    }
}

fn wait(session: &mut Session, label: &str, expression: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        session.pump(Duration::from_millis(20)).unwrap();
        if session.evaluate(expression).unwrap_or(Value::Null) == true {
            return;
        }
        if Instant::now() >= deadline {
            let media = session.evaluate("({media:state(),errors,clicks})");
            panic!(
                "Timed out {label}: {:?}; media={media:?}",
                session.shared.lock().unwrap().info
            );
        }
    }
}

fn click(session: &mut Session, id: &str) {
    let point = session.evaluate(&format!(
        "(()=>{{const r=document.getElementById({}).getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]}})()", json!(id)
    )).unwrap();
    let (x, y) = (point[0].as_f64().unwrap(), point[1].as_f64().unwrap());
    session.action(Action::Pointer(x, y)).unwrap();
    session.action(Action::Button(x, y, true)).unwrap();
    session.action(Action::Button(x, y, false)).unwrap();
}

fn progress(session: &mut Session, before: &Value) {
    wait(
        session,
        "both decoders and media clocks advancing",
        &format!(
            "(()=>{{const before={before};return state().every((v,i)=>!v.paused && !v.seeking && v.decoded>=before[i].decoded+4 && v.presented>=before[i].presented+4 && (v.time-before[i].time+v.duration)%v.duration>0.35)}})()"
        ),
    );
}

fn stays_paused(session: &mut Session) {
    let before = session.evaluate("state()").unwrap();
    let until = Instant::now() + Duration::from_millis(700);
    while Instant::now() < until {
        session.pump(Duration::from_millis(20)).unwrap();
        let after = session.evaluate("state()").unwrap();
        for i in 0..2 {
            assert_eq!(after[i]["paused"], true, "video {i} resumed: {after}");
            assert_eq!(
                after[i]["seeking"], false,
                "video {i} still seeking: {after}"
            );
            assert!(
                (after[i]["time"].as_f64().unwrap() - before[i]["time"].as_f64().unwrap()).abs()
                    < 0.03,
                "video {i} advanced while paused: {before} -> {after}"
            );
        }
    }
}

#[test]
#[ignore = "requires local Chromium; run cargo test -p cartridge-browser --test media -- --ignored --nocapture"]
fn two_h264_videos_play_pause_seek_and_resume_via_native_mouse() {
    let _ = env_logger::try_init();
    assert!(VIDEO.len() < 400 * 1024);
    let server = Server::start();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let profile =
        std::env::temp_dir().join(format!("cartridge-media-{}-{nonce}", std::process::id()));
    // Keep the profile and engine.log on failure for diagnosis.
    println!("Media profile: {}", profile.display());
    let shared = Arc::new(Mutex::new(Snapshot::default()));
    let mut session = Session::start(&discover(None).unwrap(), &profile, shared.clone()).unwrap();
    println!("Engine: {}", shared.lock().unwrap().info.engine_version);
    session
        .action(Action::Navigate(server.base.clone()))
        .unwrap();
    wait(
        &mut session,
        "two loaded 720p H.264 videos",
        "typeof state==='function' && videos.length===2 && videos.every(v=>v.paused && v.muted && v.loop && v.readyState>=2 && v.videoWidth===1280 && v.videoHeight===720 && Math.abs(v.duration-2)<0.05)",
    );
    let before = session.evaluate("state()").unwrap();
    let frame_before = shared.lock().unwrap().frame_count;
    click(&mut session, "play");
    progress(&mut session, &before);

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        session.pump(Duration::from_millis(20)).unwrap();
        let colored = {
            let snapshot = shared.lock().unwrap();
            snapshot.frame_count > frame_before
                && snapshot.frame.as_ref().is_some_and(|frame| {
                    assert_eq!((frame.width, frame.height), (720, 576));
                    // Sample only inside each video, never page chrome/text. Requiring
                    // all three primaries rejects black/gray placeholders and posters.
                    [20, 360].into_iter().all(|left| {
                        let mut primaries = [0; 3];
                        for y in (90..250).step_by(4) {
                            for x in (left + 10..left + 310).step_by(4) {
                                let offset = (y * 720 + x) * 3;
                                let rgb = &frame.rgb[offset..offset + 3];
                                for c in 0..3 {
                                    if rgb[c] > 140
                                        && rgb[c] > rgb[(c + 1) % 3].saturating_add(60)
                                        && rgb[c] > rgb[(c + 2) % 3].saturating_add(60)
                                    {
                                        primaries[c] += 1;
                                    }
                                }
                            }
                        }
                        primaries.into_iter().all(|count| count > 30)
                    })
                })
        };
        if colored {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "No colored transferred frame inside both videos"
        );
    }
    println!(
        "Playing: {}; transferred frames: {}",
        session.evaluate("state()").unwrap(),
        shared.lock().unwrap().frame_count
    );
    click(&mut session, "pause");
    wait(
        &mut session,
        "both paused",
        "state().every(v=>v.paused && !v.seeking)",
    );
    stays_paused(&mut session);
    let before_seek = session.evaluate("state()").unwrap();
    click(&mut session, "seek");
    wait(
        &mut session,
        "both sought while paused",
        &format!(
            "(()=>{{const before={before_seek};return state().every((v,i)=>v.paused && !v.seeking && v.seeks>before[i].seeks && Math.abs(v.time-before[i].time)>0.4)}})()"
        ),
    );
    stays_paused(&mut session);
    let before_resume = session.evaluate("state()").unwrap();
    click(&mut session, "play");
    progress(&mut session, &before_resume);
    assert_eq!(
        session.evaluate("clicks").unwrap(),
        json!([
            {"id":"play","trusted":true}, {"id":"pause","trusted":true},
            {"id":"seek","trusted":true}, {"id":"play","trusted":true}
        ])
    );
    assert_eq!(session.evaluate("errors").unwrap(), json!([]));
    println!("Resumed: {}", session.evaluate("state()").unwrap());
    let shutdown = Instant::now();
    drop(session);
    assert!(
        shutdown.elapsed() < Duration::from_secs(5),
        "Chromium shutdown reached the forced-termination timeout"
    );
    drop(server);
    std::fs::remove_dir_all(&profile).unwrap();
    println!(
        "PASS: two decoded 720p videos, trusted mouse controls, colored native frames, stable pause, paused seek, resume, clean shutdown"
    );
}
