//! Real inherited pipes with a deterministic protocol peer; no Chromium or network.
use cartridge_browser::engine::{Session, Shared};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Peer(PathBuf);
impl Peer {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cartridge-pipe-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        let engine = path.join("peer.py");
        std::fs::write(&engine, r#"#!/usr/bin/env python3
import json, os, time
incoming = os.fdopen(3, 'rb', buffering=0)
while True:
    line = bytearray()
    while True:
        c = incoming.read(1)
        if not c: raise SystemExit(0)
        if c == b'\0': break
        line.extend(c)
    request = json.loads(line)
    method = request['method']
    result = {}
    if method == 'Browser.close': raise SystemExit(0)
    if method == 'Browser.getVersion': result = {'product': 'Fragmented test peer'}
    if method == 'Target.getTargets': result = {'targetInfos': [{'type': 'page', 'targetId': 'page'}]}
    if method == 'Target.attachToTarget': result = {'sessionId': 'session'}
    if method == 'Page.getFrameTree': result = {'frameTree': {'frame': {'id': 'frame'}}}
    expression = request.get('params', {}).get('expression')
    if expression == 'large': result = {'result': {'value': 'x' * (7 * 1024 * 1024)}}
    if expression == 'oversize': result = {'result': {'value': 'x' * (8 * 1024 * 1024)}}
    payload = json.dumps({'id': request['id'], 'result': result}).encode() + b'\0'
    if expression == 'event':
        payload = json.dumps({'method': 'Fixture.large', 'params': {'text': 'x' * (7 * 1024 * 1024)}}).encode() + b'\0' + payload
    try:
        if expression == 'split':
            # An unrelated event and partial reply arrive in one read; the rest
            # arrives after the pipe becomes temporarily unreadable.
            os.write(4, b'{"method":"Fixture.event","params":{}}\0' + payload[:10])
            time.sleep(0.25)
            payload = payload[10:]
        # Writes and reads have independent boundaries, just like Chromium's pipe.
        for offset in range(0, len(payload), 16384):
            remaining = memoryview(payload)[offset:offset+16384]
            while remaining:
                remaining = remaining[os.write(4, remaining):]
    except BrokenPipeError:
        raise SystemExit(0)
"#).unwrap();
        std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self(path)
    }
    fn session(&self) -> Session {
        Session::start(
            &self.0.join("peer.py"),
            &self.0.join("profile"),
            Shared::default(),
        )
        .unwrap()
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn fragmented_seven_megabyte_reply_does_not_grow_the_stack() {
    let peer = Peer::new();
    let mut session = peer.session();
    let result = session.evaluate("large").unwrap();
    assert_eq!(result.as_str().unwrap().len(), 7 * 1024 * 1024);
    // A following request proves framing and session state remain usable.
    assert_eq!(session.call("Fixture.next", json!({})).unwrap(), json!({}));
}

#[test]
fn partial_reply_survives_a_gap_and_an_unrelated_event() {
    let peer = Peer::new();
    let mut session = peer.session();
    let start = Instant::now();
    assert_eq!(
        session
            .call("Runtime.evaluate", json!({"expression":"split"}))
            .unwrap(),
        json!({})
    );
    assert!(start.elapsed() >= Duration::from_millis(200));
    assert!(start.elapsed() < Duration::from_secs(3));
}

#[test]
fn oversized_reply_still_hits_the_existing_transport_limit() {
    let peer = Peer::new();
    let mut session = peer.session();
    let error = session.evaluate("oversize").unwrap_err();
    assert!(error.contains("bounded transport limit"), "{error}");
}

#[test]
fn large_unsolicited_event_preserves_the_following_reply() {
    let peer = Peer::new();
    let mut session = peer.session();
    assert_eq!(
        session
            .call("Runtime.evaluate", json!({"expression":"event"}))
            .unwrap(),
        json!({})
    );
    assert_eq!(session.call("Fixture.next", json!({})).unwrap(), json!({}));
}
