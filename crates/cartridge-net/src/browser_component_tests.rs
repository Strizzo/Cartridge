use super::*;
use ed25519_dalek::{Signer, SigningKey};
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};

fn fix_checksum(raw: &mut [u8]) {
    let mut h = tar::Header::new_ustar();
    h.as_mut_bytes().copy_from_slice(&raw[..512]);
    h.set_cksum();
    raw[..512].copy_from_slice(h.as_bytes());
}
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    key: SigningKey,
    release: ComponentRelease,
    files: BTreeMap<String, Vec<u8>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        FAIL_AT.with(|f| f.set(None));
        let _ = remove_tree(&self.root);
    }
}
impl Fixture {
    fn new() -> Self {
        // /tmp is tmpfs on some Linux runners; exercise the real persistent-
        // filesystem policy on the checkout volume instead of weakening it.
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/browser-component-fixtures");
        fs::create_dir_all(&base).unwrap();
        let root = base.join(format!(
            "browser-component-test-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::SeqCst)
        ));
        ensure_directory(&root).unwrap();
        let mut seed = [0; 32];
        File::open("/dev/urandom")
            .unwrap()
            .read_exact(&mut seed)
            .unwrap();
        let key = SigningKey::from_bytes(&seed);
        let mut files = BTreeMap::<String, Vec<u8>>::new();
        for (name, data) in [
            ("chromium", "verified wrapper"),
            ("bubblewrap", "verified helper"),
            ("component.json", "{}\n"),
            ("packages.tsv", "chromium\t1.2.3\n"),
            ("notices/bubblewrap.txt", "license"),
            ("notices/static-libc.txt", "libc license"),
            ("notices/static-libcap.txt", "libcap license"),
            ("notices/sources.txt", "pinned source metadata only"),
            ("rootfs/usr/lib/chromium/chromium", "engine v1"),
            (
                "rootfs/usr/share/doc/chromium/copyright",
                "Debian copyright",
            ),
        ] {
            files.insert(name.into(), data.as_bytes().to_vec());
        }
        let mut entries = BTreeMap::new();
        for (path, data) in &files {
            let mut parent = Path::new(path).parent();
            while let Some(p) = parent.filter(|p| !p.as_os_str().is_empty()) {
                let name = p.to_str().unwrap().to_string();
                entries.insert(name.clone(), ComponentEntry::Directory { path: name });
                parent = p.parent();
            }
            let mode = if ["chromium", "bubblewrap", "rootfs/usr/lib/chromium/chromium"]
                .contains(&path.as_str())
            {
                0o555
            } else {
                0o444
            };
            entries.insert(
                path.clone(),
                ComponentEntry::File {
                    path: path.clone(),
                    size: data.len() as u64,
                    sha256: digest(data),
                    mode,
                },
            );
        }
        entries.insert(
            "rootfs/proc".into(),
            ComponentEntry::Directory {
                path: "rootfs/proc".into(),
            },
        );
        entries.insert(
            "rootfs/link".into(),
            ComponentEntry::Symlink {
                path: "rootfs/link".into(),
                target: "usr/lib/chromium/chromium".into(),
            },
        );
        entries.insert(
            "rootfs/usr/lib/chromium/copy".into(),
            ComponentEntry::Hardlink {
                path: "rootfs/usr/lib/chromium/copy".into(),
                target: "rootfs/usr/lib/chromium/chromium".into(),
            },
        );
        let reference = |p: &str| ComponentFileReference {
            path: p.into(),
            size: files[p].len() as u64,
            sha256: digest(&files[p]),
        };
        let stored = files.values().map(|v| v.len() as u64).sum::<u64>();
        let release = ComponentRelease {
            schema: 1,
            component: "browser-engine".into(),
            target: TARGET.into(),
            version: "1.0.0".into(),
            revision: "a".repeat(40),
            min_runtime: "0.8.0".into(),
            chromium_version: "1.2.3-1".into(),
            source_distribution: "metadata-only; corresponding-source distribution not asserted"
                .into(),
            provenance: ComponentProvenance {
                package_list: reference("packages.tsv"),
                build_inventory: reference("component.json"),
                sources: Vec::new(),
            },
            archive: ComponentArchive {
                url: "https://example.org/engine.tar.gz".into(),
                size: 1,
                sha256: "0".repeat(64),
                stored_bytes: stored,
                logical_bytes: stored + files["rootfs/usr/lib/chromium/chromium"].len() as u64,
            },
            entries: entries.into_values().collect(),
        };
        Self {
            root,
            key,
            release,
            files,
        }
    }
    fn envelope(&self, release: &ComponentRelease) -> String {
        self.sign_payload(serde_json::to_string(release).unwrap())
    }
    fn sign_payload(&self, payload: String) -> String {
        let signature = self
            .key
            .sign(payload.as_bytes())
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        serde_json::to_string(&Envelope {
            key_id: KEY_ID.into(),
            payload,
            signature,
        })
        .unwrap()
    }
    fn store(&self) -> BrowserComponents {
        let home = self.root.join("home");
        ensure_directory(&home).unwrap();
        BrowserComponents::open_home(&home, self.key.verifying_key().to_bytes()).unwrap()
    }
    fn tar(&self, release: &ComponentRelease) -> Vec<u8> {
        let mut tar = tar::Builder::new(Vec::new());
        for entry in &release.entries {
            let mut h = tar::Header::new_ustar();
            h.set_path(entry.path()).unwrap();
            h.set_uid(0);
            h.set_gid(0);
            h.set_mtime(0);
            h.set_entry_type(tar::EntryType::new(entry.kind()));
            let file = match entry {
                ComponentEntry::Hardlink { target, .. } => {
                    release.entries.iter().find(|e| e.path() == target).unwrap()
                }
                _ => entry,
            };
            h.set_mode(file.mode());
            if let Some(target) = entry.target() {
                h.set_link_name(target).unwrap();
            }
            let data = if let ComponentEntry::File { path, .. } = entry {
                self.files[path].as_slice()
            } else {
                &[]
            };
            h.set_size(data.len() as u64);
            h.set_cksum();
            tar.append(&h, data).unwrap();
        }
        tar.into_inner().unwrap()
    }
    fn artifact(&self, release: &mut ComponentRelease, raw: Vec<u8>) -> Vec<u8> {
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gzip.write_all(&raw).unwrap();
        let bytes = gzip.finish().unwrap();
        release.archive.size = bytes.len() as u64;
        release.archive.sha256 = digest(&bytes);
        bytes
    }
    fn build(&self, version: &str) -> (VerifiedComponent, Vec<u8>) {
        let mut r = self.release.clone();
        r.version = version.into();
        let raw = self.tar(&r);
        let bytes = self.artifact(&mut r, raw);
        (
            verify_with_key(&self.envelope(&r), &self.key.verifying_key().to_bytes()).unwrap(),
            bytes,
        )
    }
    fn source(&self, release: &VerifiedComponent) -> PathBuf {
        let root = self.root.join("prepared");
        ensure_directory(&root).unwrap();
        for entry in &release.release.entries {
            let path = root.join(entry.path());
            match entry {
                ComponentEntry::Directory { .. } => ensure_directory(&path).unwrap(),
                ComponentEntry::File {
                    path: name, mode, ..
                } => {
                    fs::write(&path, &self.files[name]).unwrap();
                    fs::set_permissions(&path, fs::Permissions::from_mode(*mode | 0o200)).unwrap();
                }
                ComponentEntry::Symlink { target, .. } => symlink(target, path).unwrap(),
                ComponentEntry::Hardlink { target, .. } => {
                    fs::hard_link(root.join(target), path).unwrap()
                }
            }
        }
        root
    }
    fn signed_path(&self, c: &VerifiedComponent) -> PathBuf {
        let path = self.root.join(format!("{}.json", c.id()));
        fs::write(&path, c.signed_manifest()).unwrap();
        path
    }
}

#[test]
fn signatures_identity_and_provenance_are_required() {
    let f = Fixture::new();
    let (c, _) = f.build("1.0.0");
    assert!(verify_manifest(c.signed_manifest()).is_err()); // Test signer is never production trust.
    let mut envelope: Envelope = serde_json::from_str(c.signed_manifest()).unwrap();
    envelope.payload.push(' ');
    assert!(
        verify_with_key(
            &serde_json::to_string(&envelope).unwrap(),
            &f.key.verifying_key().to_bytes()
        )
        .is_err()
    );
    envelope.key_id = "attacker".into();
    assert!(
        verify_with_key(
            &serde_json::to_string(&envelope).unwrap(),
            &f.key.verifying_key().to_bytes()
        )
        .is_err()
    );
    let mut r = c.release.clone();
    r.provenance.package_list.sha256 = "0".repeat(64);
    assert!(validate_release(&r).is_err());
    r = c.release.clone();
    r.provenance.sources.push(ComponentSourceArtifact {
        role: "index".into(),
        url: "https://example.org/source-index.json".into(),
        size: 123,
        sha256: "a".repeat(64),
    });
    validate_release(&r).unwrap();
    r.provenance.sources[0].url = "http://example.org/source".into();
    assert!(validate_release(&r).is_err());
    for key in ["schema", "version"] {
        let original = serde_json::to_string(&c.release).unwrap();
        let payload = format!("{{\"{key}\":null,{}", &original[1..]);
        assert!(
            verify_with_key(&f.sign_payload(payload), &f.key.verifying_key().to_bytes()).is_err()
        );
    }
    let original = serde_json::to_string(&c.release).unwrap();
    let payload = original.replacen("\"path\":", "\"path\":\"attacker\",\"path\":", 1);
    assert!(verify_with_key(&f.sign_payload(payload), &f.key.verifying_key().to_bytes()).is_err());
}
#[test]
fn manifest_rejects_path_link_parent_and_special_entry_attacks() {
    let f = Fixture::new();
    for path in [
        "/absolute",
        "../escape",
        "rootfs/../escape",
        "rootfs//escape",
        "rootfs/./escape",
        "rootfs\\escape",
        ".browser-release.json",
    ] {
        let mut r = f.release.clone();
        r.entries
            .push(ComponentEntry::Directory { path: path.into() });
        r.entries.sort_by(|a, b| a.path().cmp(b.path()));
        assert!(validate_release(&r).is_err(), "{path}");
    }
    for target in ["/etc/passwd", "../../escape", "link", "usr/../../../escape"] {
        let mut r = f.release.clone();
        *r.entries
            .iter_mut()
            .find(|e| e.path() == "rootfs/link")
            .unwrap() = ComponentEntry::Symlink {
            path: "rootfs/link".into(),
            target: target.into(),
        };
        assert!(validate_release(&r).is_err(), "{target}");
    }
    let mut r = f.release.clone();
    r.entries.push(ComponentEntry::File {
        path: "rootfs/link/child".into(),
        size: 0,
        sha256: digest(b""),
        mode: 0o444,
    });
    r.entries.sort_by(|a, b| a.path().cmp(b.path()));
    assert!(validate_release(&r).is_err());
    for target in [
        "/etc/passwd",
        "../escape",
        "rootfs/link",
        "rootfs/usr/lib/chromium/copy",
    ] {
        let mut r = f.release.clone();
        *r.entries
            .iter_mut()
            .find(|e| e.path().ends_with("/copy"))
            .unwrap() = ComponentEntry::Hardlink {
            path: "rootfs/usr/lib/chromium/copy".into(),
            target: target.into(),
        };
        assert!(validate_release(&r).is_err());
    }
    let mut r = f.release.clone();
    r.entries.push(r.entries[0].clone());
    assert!(validate_release(&r).is_err());
}
#[test]
fn archive_tampering_special_files_extensions_duplicates_and_trailing_data_fail() {
    let f = Fixture::new();
    let (c, bytes) = f.build("1.0.0");
    unpack(Cursor::new(&bytes), None, &c, &HashMap::new()).unwrap();
    let mut bad = bytes.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(unpack(Cursor::new(bad), None, &c, &HashMap::new()).is_err());
    for kind in [b'1', b'2', b'3', b'4', b'6', b'x', b'g', b'L', b'K'] {
        let mut raw = f.tar(&c.release);
        raw[156] = kind;
        fix_checksum(&mut raw);
        let mut r = c.release.clone();
        let bytes = f.artifact(&mut r, raw);
        let signed = verify_with_key(&f.envelope(&r), &f.key.verifying_key().to_bytes()).unwrap();
        assert!(
            unpack(Cursor::new(bytes), None, &signed, &HashMap::new()).is_err(),
            "{kind}"
        );
    }
    for name in ["../escape", "/absolute", "rootfs//evil"] {
        let mut raw = f.tar(&c.release);
        raw[..100].fill(0);
        raw[..name.len()].copy_from_slice(name.as_bytes());
        fix_checksum(&mut raw);
        let mut r = c.release.clone();
        let bytes = f.artifact(&mut r, raw);
        let signed = verify_with_key(&f.envelope(&r), &f.key.verifying_key().to_bytes()).unwrap();
        assert!(unpack(Cursor::new(bytes), None, &signed, &HashMap::new()).is_err());
    }
    let mut raw = f.tar(&c.release);
    raw[512] ^= 1;
    let mut r = c.release.clone();
    let bytes = f.artifact(&mut r, raw);
    let signed = verify_with_key(&f.envelope(&r), &f.key.verifying_key().to_bytes()).unwrap();
    assert!(
        unpack(Cursor::new(bytes), None, &signed, &HashMap::new())
            .unwrap_err()
            .contains("hash")
    );
    let raw = f.tar(&c.release);
    for raw in [
        [raw[..1024].to_vec(), raw.clone()].concat(),
        [raw.clone(), vec![b'x']].concat(),
        raw[..raw.len() - 512].to_vec(),
    ] {
        let mut r = c.release.clone();
        let bytes = f.artifact(&mut r, raw);
        let signed = verify_with_key(&f.envelope(&r), &f.key.verifying_key().to_bytes()).unwrap();
        assert!(unpack(Cursor::new(bytes), None, &signed, &HashMap::new()).is_err());
    }
}
#[test]
fn install_reuses_immutable_files_preserves_hardlinks_and_rolls_back() {
    let f = Fixture::new();
    let store = f.store();
    let (a, bytes) = f.build("1.0.0");
    store
        .install_reader(&a, Cursor::new(bytes), |_| {})
        .unwrap();
    let a_path = store.root.join("releases").join(a.id());
    let native = a_path.join("rootfs/usr/lib/chromium/chromium");
    assert_eq!(
        fs::metadata(&native).unwrap().ino(),
        fs::metadata(a_path.join("rootfs/usr/lib/chromium/copy"))
            .unwrap()
            .ino()
    );
    assert_eq!(fs::metadata(&native).unwrap().mode() & 0o7777, 0o555);
    let (b, bytes) = f.build("1.1.0");
    store
        .install_reader(&b, Cursor::new(bytes), |_| {})
        .unwrap();
    assert_eq!(
        fs::metadata(&native).unwrap().ino(),
        fs::metadata(store.root.join("current/rootfs/usr/lib/chromium/chromium"))
            .unwrap()
            .ino()
    );
    assert_eq!(store.status().unwrap().previous.as_deref(), Some(a.id()));
    assert_eq!(
        fs::read(store.root.join("chromium")).unwrap(),
        f.files["chromium"]
    );
    assert_eq!(store.rollback().unwrap().id(), a.id());
    assert_eq!(store.status().unwrap().previous.as_deref(), Some(b.id()));
    assert_eq!(store.rollback().unwrap().id(), b.id());
    assert_eq!(store.verify_current().unwrap().id(), b.id());
}
#[test]
fn adopt_moves_without_copy_and_retry_recovers_an_interrupted_move() {
    let f = Fixture::new();
    let store = f.store();
    let (c, _) = f.build("1.0.0");
    let source = f.source(&c);
    let inode = fs::metadata(source.join("chromium")).unwrap().ino();
    let manifest = f.signed_path(&c);
    FAIL_AT.with(|f| f.set(Some("adopt-renamed")));
    assert!(store.adopt(&manifest, &source).is_err());
    assert!(!source.exists());
    assert!(store.status().unwrap().current.is_none());
    FAIL_AT.with(|f| f.set(None));
    store.adopt(&manifest, &source).unwrap();
    assert_eq!(
        fs::metadata(store.root.join("chromium")).unwrap().ino(),
        inode
    );
    store.verify_current().unwrap();
}
#[test]
fn adopt_rejects_extras_hash_changes_and_external_hardlinks_without_moving() {
    for attack in ["extra", "hash", "external", "symlink", "fifo"] {
        let f = Fixture::new();
        let store = f.store();
        let (c, _) = f.build("1.0.0");
        let source = f.source(&c);
        let manifest = f.signed_path(&c);
        match attack {
            "extra" => fs::write(source.join("unexpected"), "no").unwrap(),
            "hash" => fs::write(source.join("chromium"), "bad").unwrap(),
            "external" => fs::hard_link(source.join("chromium"), f.root.join("external")).unwrap(),
            "symlink" => {
                fs::remove_file(source.join("chromium")).unwrap();
                symlink("/etc/passwd", source.join("chromium")).unwrap();
            }
            "fifo" => {
                use std::os::unix::ffi::OsStrExt;
                fs::remove_file(source.join("chromium")).unwrap();
                let name =
                    std::ffi::CString::new(source.join("chromium").as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            _ => unreachable!(),
        }
        assert!(store.adopt(&manifest, &source).is_err(), "{attack}");
        assert!(source.exists());
        assert!(store.status().unwrap().current.is_none());
    }
}
#[test]
fn interrupted_activation_always_exposes_a_complete_current_previous_pair() {
    for phase in [
        "release-renamed",
        "generation-ready",
        "before-state-switch",
        "after-state-switch",
    ] {
        let f = Fixture::new();
        let store = f.store();
        let (a, bytes) = f.build("1.0.0");
        store
            .install_reader(&a, Cursor::new(bytes), |_| {})
            .unwrap();
        let (b, bytes) = f.build("1.1.0");
        FAIL_AT.with(|f| f.set(Some(phase)));
        assert!(
            store
                .install_reader(&b, Cursor::new(&bytes), |_| {})
                .is_err()
        );
        let state = store.status().unwrap();
        if phase == "after-state-switch" {
            assert_eq!(state.current.as_deref(), Some(b.id()));
            assert_eq!(state.previous.as_deref(), Some(a.id()));
        } else {
            assert_eq!(state.current.as_deref(), Some(a.id()));
            assert!(state.previous.is_none());
        }
        FAIL_AT.with(|f| f.set(None));
        store
            .install_reader(&b, Cursor::new(bytes), |_| {})
            .unwrap();
        assert_eq!(store.verify_current().unwrap().id(), b.id());
        assert_eq!(store.status().unwrap().previous.as_deref(), Some(a.id()));
    }
}
#[test]
fn bad_update_and_bad_rollback_cannot_change_activation() {
    let f = Fixture::new();
    let store = f.store();
    let (a, bytes) = f.build("1.0.0");
    store
        .install_reader(&a, Cursor::new(bytes), |_| {})
        .unwrap();
    let (b, bytes) = f.build("1.1.0");
    let mut bad = bytes.clone();
    bad[0] ^= 1;
    assert!(store.install_reader(&b, Cursor::new(bad), |_| {}).is_err());
    assert_eq!(store.status().unwrap().current.as_deref(), Some(a.id()));
    store
        .install_reader(&b, Cursor::new(bytes), |_| {})
        .unwrap();
    let path = store.root.join("releases").join(a.id()).join("chromium");
    // Shared immutable files are protected from accidental writes. Deliberate
    // owner tampering is detected on verify/rollback; it is outside user isolation.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(&path, "bad").unwrap();
    assert!(store.rollback().is_err());
    assert_eq!(store.status().unwrap().current.as_deref(), Some(b.id()));
}
#[test]
fn private_store_scope_and_lock_are_enforced() {
    let f = Fixture::new();
    let store = f.store();
    let _guard = store.lock().unwrap();
    assert!(store.status().is_err());
    let home = f.root.join("unsafe");
    ensure_directory(&home).unwrap();
    symlink(&store.root, home.join(".cartridges")).unwrap();
    assert!(BrowserComponents::open_home(&home, f.key.verifying_key().to_bytes()).is_err());
    for home in ["/", "/usr", "/boot", "/etc", "/dev"] {
        assert!(
            BrowserComponents::open_home(Path::new(home), f.key.verifying_key().to_bytes())
                .is_err()
        );
    }
    for url in [
        "http://example.org/a",
        "file:///etc/passwd",
        "https://a@b/c",
        "https://a/#f",
        "https://a\\b/c",
    ] {
        assert!(https_url(url).is_err());
    }
}
#[test]
fn independent_limits_support_the_real_component_entry_count() {
    let f = Fixture::new();
    let mut r = f.release.clone();
    for n in 0..18000 {
        r.entries.push(ComponentEntry::File {
            path: format!("rootfs/file-{n:05}"),
            size: 0,
            sha256: digest(b""),
            mode: 0o444,
        });
    }
    r.entries.sort_by(|a, b| a.path().cmp(b.path()));
    validate_release(&r).unwrap();
    assert!(serde_json::to_vec(&r).unwrap().len() < MAX_MANIFEST as usize);
    r.archive.stored_bytes += 1;
    assert!(validate_release(&r).is_err());
}

#[test]
fn python_packager_and_rust_verifier_interoperate_with_generated_test_key() {
    use std::process::Command;
    let f = Fixture::new();
    let openssl = if Path::new("/opt/homebrew/opt/openssl@3/bin/openssl").exists() {
        "/opt/homebrew/opt/openssl@3/bin/openssl"
    } else {
        "openssl"
    };
    let version = Command::new(openssl)
        .arg("version")
        .output()
        .expect("OpenSSL 3 required for package interoperability test");
    assert!(version.stdout.starts_with(b"OpenSSL 3."));
    let der = f.root.join("test-private.der");
    let mut bytes = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    bytes.extend(f.key.to_bytes());
    fs::write(&der, bytes).unwrap();
    let private = f.root.join("test-private.pem");
    let public = f.root.join("test-public.pem");
    for args in [
        vec![
            "pkey",
            "-inform",
            "DER",
            "-in",
            der.to_str().unwrap(),
            "-out",
            private.to_str().unwrap(),
        ],
        vec![
            "pkey",
            "-in",
            private.to_str().unwrap(),
            "-pubout",
            "-out",
            public.to_str().unwrap(),
        ],
    ] {
        assert!(
            Command::new(openssl)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let result = Command::new("python3")
        .arg("-c")
        .arg(
            r#"
import sys
from pathlib import Path
sys.path.insert(0, str(Path(sys.argv[1]) / 'tests'))
from test_browser_engine_package import make_component, package
root = Path(sys.argv[2])
make_component(root / 'python-prepared')
package.build(root / 'python-prepared', root / 'python-output', '1.0.0', 'a'*40,
              'https://example.org/engine.tar.gz', Path(sys.argv[3]), Path(sys.argv[4]))
"#,
        )
        .args([repo, &f.root, &private, &public])
        .env("OPENSSL", openssl)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let store = f.store();
    let output = f.root.join("python-output");
    let manifest = output.join(MANIFEST_NAME);
    let component = store
        .inspect(&read_text(&manifest, MAX_MANIFEST).unwrap())
        .unwrap();
    verify_archive(
        &component,
        &output.join("browser-engine-r36s-plus-1.0.0.tar.gz"),
    )
    .unwrap();
    component
        .verify_directory(&f.root.join("python-prepared"))
        .unwrap();
    store
        .adopt(&manifest, &f.root.join("python-prepared"))
        .unwrap();
    store.verify_current().unwrap();
}

#[test]
fn install_rejects_downgrade_same_version_replacement_and_future_runtime() {
    let f = Fixture::new();
    let store = f.store();
    let (current, bytes) = f.build("2.0.0");
    store
        .install_reader(&current, Cursor::new(bytes), |_| {})
        .unwrap();
    let (old, bytes) = f.build("1.0.0");
    assert!(
        store
            .install_reader(&old, Cursor::new(bytes), |_| {})
            .unwrap_err()
            .contains("newer")
    );
    let mut r = current.release.clone();
    r.revision = "b".repeat(40);
    let replacement = store.inspect(&f.envelope(&r)).unwrap();
    assert!(
        store
            .install_reader(&replacement, Cursor::new(Vec::new()), |_| {})
            .is_err()
    );
    r.version = "3.0.0".into();
    r.min_runtime = "99.0.0".into();
    let future = store.inspect(&f.envelope(&r)).unwrap();
    assert!(
        store
            .install_reader(&future, Cursor::new(Vec::new()), |_| {})
            .unwrap_err()
            .contains("runtime")
    );
    assert_eq!(
        store.status().unwrap().current.as_deref(),
        Some(current.id())
    );
}
#[test]
fn indirect_symlink_escape_and_archive_link_target_substitution_fail() {
    let f = Fixture::new();
    let mut r = f.release.clone();
    *r.entries
        .iter_mut()
        .find(|e| e.path() == "rootfs/link")
        .unwrap() = ComponentEntry::Symlink {
        path: "rootfs/link".into(),
        target: ".".into(),
    };
    r.entries.push(ComponentEntry::Symlink {
        path: "rootfs/escape".into(),
        target: "link/../..".into(),
    });
    r.entries.sort_by(|a, b| a.path().cmp(b.path()));
    assert!(validate_release(&r).is_err());
    let (component, _) = f.build("1.0.0");
    for kind in [b'1', b'2'] {
        let mut raw = f.tar(&component.release);
        let mut offset = 0;
        for entry in &component.release.entries {
            let size = if let ComponentEntry::File { size, .. } = entry {
                *size as usize
            } else {
                0
            };
            if entry.kind() == kind {
                raw[offset + 157..offset + 257].fill(0);
                raw[offset + 157..offset + 166].copy_from_slice(b"../escape");
                fix_checksum(&mut raw[offset..]);
                break;
            }
            offset += 512 + size.div_ceil(512) * 512;
        }
        let mut r = component.release.clone();
        let bytes = f.artifact(&mut r, raw);
        let signed = verify_with_key(&f.envelope(&r), &f.key.verifying_key().to_bytes()).unwrap();
        let store = f.store();
        assert!(
            store
                .install_reader(&signed, Cursor::new(bytes), |_| {})
                .is_err()
        );
        assert!(store.status().unwrap().current.is_none());
        assert!(!store.root.join(".incoming").exists());
        assert!(!f.root.join("escape").exists());
    }
}
