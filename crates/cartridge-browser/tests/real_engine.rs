//! Explicit opt-in: requires a locally installed, sandboxed Chromium engine.
use cartridge_browser::engine::{Action, Session, Snapshot, discover};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn wait(session: &mut Session, expression: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        session.pump(Duration::from_millis(20)).unwrap();
        if session.evaluate(expression).unwrap_or(Value::Null) == Value::Bool(true) {
            return;
        }
    }
    panic!(
        "Timed out: {expression}; page={:?}",
        session.shared.lock().unwrap().info
    );
}
#[test]
#[ignore = "requires Chromium; run cargo test -p cartridge-browser --test real_engine -- --ignored --nocapture"]
fn real_html_css_javascript_links_forms_history_and_pixels() {
    let _ = env_logger::try_init();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let serve = base.clone();
    let slow_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let slow_release = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_started = slow_started.clone();
    let server_release = slow_release.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => break,
            };
            let serve = serve.clone();
            let server_started = server_started.clone();
            let server_release = server_release.clone();
            std::thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let count = match stream.read(&mut chunk) {
                        Ok(n) if n > 0 => n,
                        _ => return,
                    };
                    request.extend_from_slice(&chunk[..count]);
                    if request.len() > 8192 {
                        return;
                    }
                }
                let count = request.len();
                let request = String::from_utf8_lossy(&request[..count]);
                let path = request.split_whitespace().nth(1).unwrap_or("/");
                log::debug!("Fixture request: {path} ({count} bytes)");
                if path == "/slow" {
                    server_started.store(true, std::sync::atomic::Ordering::Relaxed);
                    let until = Instant::now() + Duration::from_secs(6);
                    while !server_release.load(std::sync::atomic::Ordering::Relaxed)
                        && Instant::now() < until
                    {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                let page = if path == "/server-error" {
                    "<title>Service unavailable</title><h1>Upstream server error</h1>".to_string()
                } else if path.starts_with("/frame") {
                    "<body>SUBFRAME<script>history.replaceState({},'', '/frame?ad-frame');</script>"
                        .to_string()
                } else if path == "/second" {
                    "<title>Second page</title><h1>LINK WORKED</h1>".to_string()
                } else if path.starts_with("/search?") {
                    format!("<title>Form result</title><h1>{path}</h1>")
                } else {
                    format!(
                        r#"<!doctype html><title>Browser acceptance</title>
<style>body{{margin:0;background:rgb(17,51,170);color:white;font:24px sans-serif}}main{{display:grid;grid-template-columns:1fr 1fr;gap:20px;padding:28px}}input,button{{font:24px sans-serif}}a{{color:white}}@media(max-width:500px){{body{{background:rgb(170,30,20)}}}}</style>
<main><h1>Actual HTML + CSS</h1><span id="js">PENDING</span><a id="next" href="{serve}/second">Next page</a>
<a id="popup" href="{serve}/second" target="_blank">Open new window</a><button id="dialog" onclick="document.body.dataset.dialogResult=String(confirm('Continue?'))">Dialog</button><form action="/search"><input id="query" name="q"><button>Search</button></form></main>
<iframe src="{serve}/frame" style="position:absolute;width:1px;height:1px;top:0"></iframe>
<script>document.getElementById('js').textContent='JAVASCRIPT WORKED';</script>"#
                    )
                };
                let status = if path == "/server-error" {
                    "503 Service Unavailable"
                } else {
                    "200 OK"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nSet-Cookie: test_cookie=stored; SameSite=Lax\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                    page.len()
                );
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let profile = std::env::temp_dir().join(format!(
        "cartridge-browser-test-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&profile).unwrap();
    let shared = Arc::new(Mutex::new(Snapshot::default()));
    let mut session = Session::start(&discover(None).unwrap(), &profile, shared.clone()).unwrap();
    println!("Engine: {}", shared.lock().unwrap().info.engine_version);
    #[cfg(target_os = "linux")]
    {
        // Offscreen display must retain the normal browser identity without
        // hiding the automation exposed by our inherited DevTools pipe.
        let identity = session
            .evaluate("({agent:navigator.userAgent,webdriver:navigator.webdriver})")
            .unwrap();
        assert!(!identity["agent"].as_str().unwrap().contains("HeadlessChrome"));
        assert_eq!(identity["webdriver"], true);
    }
    session.action(Action::Navigate(base.clone())).unwrap();
    wait(
        &mut session,
        "document.getElementById('js')?.textContent==='JAVASCRIPT WORKED'",
    );
    wait(
        &mut session,
        "document.querySelector('iframe')?.contentWindow.location.search==='?ad-frame'",
    );
    for _ in 0..5 {
        session.pump(Duration::from_millis(20)).unwrap();
    }
    assert_eq!(
        shared.lock().unwrap().info.url,
        format!("{base}/"),
        "subframe navigation changed the main address"
    );
    assert_eq!(
        session
            .evaluate("getComputedStyle(document.querySelector('main')).display")
            .unwrap(),
        "grid"
    );
    assert!(
        session
            .evaluate("document.cookie")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("test_cookie=stored")
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        session.pump(Duration::from_millis(20)).unwrap();
        if shared.lock().unwrap().frame_count > 0 {
            break;
        }
        assert!(Instant::now() < deadline, "No rendered web pixels received");
    }
    let frame = shared.lock().unwrap().frame.take().unwrap();
    assert_eq!((frame.width, frame.height), (720, 576));
    // A genuine CSS-rendered background pixel, not an app imitation.
    let pixel = &frame.rgb[(550 * 720 + 700) * 3..(550 * 720 + 700) * 3 + 3];
    assert!(
        pixel[0] < 30 && (30..75).contains(&pixel[1]) && pixel[2] > 140,
        "wrong CSS pixels: {pixel:?}"
    );
    let output = std::env::var_os("CARTRIDGE_BROWSER_TEST_OUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| profile.join("captures"));
    std::fs::create_dir_all(&output).unwrap();
    image::save_buffer(
        output.join("real-webpage.png"),
        &frame.rgb,
        720,
        576,
        image::ColorType::Rgb8,
    )
    .unwrap();
    session.action(Action::Zoom(1.5)).unwrap();
    assert_eq!(session.evaluate("innerWidth").unwrap(), 480);
    assert_eq!(
        session
            .evaluate("getComputedStyle(document.body).backgroundColor")
            .unwrap(),
        "rgb(170, 30, 20)"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        session.pump(Duration::from_millis(20)).unwrap();
        let red = shared.lock().unwrap().frame.as_ref().is_some_and(|f| {
            assert_eq!(
                (f.width, f.height),
                (720, 576),
                "zoom lost native resolution"
            );
            // The bottom-right corner is a classic scrollbar on Linux. Probe
            // an unobstructed body margin, which is red at this viewport width.
            let offset = ((f.height as usize - 40) * f.width as usize + 10) * 3;
            let pixel = &f.rgb[offset..offset + 3];
            pixel[0] > 140 && pixel[1] < 70 && pixel[2] < 60
        });
        if red {
            break;
        }
        if Instant::now() >= deadline {
            if let Some(frame) = shared.lock().unwrap().frame.take() {
                image::save_buffer(
                    output.join("zoom-failed.png"),
                    &frame.rgb,
                    frame.width,
                    frame.height,
                    image::ColorType::Rgb8,
                )
                .unwrap();
            }
            panic!(
                "zoom changed layout but never delivered updated page pixels; see zoom-failed.png"
            );
        }
    }
    let point=session.evaluate("(()=>{let r=document.getElementById('next').getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]})()").unwrap();
    session
        .action(Action::Click(
            point[0].as_f64().unwrap() * 1.5,
            point[1].as_f64().unwrap() * 1.5,
        ))
        .unwrap();
    wait(&mut session, "document.title==='Second page'");
    session.action(Action::Back).unwrap();
    wait(&mut session, "document.title==='Browser acceptance'");
    session.action(Action::Zoom(0.75)).unwrap();
    assert_eq!(session.evaluate("innerWidth").unwrap(), 960);
    session.action(Action::Zoom(1.3)).unwrap();
    assert_eq!(session.evaluate("innerWidth").unwrap(), 554);
    session.action(Action::Zoom(1.0)).unwrap();
    assert_eq!(session.evaluate("innerWidth").unwrap(), 720);
    let point=session.evaluate("(()=>{let r=document.getElementById('popup').getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]})()").unwrap();
    session
        .action(Action::Click(
            point[0].as_f64().unwrap(),
            point[1].as_f64().unwrap(),
        ))
        .unwrap();
    wait(&mut session, "document.title==='Second page'");
    let targets = session
        .call("Target.getTargets", serde_json::json!({}))
        .unwrap();
    assert_eq!(
        targets["targetInfos"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["type"] == "page")
            .count(),
        1,
        "background page leaked"
    );
    session.action(Action::Back).unwrap();
    wait(&mut session, "document.title==='Browser acceptance'");
    let point=session.evaluate("(()=>{let r=document.getElementById('dialog').getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]})()").unwrap();
    session
        .action(Action::Click(
            point[0].as_f64().unwrap(),
            point[1].as_f64().unwrap(),
        ))
        .unwrap();
    assert!(shared.lock().unwrap().info.dialog.is_some());
    session.action(Action::Dialog(true)).unwrap();
    wait(&mut session, "document.body.dataset.dialogResult==='true'");
    session
        .evaluate("document.getElementById('query').focus();true")
        .unwrap();
    session.action(Action::FocusInfo(42)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while shared.lock().unwrap().info.focus_serial != 42 {
        session.pump(Duration::from_millis(20)).unwrap();
        assert!(
            Instant::now() < deadline,
            "fresh focus result was not returned"
        );
    }
    assert!(shared.lock().unwrap().info.editable);
    assert!(!shared.lock().unwrap().info.password);
    session.action(Action::Text("old query".into())).unwrap();
    session
        .action(Action::ReplaceText("hello world".into()))
        .unwrap();
    assert_eq!(
        session
            .evaluate("document.getElementById('query').value")
            .unwrap(),
        "hello world"
    );
    session.action(Action::Key("Enter".into(), false)).unwrap();
    wait(&mut session, "document.title==='Form result'");
    assert!(
        session
            .evaluate("location.search")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("hello+world")
    );
    session.action(Action::Back).unwrap();
    wait(&mut session, "document.title==='Browser acceptance'");
    session.action(Action::Forward).unwrap();
    wait(&mut session, "document.title==='Form result'");
    session
        .action(Action::Navigate(format!("{base}/server-error")))
        .unwrap();
    wait(&mut session, "document.title==='Service unavailable'");
    let deadline = Instant::now() + Duration::from_secs(2);
    while shared.lock().unwrap().info.http_status != Some(503) {
        session.pump(Duration::from_millis(20)).unwrap();
        assert!(
            Instant::now() < deadline,
            "server error status was not reflected in browser metadata"
        );
    }
    let before = Instant::now();
    session
        .action(Action::Navigate(format!("{base}/slow")))
        .unwrap();
    assert!(
        before.elapsed() < Duration::from_secs(1),
        "navigation blocked the input worker"
    );
    let deadline = Instant::now() + Duration::from_secs(4);
    while !slow_started.load(std::sync::atomic::Ordering::Relaxed) {
        session.pump(Duration::from_millis(20)).unwrap();
        assert!(
            Instant::now() < deadline,
            "slow fixture did not receive navigation"
        );
    }
    let before = Instant::now();
    session.action(Action::Stop).unwrap();
    assert!(
        before.elapsed() < Duration::from_secs(1),
        "Stop waited for the server response"
    );
    slow_release.store(true, std::sync::atomic::Ordering::Relaxed);
    println!(
        "PASS: actual HTML/CSS grid, JS, cookies, CSS pixels, mouse link, Back/Forward, text/forms, zoom, dialogs, new-window routing, main-frame address and cancellation during slow HTTP; screenshot {}",
        output.display()
    );
    drop(session);
    let _ = std::fs::remove_dir_all(profile);
}
