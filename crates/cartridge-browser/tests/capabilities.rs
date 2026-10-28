//! Real-engine capability fixtures. No browser responses or page execution mocked.
use cartridge_browser::engine::{Action, MAX_FILE_BYTES, Permission, Session, Snapshot, discover};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn pump_until(s: &mut Session, label: &str, check: impl Fn(&Snapshot) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        s.pump(Duration::from_millis(10)).unwrap();
        if check(&s.shared.lock().unwrap()) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Timed out {label}: {:?}",
            s.shared.lock().unwrap().info
        );
    }
}
fn wait(s: &mut Session, expression: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        s.pump(Duration::from_millis(10)).unwrap();
        if s.evaluate(expression).unwrap_or(Value::Null) == true {
            return;
        }
        assert!(Instant::now() < deadline, "Timed out {expression}");
    }
}
fn click(s: &mut Session, id: &str) {
    let point=s.evaluate(&format!("(()=>{{let r=document.getElementById({}).getBoundingClientRect();return [r.x+r.width/2,r.y+r.height/2]}})()",json!(id))).unwrap();
    s.action(Action::Click(
        point[0].as_f64().unwrap(),
        point[1].as_f64().unwrap(),
    ))
    .unwrap();
}
struct Fixture {
    base: String,
    profile: std::path::PathBuf,
    session: Session,
}
impl Fixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                std::thread::spawn(move || {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut chunk = [0; 1024];
                    while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                        let Ok(n) = stream.read(&mut chunk) else {
                            return;
                        };
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&chunk[..n]);
                        if bytes.len() > 16384 {
                            return;
                        }
                    }
                    let request = String::from_utf8_lossy(&bytes);
                    let path = request.split_whitespace().nth(1).unwrap_or("/");
                    if path == "/file"
                        || path == "/huge"
                        || path == "/unknown"
                        || path == "/slowfile"
                    {
                        let length = if path == "/huge" {
                            MAX_FILE_BYTES + 1
                        } else {
                            30
                        };
                        let size = if path == "/unknown" || path == "/slowfile" {
                            String::new()
                        } else {
                            format!("Content-Length: {length}\r\n")
                        };
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"fixture.txt\"\r\n{size}Connection: close\r\n\r\n"
                        );
                        if path == "/slowfile" {
                            std::thread::sleep(Duration::from_secs(3));
                        }
                        if path == "/unknown" {
                            let buf = [b'x'; 32768];
                            for _ in 0..1025 {
                                if stream.write_all(&buf).is_err() {
                                    break;
                                }
                            }
                        } else {
                            let _ = stream.write_all(b"actual HTTP attachment payload");
                        }
                        return;
                    }
                    let body = r#"<!doctype html><title>Native capability fixture</title>
<style>body{font:20px sans-serif;background:#123;color:white}button,a,input{display:block;margin:6px;height:32px}</style>
<button id="prompt" onclick="window.answer=prompt('Enter Unicode text','default text')">Prompt</button>
<button id="audio" onclick="window.audio=new AudioContext();let tone=audio.createOscillator();let gain=audio.createGain();gain.gain.value=0.001;tone.connect(gain).connect(audio.destination);tone.start();audio.resume()">Audio</button>
<button id="confirm" onclick="window.answer=confirm('Continue?')">Confirm</button>
<input id="upload" type="file" multiple onchange="window.uploaded=[...this.files].map(f=>f.name);window.fileText=this.files[0]?.text()">
<a id="file" href="/file">Download</a><a id="huge" href="/huge">Oversized</a><a id="unknown" href="/unknown">Unknown length</a><a id="slowfile" href="/slowfile">Slow file</a>
<script>window.ticks=0;setInterval(()=>{window.ticks++;},50);</script>"#;
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let profile = std::env::temp_dir().join(format!(
            "browser-capability-{}-{nonce}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let shared = Arc::new(Mutex::new(Snapshot::default()));
        let mut session = Session::start(&discover(None).unwrap(), &profile, shared).unwrap();
        println!(
            "Real engine {}",
            session.shared.lock().unwrap().info.engine_version
        );
        session.action(Action::Navigate(base.clone())).unwrap();
        wait(&mut session, "document.title==='Native capability fixture'");
        Self {
            base,
            profile,
            session,
        }
    }
    fn reload(&mut self) {
        self.session
            .action(Action::Navigate(self.base.clone()))
            .unwrap();
        wait(
            &mut self.session,
            "document.title==='Native capability fixture'",
        );
    }
}

#[test]
#[ignore = "requires local Chromium"]
fn native_prompt_upload_permission_download_and_tab_lifecycle() {
    let mut f = Fixture::new();
    let s = &mut f.session;
    click(s, "prompt");
    pump_until(s, "prompt", |s| s.info.javascript_dialog.is_some());
    let p = s
        .shared
        .lock()
        .unwrap()
        .info
        .javascript_dialog
        .clone()
        .unwrap();
    assert_eq!(p.kind, "prompt");
    assert_eq!(p.default_text, "default text");
    assert_eq!(p.origin, f.base);
    s.action(Action::Prompt {
        id: p.id,
        text: Some("Hello 世界 🦀".into()),
    })
    .unwrap();
    wait(s, "window.answer==='Hello 世界 🦀'");
    click(s, "prompt");
    let id = s
        .shared
        .lock()
        .unwrap()
        .info
        .javascript_dialog
        .as_ref()
        .unwrap()
        .id;
    s.action(Action::Prompt { id, text: None }).unwrap();
    wait(s, "window.answer===null");
    click(s, "prompt");
    let id = s
        .shared
        .lock()
        .unwrap()
        .info
        .javascript_dialog
        .as_ref()
        .unwrap()
        .id;
    s.action(Action::Prompt {
        id,
        text: Some(String::new()),
    })
    .unwrap();
    wait(s, "window.answer===''");

    assert_eq!(
        s.evaluate("navigator.permissions.query({name:'geolocation'}).then(p=>p.state)")
            .unwrap(),
        "denied"
    );
    s.action(Action::RequestPermission(Permission::Location))
        .unwrap();
    let p = s.shared.lock().unwrap().info.permission.clone().unwrap();
    assert_eq!(p.origin, f.base);
    s.action(Action::PermissionDecision {
        id: p.id,
        allow: false,
    })
    .unwrap();
    assert_eq!(
        s.evaluate("navigator.permissions.query({name:'geolocation'}).then(p=>p.state)")
            .unwrap(),
        "denied"
    );
    s.action(Action::RequestPermission(Permission::Location))
        .unwrap();
    let id = s
        .shared
        .lock()
        .unwrap()
        .info
        .permission
        .as_ref()
        .unwrap()
        .id;
    s.action(Action::PermissionDecision { id, allow: true })
        .unwrap();
    s.call(
        "Emulation.setGeolocationOverride",
        json!({"latitude":49.6,"longitude":6.1,"accuracy":10}),
    )
    .unwrap();
    assert_eq!(s.evaluate("new Promise(resolve=>navigator.geolocation.getCurrentPosition(p=>resolve(p.coords.latitude),e=>resolve(e.message)))").unwrap(),49.6);
    assert_eq!(
        s.evaluate("navigator.permissions.query({name:'camera'}).then(p=>p.state)")
            .unwrap(),
        "denied"
    );

    let files = f.profile.join("files");
    std::fs::write(files.join("upload.txt"), "native selected bytes").unwrap();
    std::os::unix::fs::symlink("/etc/passwd", files.join("escape.txt")).unwrap();
    click(s, "upload");
    pump_until(s, "file chooser", |s| s.info.upload.is_some());
    let p = s.shared.lock().unwrap().info.upload.clone().unwrap();
    assert_eq!(p.origin, f.base);
    assert!(p.multiple);
    assert_eq!(p.files.len(), 1);
    assert!(
        s.action(Action::Upload {
            id: p.id,
            names: vec!["../secret".into()]
        })
        .is_err()
    );
    // Invalid selection is rejected and a subsequent cancel still closes the chooser.
    s.action(Action::Upload {
        id: p.id,
        names: vec![],
    })
    .unwrap();
    click(s, "upload");
    pump_until(s, "file chooser", |s| s.info.upload.is_some());
    let id = s.shared.lock().unwrap().info.upload.as_ref().unwrap().id;
    s.action(Action::Upload {
        id,
        names: vec!["upload.txt".into()],
    })
    .unwrap();
    wait(s, "window.uploaded?.[0]==='upload.txt'");
    assert_eq!(
        s.evaluate("window.fileText").unwrap(),
        "native selected bytes"
    );
    s.evaluate("document.getElementById('upload').focus();true")
        .unwrap();
    s.action(Action::FocusInfo(77)).unwrap();
    pump_until(s, "file input focus", |s| s.info.focus_serial == 77);
    assert!(
        !s.shared.lock().unwrap().info.editable,
        "file input was treated as a text field"
    );
    s.action(Action::Key("Enter".into(), false)).unwrap();
    pump_until(s, "controller file chooser", |s| s.info.upload.is_some());
    let id = s.shared.lock().unwrap().info.upload.as_ref().unwrap().id;
    s.action(Action::Upload { id, names: vec![] }).unwrap();
    assert_eq!(
        s.evaluate("document.getElementById('upload').files[0].name")
            .unwrap(),
        "upload.txt",
        "cancel changed the previous file selection"
    );

    click(s, "file");
    pump_until(s, "download consent", |s| {
        s.info.download.as_ref().is_some_and(|d| d.pending)
    });
    let id = s.shared.lock().unwrap().info.download.as_ref().unwrap().id;
    assert_eq!(
        std::fs::read_dir(&files)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with("fixture.txt"))
            .count(),
        0
    );
    s.action(Action::Download { id, allow: false }).unwrap();
    click(s, "file");
    pump_until(s, "download consent", |s| {
        s.info.download.as_ref().is_some_and(|d| d.pending)
    });
    let id = s.shared.lock().unwrap().info.download.as_ref().unwrap().id;
    s.action(Action::Download { id, allow: true }).unwrap();
    pump_until(s, "download completed", |s| {
        s.info
            .download
            .as_ref()
            .is_some_and(|d| !d.pending && !d.active)
    });
    let d = s.shared.lock().unwrap().info.download.clone().unwrap();
    assert!(d.status.starts_with("Saved"), "{d:?}");
    assert_eq!(
        std::fs::read(files.join(d.name)).unwrap(),
        b"actual HTTP attachment payload"
    );

    let first = s.shared.lock().unwrap().info.tabs[0].id.clone();
    s.action(Action::NewTab).unwrap();
    assert_eq!(s.shared.lock().unwrap().info.tabs.len(), 2);
    assert!(s.action(Action::NewTab).is_err());
    let second = s
        .shared
        .lock()
        .unwrap()
        .info
        .tabs
        .iter()
        .find(|t| t.active)
        .unwrap()
        .id
        .clone();
    s.action(Action::Navigate(f.base.clone())).unwrap();
    wait(s, "document.title==='Native capability fixture'");
    let targets = s.call("Target.getTargets", json!({})).unwrap();
    assert_eq!(
        targets["targetInfos"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["type"] == "page")
            .count(),
        2
    );
    let before = raw_evaluate(s, &first, "window.ticks");
    let deadline = Instant::now() + Duration::from_millis(350);
    while Instant::now() < deadline {
        s.pump(Duration::from_millis(10)).unwrap();
    }
    assert_eq!(
        before,
        raw_evaluate(s, &first, "window.ticks"),
        "background JS was not frozen"
    );
    s.action(Action::SwitchTab(first.clone())).unwrap();
    assert_eq!(
        s.evaluate("navigator.permissions.query({name:'geolocation'}).then(p=>p.state)")
            .unwrap(),
        "denied",
        "tab switching did not revoke grant"
    );
    let ticks = s.evaluate("window.ticks").unwrap().as_u64().unwrap();
    wait(s, &format!("window.ticks>{ticks}"));
    click(s, "audio");
    wait(s, "window.audio?.state==='running'");
    let audio_time = s.evaluate("audio.currentTime").unwrap().as_f64().unwrap();
    s.action(Action::Visible(false)).unwrap();
    let ticks = s.evaluate("window.ticks").unwrap().as_u64().unwrap();
    wait(s, &format!("window.ticks>{ticks}"));
    wait(
        s,
        &format!(
            "audio.state==='running' && audio.currentTime>{}",
            audio_time + 0.05
        ),
    );
    s.action(Action::Visible(true)).unwrap();
    s.action(Action::CloseTab(second)).unwrap();
    assert_eq!(s.shared.lock().unwrap().info.tabs.len(), 1);
    assert!(s.action(Action::CloseTab(first)).is_err());
    drop(f.session);
    std::fs::remove_dir_all(f.profile).unwrap();
}
fn raw_evaluate(s: &mut Session, target: &str, expression: &str) -> Value {
    let result = s
        .call_tab(
            target,
            "Runtime.evaluate",
            json!({"expression":expression,"returnByValue":true}),
        )
        .unwrap();
    assert!(result["exceptionDetails"].is_null(), "{result}");
    result["result"]["value"].clone()
}

#[test]
#[ignore = "requires local Chromium"]
fn downloads_reject_oversize_bound_unknown_length_and_cancel() {
    let mut f = Fixture::new();
    click(&mut f.session, "huge");
    pump_until(&mut f.session, "oversize rejection", |s| {
        s.info.notice.is_some()
    });
    assert!(f.session.shared.lock().unwrap().info.download.is_none());
    f.reload();
    click(&mut f.session, "unknown");
    pump_until(&mut f.session, "unknown-size consent", |s| {
        s.info.download.as_ref().is_some_and(|d| d.pending)
    });
    let id = f
        .session
        .shared
        .lock()
        .unwrap()
        .info
        .download
        .as_ref()
        .unwrap()
        .id;
    f.session
        .action(Action::Download { id, allow: true })
        .unwrap();
    pump_until(&mut f.session, "hard size limit", |s| {
        s.info
            .download
            .as_ref()
            .is_some_and(|d| !d.pending && !d.active)
    });
    assert!(
        f.session
            .shared
            .lock()
            .unwrap()
            .info
            .download
            .as_ref()
            .unwrap()
            .status
            .contains("exceeded")
    );
    assert!(
        !std::fs::read_dir(f.profile.join("files"))
            .unwrap()
            .any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".part"))
    );
    f.reload();
    click(&mut f.session, "slowfile");
    pump_until(&mut f.session, "slow consent", |s| {
        s.info.download.as_ref().is_some_and(|d| d.pending)
    });
    let id = f
        .session
        .shared
        .lock()
        .unwrap()
        .info
        .download
        .as_ref()
        .unwrap()
        .id;
    f.session
        .action(Action::Download { id, allow: true })
        .unwrap();
    let start = Instant::now();
    f.session
        .action(Action::Download { id, allow: false })
        .unwrap();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(
        f.session
            .shared
            .lock()
            .unwrap()
            .info
            .download
            .as_ref()
            .unwrap()
            .status
            .contains("canceled")
    );
    drop(f.session);
    std::fs::remove_dir_all(f.profile).unwrap();
}

#[test]
#[ignore = "requires local Chromium"]
fn cookies_local_storage_and_indexed_db_survive_separate_chromium_processes() {
    let Fixture {
        base,
        profile,
        mut session,
    } = Fixture::new();
    let first_process = session.call("Browser.getVersion", json!({})).unwrap();
    println!("Persistence writer: {}", first_process["product"]);
    assert_eq!(session.evaluate("document.cookie='persistent_fixture=kept; Max-Age=3600; SameSite=Lax; Path=/';localStorage.setItem('persistent_fixture','local bytes');true").unwrap(),true);
    assert_eq!(session.evaluate(r#"new Promise((resolve,reject)=>{
      const request=indexedDB.open('browser-persistence-fixture',1);
      request.onupgradeneeded=()=>request.result.createObjectStore('values');
      request.onerror=()=>reject(request.error);
      request.onsuccess=()=>{const db=request.result;const tx=db.transaction('values','readwrite');tx.objectStore('values').put('database bytes','fixture');tx.oncomplete=()=>{db.close();resolve(true)};tx.onerror=()=>reject(tx.error)};
    })"#).unwrap(),true);
    // This destroys the first Chromium process and waits for its profile flush.
    let closing = Instant::now();
    drop(session);
    assert!(closing.elapsed() < Duration::from_secs(6));
    let shared = Arc::new(Mutex::new(Snapshot::default()));
    let mut second = Session::start(&discover(None).unwrap(), &profile, shared).unwrap();
    second.action(Action::Navigate(base)).unwrap();
    wait(&mut second, "document.title==='Native capability fixture'");
    assert_eq!(
        second
            .evaluate("document.cookie.includes('persistent_fixture=kept')")
            .unwrap(),
        true
    );
    assert_eq!(
        second
            .evaluate("localStorage.getItem('persistent_fixture')")
            .unwrap(),
        "local bytes"
    );
    assert_eq!(second.evaluate(r#"new Promise((resolve,reject)=>{
      const request=indexedDB.open('browser-persistence-fixture',1);
      request.onerror=()=>reject(request.error);
      request.onsuccess=()=>{const db=request.result;const tx=db.transaction('values','readonly');const get=tx.objectStore('values').get('fixture');get.onsuccess=()=>resolve(get.result);get.onerror=()=>reject(get.error);tx.oncomplete=()=>db.close()};
    })"#).unwrap(),"database bytes");
    drop(second);
    std::fs::remove_dir_all(profile).unwrap();
}
