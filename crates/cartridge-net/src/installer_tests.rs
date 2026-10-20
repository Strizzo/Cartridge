use super::*;
use crate::registry::AppPackage;
use std::net::TcpListener;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const ID: &str = "dev.cartridge.test";
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    installer: AppInstaller,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cartridge-installer-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self {
            installer: AppInstaller {
                http: HttpClient::new(root.join("cache")),
                install_dir: root.join("apps"),
            },
            root,
        }
    }

    fn put(&self, version: &str) {
        put_app(&self.installer.install_dir.join(ID), version);
    }

    fn install(&self, bytes: Vec<u8>, version: &str) -> Result<(), String> {
        let mut app = app(&bytes, version);
        let (url, server) = serve(200, vec![], bytes, false);
        app.package.as_mut().unwrap().url = url;
        let result = self.installer.install(&app);
        server.join().unwrap();
        result
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn manifest(version: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "id": ID, "version": version, "name": "Test", "author": "Tests",
        "entry": "main.lua", "permissions": ["storage", "network"]
    }))
    .unwrap()
}

fn put_app(dir: &Path, version: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("cartridge.json"), manifest(version)).unwrap();
    fs::write(dir.join("main.lua"), format!("-- {version}")).unwrap();
}

fn app(bytes: &[u8], version: &str) -> RegistryApp {
    RegistryApp {
        id: ID.into(),
        name: "Test".into(),
        description: String::new(),
        version: version.into(),
        author: "Tests".into(),
        category: "tools".into(),
        tags: vec![],
        repo_url: "https://example.invalid/this-must-not-be-fetched".into(),
        permissions: vec!["network".into(), "storage".into()],
        package: Some(AppPackage {
            url: "https://example.invalid/package.tar.gz".into(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            size: bytes.len() as u64,
            min_runtime: env!("CARGO_PKG_VERSION").into(),
        }),
    }
}

// Build raw USTAR headers so traversal and special entry tests do not depend
// on tar::Builder (which itself sanitizes malicious paths).
fn raw_tar(entries: &[(&str, u8, Vec<u8>, Option<u64>)]) -> Vec<u8> {
    let mut data = Vec::new();
    for (path, kind, contents, declared_size) in entries {
        let mut header = tar::Header::new_ustar();
        header.set_mode(0o644);
        header.set_size(declared_size.unwrap_or(contents.len() as u64));
        header.set_entry_type(tar::EntryType::new(*kind));
        header.as_mut_bytes()[..100].fill(0);
        header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
        if *kind == b'1' || *kind == b'2' {
            header.as_mut_bytes()[157..164].copy_from_slice(b"outside");
        }
        header.set_cksum();
        data.extend_from_slice(header.as_bytes());
        data.extend_from_slice(contents);
        data.resize(data.len().div_ceil(512) * 512, 0);
    }
    data.resize(data.len() + 1024, 0);
    data
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn archive(version: &str) -> Vec<u8> {
    gzip(&raw_tar(&[
        ("./", b'5', vec![], None),
        ("./cartridge.json", b'0', manifest(version), None),
        (
            "./main.lua",
            b'0',
            format!("-- {version}").into_bytes(),
            None,
        ),
        ("assets/icon.txt", b'0', b"asset".to_vec(), None),
    ]))
}

fn serve(
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
    chunked: bool,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/artifact.tar.gz", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "Installer never connected");
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("accept: {e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        while !request.windows(4).any(|part| part == b"\r\n\r\n") {
            let n = stream.read(&mut buffer).unwrap();
            assert!(n > 0 && request.len() + n <= 8192, "Invalid test request");
            request.extend_from_slice(&buffer[..n]);
        }
        let mut response = format!("HTTP/1.1 {status} Test\r\nConnection: close\r\n");
        if chunked {
            response.push_str("Transfer-Encoding: chunked\r\n");
        } else if !headers.iter().any(|(k, _)| *k == "Content-Length") {
            response.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        for (k, v) in headers {
            response.push_str(&format!("{k}: {v}\r\n"));
        }
        response.push_str("\r\n");
        let _ = stream.write_all(response.as_bytes());
        if chunked {
            let _ = write!(stream, "{:x}\r\n", body.len());
            let _ = stream.write_all(&body);
            let _ = stream.write_all(b"\r\n0\r\n\r\n");
        } else {
            let _ = stream.write_all(&body);
        }
    });
    (url, handle)
}

#[test]
fn verified_install_reinstall_update_rollback_and_remove_preserve_data() {
    let f = Fixture::new();
    fs::create_dir_all(f.root.join(ID).join("data")).unwrap();
    fs::write(
        f.root.join(ID).join("data").join("settings.json"),
        "app data",
    )
    .unwrap();
    fs::write(f.root.join("settings.json"), "global settings").unwrap();
    f.install(archive("1.0.0"), "1.0.0").unwrap();
    assert_eq!(f.installer.list_installed(), vec![ID]);
    assert!(!f.installer.can_rollback(ID));
    assert!(f.installer.has_update(&app(&archive("1.1.0"), "1.1.0")));
    assert!(!f.installer.has_update(&app(&archive("0.9.0"), "0.9.0")));
    f.install(archive("1.0.0"), "1.0.0").unwrap();
    assert!(f.installer.can_rollback(ID));
    f.install(archive("1.1.0"), "1.1.0").unwrap();
    assert_eq!(f.installer.installed_version(ID).as_deref(), Some("1.1.0"));
    assert!(!f.installer.has_update(&app(&archive("1.1.0"), "1.1.0")));
    f.installer.clone().rollback(ID).unwrap();
    assert_eq!(f.installer.installed_version(ID).as_deref(), Some("1.0.0"));
    f.installer.rollback(ID).unwrap();
    assert_eq!(f.installer.installed_version(ID).as_deref(), Some("1.1.0"));
    f.installer.remove(ID).unwrap();
    assert!(!f.installer.is_installed(ID));
    assert!(!f.installer.can_rollback(ID));
    assert_eq!(
        fs::read_to_string(f.root.join(ID).join("data").join("settings.json")).unwrap(),
        "app data"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("settings.json")).unwrap(),
        "global settings"
    );
}

#[test]
fn tampered_digest_and_wrong_exact_size_leave_current_and_backup_unchanged() {
    let f = Fixture::new();
    f.put("1.0.0");
    put_app(&f.installer.work_path(ID).join("rollback"), "0.9.0");
    for mode in 0..3 {
        let bytes = archive("2.0.0");
        let mut record = app(&bytes, "2.0.0");
        let p = record.package.as_mut().unwrap();
        match mode {
            0 => p.sha256 = "00".repeat(32),
            1 => p.size += 1,
            _ => p.size -= 1,
        }
        let (url, server) = serve(200, vec![], bytes, false);
        p.url = url;
        assert!(f.installer.install(&record).is_err());
        server.join().unwrap();
        assert_eq!(f.installer.installed_version(ID).as_deref(), Some("1.0.0"));
        assert_eq!(
            read_manifest(&f.installer.work_path(ID).join("rollback"), ID)
                .unwrap()
                .version,
            "0.9.0"
        );
        assert!(!f.installer.work_path(ID).join("stage").exists());
    }
}

#[test]
fn unsafe_paths_links_devices_duplicates_and_bombs_are_rejected() {
    let f = Fixture::new();
    f.put("1.0.0");
    let mut cases = vec![];
    for path in [
        "../escape",
        "/tmp/escape",
        "foo/../../escape",
        "C:/escape",
        "foo\\escape",
    ] {
        cases.push(vec![(path, b'0', b"bad".to_vec(), None)]);
    }
    for kind in [b'1', b'2', b'3', b'4', b'6', b'S', b'x', b'g', b'L', b'K'] {
        cases.push(vec![("bad", kind, vec![], None)]);
    }
    cases.push(vec![
        ("same", b'0', vec![], None),
        ("./same", b'0', vec![], None),
    ]);
    cases.push(vec![
        ("Same", b'0', vec![], None),
        ("same", b'0', vec![], None),
    ]);
    cases.push(vec![("large", b'0', vec![], Some(MAX_EXTRACTED_SIZE + 1))]);
    cases.push(vec![("dir", b'5', b"content".to_vec(), None)]);
    cases.push(vec![("file", b'0', b"short".to_vec(), Some(4096))]);
    for entries in cases {
        let result = f.install(gzip(&raw_tar(&entries)), "2.0.0");
        assert!(result.is_err(), "accepted {:?}", entries);
        assert_eq!(f.installer.installed_version(ID).as_deref(), Some("1.0.0"));
    }
    assert!(!f.root.join("escape").exists());
}

#[test]
fn extracted_entry_count_and_gzip_trailer_are_bounded_and_verified() {
    let f = Fixture::new();
    let names: Vec<_> = (0..=MAX_ENTRIES).map(|n| format!("file{n}")).collect();
    let entries: Vec<_> = names
        .iter()
        .map(|n| (n.as_str(), b'5', vec![], None))
        .collect();
    assert!(
        f.install(gzip(&raw_tar(&entries)), "1.0.0")
            .unwrap_err()
            .contains("too many")
    );
    let mut corrupt = archive("1.0.0");
    let length = corrupt.len();
    corrupt[length - 8] ^= 0x80;
    assert!(f.install(corrupt, "1.0.0").is_err());
    let mut hidden = raw_tar(&[("cartridge.json", b'0', manifest("1.0.0"), None)]);
    hidden.extend_from_slice(&raw_tar(&[("main.lua", b'0', b"bad".to_vec(), None)]));
    assert!(f.install(gzip(&hidden), "1.0.0").is_err());
}

#[test]
fn manifest_identity_version_permissions_entry_and_required_files_are_checked() {
    let f = Fixture::new();
    f.put("1.0.0");
    for (key, value) in [
        ("id", serde_json::json!("dev.other")),
        ("version", serde_json::json!("2.0.1")),
        (
            "permissions",
            serde_json::json!(["network", "storage", "ssh"]),
        ),
        ("permissions", serde_json::json!(["network"])),
        ("entry", serde_json::json!("../main.lua")),
    ] {
        let mut m: serde_json::Value = serde_json::from_slice(&manifest("2.0.0")).unwrap();
        m[key] = value;
        let bytes = gzip(&raw_tar(&[
            (
                "cartridge.json",
                b'0',
                serde_json::to_vec(&m).unwrap(),
                None,
            ),
            ("main.lua", b'0', b"-- code".to_vec(), None),
        ]));
        assert!(f.install(bytes, "2.0.0").is_err());
    }
    for missing in ["main.lua", "cartridge.json"] {
        let entries = [
            ("cartridge.json", b'0', manifest("2.0.0"), None),
            ("main.lua", b'0', vec![], None),
        ];
        let retained: Vec<_> = entries
            .into_iter()
            .filter(|(p, _, _, _)| *p != missing)
            .collect();
        assert!(f.install(gzip(&raw_tar(&retained)), "2.0.0").is_err());
    }
    assert_eq!(f.installer.installed_version(ID).as_deref(), Some("1.0.0"));
}

#[test]
fn metadata_runtime_and_https_are_checked_before_network_or_live_changes() {
    let f = Fixture::new();
    f.put("1.0.0");
    let valid = app(&archive("2.0.0"), "2.0.0");
    for case in 0..9 {
        let mut record = valid.clone();
        let p = record.package.as_mut().unwrap();
        match case {
            0 => p.size = MAX_PACKAGE_SIZE + 1,
            1 => p.size = 0,
            2 => p.min_runtime = "999.0.0".into(),
            3 => p.min_runtime = "invalid".into(),
            4 => p.sha256 = "not a digest".into(),
            5 => p.url = "http://example.com/package".into(),
            6 => p.url = "file:///etc/passwd".into(),
            7 => record.version = "latest".into(),
            _ => record.package = None,
        }
        assert!(f.installer.install(&record).is_err());
        assert_eq!(f.installer.installed_version(ID).as_deref(), Some("1.0.0"));
    }
    for url in [
        "https://user:pass@example.com/a",
        "https://example.com/a#fragment",
        "//example.com/a",
    ] {
        assert!(validate_artifact_url(url).is_err());
    }
}

#[test]
fn http_errors_truncation_redirect_encoding_and_chunked_overflow_are_rejected() {
    let f = Fixture::new();
    f.put("1.0.0");
    for case in 0..7 {
        let bytes = archive("2.0.0");
        let mut record = app(&bytes, "2.0.0");
        let (status, headers, body, chunked) = match case {
            0 => (404, vec![], bytes, false),
            1 => (206, vec![], bytes, false),
            2 => (
                302,
                vec![("Location", "http://example.com/must-not-fetch".into())],
                bytes,
                false,
            ),
            3 => {
                let length = bytes.len();
                let mut truncated = bytes;
                truncated.truncate(length / 2);
                (
                    200,
                    vec![("Content-Length", length.to_string())],
                    truncated,
                    false,
                )
            }
            4 => (200, vec![("Content-Encoding", "gzip".into())], bytes, false),
            5 => {
                record.package.as_mut().unwrap().size = 16;
                (200, vec![], bytes, true)
            }
            _ => (
                200,
                vec![("Content-Length", "33554433".into())],
                vec![],
                false,
            ),
        };
        let (url, server) = serve(status, headers, body, chunked);
        record.package.as_mut().unwrap().url = url;
        assert!(f.installer.install(&record).is_err());
        server.join().unwrap();
        assert_eq!(f.installer.installed_version(ID).as_deref(), Some("1.0.0"));
    }
}

#[test]
fn every_interrupted_install_and_rollback_swap_recovers() {
    for operation in ["install", "rollback"] {
        for step in 0..=3 {
            let f = Fixture::new();
            f.put("1.0.0");
            let work = f.installer.work_path(ID);
            let live = f.installer.install_dir.join(ID);
            put_app(&work.join("rollback"), "0.9.0");
            if operation == "install" {
                put_app(&work.join("stage"), "2.0.0");
            }
            let pending = work.join(if operation == "install" {
                "previous"
            } else {
                "rollback-swap"
            });
            if step >= 1 {
                fs::rename(&live, &pending).unwrap();
            }
            if step >= 2 {
                fs::rename(
                    work.join(if operation == "install" {
                        "stage"
                    } else {
                        "rollback"
                    }),
                    &live,
                )
                .unwrap();
            }
            if step >= 3 {
                remove_directory(&work.join("rollback")).unwrap();
                fs::rename(&pending, work.join("rollback")).unwrap();
            }
            // list_installed must also discover/restore an app with no live directory.
            assert_eq!(f.installer.list_installed(), vec![ID]);
            let expected = if step < 2 {
                "1.0.0"
            } else if operation == "install" {
                "2.0.0"
            } else {
                "0.9.0"
            };
            assert_eq!(f.installer.installed_version(ID).as_deref(), Some(expected));
            assert!(f.installer.can_rollback(ID));
            assert!(!pending.exists());
            assert!(!work.join("stage").exists());
            assert_eq!(
                read_manifest(&work.join("rollback"), ID).unwrap().version,
                if step < 2 { "0.9.0" } else { "1.0.0" }
            );
        }
    }
}

#[test]
fn invalid_ids_cannot_read_write_remove_or_rollback_outside_apps() {
    let f = Fixture::new();
    put_app(&f.root.join("outside"), "1.0.0");
    for id in [
        "",
        ".",
        "..",
        "../outside",
        "dev/../../outside",
        "/tmp/outside",
        "C:\\outside",
        ".installer",
        "dev..test",
    ] {
        let mut record = app(&archive("2.0.0"), "2.0.0");
        record.id = id.into();
        assert!(f.installer.install(&record).is_err());
        assert!(f.installer.remove(id).is_err());
        assert!(f.installer.rollback(id).is_err());
        assert!(!f.installer.can_rollback(id));
        assert!(!f.installer.is_installed(id));
        assert_eq!(f.installer.installed_version(id), None);
        assert_eq!(
            f.installer.app_path(id),
            f.installer.install_dir.join(".invalid-app-id")
        );
    }
    assert!(f.root.join("outside/main.lua").is_file());
}

#[cfg(unix)]
#[test]
fn symlinked_live_or_work_directories_cannot_escape_the_install_root() {
    use std::os::unix::fs::symlink;
    for target in ["live", "work", "backup", "stage"] {
        let f = Fixture::new();
        let outside = f.root.join("outside");
        put_app(&outside, "1.0.0");
        fs::create_dir_all(&f.installer.install_dir).unwrap();
        if target == "live" {
            symlink(&outside, f.installer.install_dir.join(ID)).unwrap();
        } else {
            let work = f.installer.work_path(ID);
            fs::create_dir_all(&work).unwrap();
            match target {
                "work" => {
                    fs::remove_dir(&work).unwrap();
                    symlink(&outside, &work).unwrap();
                }
                "backup" => symlink(&outside, work.join("rollback")).unwrap(),
                _ => symlink(&outside, work.join("stage")).unwrap(),
            }
        }
        assert!(f.installer.remove(ID).is_err());
        assert!(f.installer.rollback(ID).is_err());
        assert!(outside.join("main.lua").exists());
    }
}

#[test]
fn kernel_lock_serializes_clones_and_survives_no_stale_lock_state() {
    let f = Fixture::new();
    f.put("1.0.0");
    let clone = f.installer.clone();
    let lock = f.installer.lock_app(ID).unwrap();
    assert!(clone.remove(ID).unwrap_err().contains("busy"));
    assert_eq!(clone.installed_version(ID).as_deref(), Some("1.0.0"));
    drop(lock);
    clone.remove(ID).unwrap();
}

#[test]
fn legacy_local_apps_remain_launchable_without_remote_package() {
    let f = Fixture::new();
    f.put("0.4.0");
    let mut legacy = app(&archive("1.0.0"), "1.0.0");
    legacy.package = None;
    assert!(f.installer.is_installed(ID));
    assert_eq!(f.installer.app_path(ID), f.installer.install_dir.join(ID));
    assert!(!f.installer.has_update(&legacy));
    assert!(f.installer.install(&legacy).is_err());
}

#[test]
fn updates_use_installed_override_then_bundled_version_and_semver_precedence() {
    let f = Fixture::new();
    let bundled = f.root.join("bundled");
    put_app(&bundled.join("test"), "1.2.0");
    let mut record = app(&archive("1.2.1"), "1.2.1");
    assert!(f.installer.has_update_in(&record, &bundled));
    record.version = "1.2.0+new.build".into();
    assert!(!f.installer.has_update_in(&record, &bundled));
    record.version = "1.2.0-rc.1".into();
    assert!(!f.installer.has_update_in(&record, &bundled));
    f.put("1.3.0");
    record.version = "1.2.1".into();
    assert!(!f.installer.has_update_in(&record, &bundled));
    record.version = "1.3.1".into();
    assert!(f.installer.has_update_in(&record, &bundled));
}

#[test]
fn optional_manifest_runtime_must_be_compatible_and_match_signed_metadata() {
    let f = Fixture::new();
    f.put("1.0.0");
    for minimum in ["999.0.0", "not-semver", "0.1.0", env!("CARGO_PKG_VERSION")] {
        let mut m: serde_json::Value = serde_json::from_slice(&manifest("2.0.0")).unwrap();
        m["min_runtime"] = minimum.into();
        let bytes = gzip(&raw_tar(&[
            (
                "cartridge.json",
                b'0',
                serde_json::to_vec(&m).unwrap(),
                None,
            ),
            ("main.lua", b'0', b"-- code".to_vec(), None),
        ]));
        let result = f.install(bytes, "2.0.0");
        if minimum == env!("CARGO_PKG_VERSION") {
            result.unwrap();
            assert_eq!(f.installer.installed_version(ID).as_deref(), Some("2.0.0"));
        } else {
            assert!(result.is_err());
            assert_eq!(f.installer.installed_version(ID).as_deref(), Some("1.0.0"));
        }
    }
}
