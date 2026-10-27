//! Host-owned file policy. Web content never receives a filesystem API.
use super::*;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

pub const MAX_TABS: usize = 2;
pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_FILES_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_FILES: usize = 128;

#[derive(Clone, Debug)]
pub struct JavascriptDialog {
    pub id: u64,
    pub origin: String,
    pub kind: String,
    pub message: String,
    pub default_text: String,
}
#[derive(Clone, Debug)]
pub struct TabInfo {
    pub id: String,
    pub title: String,
    pub url: String,
    pub active: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Location,
    Notifications,
    Camera,
    Microphone,
}
impl Permission {
    pub fn name(self) -> &'static str {
        match self {
            Self::Location => "geolocation",
            Self::Notifications => "notifications",
            Self::Camera => "videoCapture",
            Self::Microphone => "audioCapture",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Location => "Location",
            Self::Notifications => "Notifications",
            Self::Camera => "Camera",
            Self::Microphone => "Microphone",
        }
    }
    fn descriptor(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Microphone => "microphone",
            _ => self.name(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct PermissionPrompt {
    pub id: u64,
    pub origin: String,
    pub permission: Permission,
}
#[derive(Clone, Debug)]
pub struct FileChoice {
    pub name: String,
    pub bytes: u64,
}
#[derive(Clone, Debug)]
pub struct UploadPrompt {
    pub id: u64,
    pub origin: String,
    pub files: Vec<FileChoice>,
    pub multiple: bool,
}
#[derive(Clone, Debug)]
pub struct DownloadInfo {
    pub id: u64,
    pub origin: String,
    pub name: String,
    pub received: u64,
    pub total: Option<u64>,
    pub status: String,
    pub pending: bool,
    pub active: bool,
}
pub(super) struct Upload {
    pub(super) id: u64,
    pub(super) node: u64,
    pub(super) frame: String,
    since: Instant,
}
pub(super) struct Download {
    info: DownloadInfo,
    request: String,
    stream: Option<String>,
    pending: Option<u64>,
    file: Option<File>,
    temporary: PathBuf,
    destination: PathBuf,
    since: Instant,
    last_data: Instant,
}
pub(super) struct Tab {
    pub(super) target: String,
    pub(super) session: String,
    pub(super) frame: String,
    info: PageInfo,
    zoom: f64,
}

// A flat, private user folder deliberately excludes symlinks, devices, hidden
// files, subdirectories and oversized files. Bound work even with a hostile dir.
fn list_files(root: &Path) -> Result<Vec<FileChoice>, String> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)
        .map_err(|e| e.to_string())?
        .take(MAX_FILES * 4)
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !safe_name(&name) {
            continue;
        }
        let meta = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        if meta.is_file() && meta.nlink() == 1 && meta.len() <= MAX_FILE_BYTES {
            files.push(FileChoice {
                name,
                bytes: meta.len(),
            });
            if files.len() == MAX_FILES {
                break;
            }
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name.len() <= 180
        && !name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
}
fn download_name(headers: &[Value], url: &str) -> String {
    let disposition = header(headers, "content-disposition");
    let filename = disposition
        .split(';')
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            key.eq_ignore_ascii_case("filename")
                .then(|| value.trim_matches('"').to_string())
        })
        .or_else(|| {
            url::Url::parse(url)
                .ok()?
                .path_segments()?
                .next_back()
                .map(str::to_owned)
        });
    filename
        .filter(|n| safe_name(n))
        .unwrap_or_else(|| "download.bin".into())
}
fn publish_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let old = CString::new(temporary.as_os_str().as_bytes())?;
    let new = CString::new(destination.as_os_str().as_bytes())?;
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            old.as_ptr(),
            libc::AT_FDCWD,
            new.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let result = unsafe { libc::renamex_np(old.as_ptr(), new.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
fn header<'a>(headers: &'a [Value], name: &str) -> &'a str {
    headers
        .iter()
        .find(|h| {
            h["name"]
                .as_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .and_then(|h| h["value"].as_str())
        .unwrap_or("")
}
fn origin(url: &str) -> String {
    crate::model::site_key(url).unwrap_or_else(|| "Opaque / local origin (blocked)".into())
}
#[cfg(any(target_os = "linux", test))]
fn available_kib(text: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        if parts.next() != Some("MemAvailable:") {
            return None;
        }
        let value = parts.next()?.parse().ok()?;
        (parts.next() == Some("kB")).then_some(value)
    })
}
#[cfg(any(target_os = "linux", test))]
fn under_memory_pressure(kib: u64, low_samples: &mut u8) -> bool {
    *low_samples = if kib < 220_000 {
        low_samples.saturating_add(1)
    } else {
        0
    };
    kib < 190_000 || *low_samples >= 2
}
impl Session {
    pub(super) fn check_memory(&mut self) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        if self.memory_check.elapsed() >= Duration::from_millis(500) {
            self.memory_check = Instant::now();
            // Read only the kernel's available-memory estimate, which accounts
            // for reclaimable cache. Never signal another application or the OS.
            let available = File::open("/proc/meminfo").ok().and_then(|f| {
                let mut text = String::new();
                f.take(16384).read_to_string(&mut text).ok()?;
                available_kib(&text)
            });
            if let Some(kib) = available {
                if under_memory_pressure(kib, &mut self.memory_low_samples) {
                    self.memory_pressure = true;
                    return Err(format!(
                        "Browser closed to protect available memory ({kib} KiB free). Exit Browser and free memory before reopening. Your saved files and bookmarks are kept."
                    ));
                }
            }
        }
        Ok(())
    }
    pub(super) fn init_native(&mut self) -> Result<(), String> {
        fs::create_dir_all(&self.files_root).map_err(|e| e.to_string())?;
        if fs::symlink_metadata(&self.files_root)
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Browser Files folder must not be a symlink.".into());
        }
        fs::set_permissions(&self.files_root, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        // An empty grant list explicitly denies every permission, including ones
        // not exposed in the native menu. Individual approvals override one only.
        self.block_permissions()?;
        self.configure_native_page()?;
        self.tabs.push(Tab {
            target: self.target.clone(),
            session: self.session.clone().unwrap(),
            frame: self.main_frame.clone(),
            info: self.shared.lock().unwrap().info.clone(),
            zoom: self.zoom,
        });
        self.publish_tabs();
        Ok(())
    }
    pub(super) fn block_permissions(&mut self) -> Result<(), String> {
        self.call("Browser.resetPermissions", json!({}))?;
        self.call("Browser.grantPermissions", json!({"permissions":[]}))?;
        Ok(())
    }
    fn configure_native_page(&mut self) -> Result<(), String> {
        self.call(
            "Page.setInterceptFileChooserDialog",
            json!({"enabled":true}),
        )?;
        // Chromium itself never writes a download. Only a confirmed HTTP response
        // stream can be written by this host, with a hard decoded byte limit.
        self.call("Fetch.enable", json!({"patterns":[{"urlPattern":"*","requestStage":"Response","resourceType":"Document"}]}))?;
        Ok(())
    }
    pub(super) fn publish_tabs(&mut self) {
        let mut s = self.shared.lock().unwrap();
        s.info.tabs = self
            .tabs
            .iter()
            .map(|t| {
                let info = if t.target == self.target {
                    &s.info
                } else {
                    &t.info
                };
                TabInfo {
                    id: t.target.clone(),
                    title: info.title.clone(),
                    url: info.url.clone(),
                    active: t.target == self.target,
                }
            })
            .collect();
        s.revision += 1;
    }
    pub(super) fn clear_native_prompts(&mut self) -> Result<(), String> {
        let dialog = self.shared.lock().unwrap().info.javascript_dialog.is_some();
        if dialog {
            self.call("Page.handleJavaScriptDialog", json!({"accept":false}))?;
        }
        self.upload = None;
        self.finish_download("Canceled", false)?;
        self.block_permissions()?;
        let mut s = self.shared.lock().unwrap();
        s.info.permission = None;
        s.info.upload = None;
        s.info.javascript_dialog = None;
        s.info.dialog = None;
        s.revision += 1;
        Ok(())
    }
    pub(super) fn switch_tab(&mut self, id: &str) -> Result<(), String> {
        if id == self.target {
            return Ok(());
        }
        let next = self
            .tabs
            .iter()
            .position(|t| t.target == id)
            .ok_or("That tab was closed.")?;
        self.clear_native_prompts()?;
        self.call("Page.stopScreencast", json!({}))?;
        self.call("Page.setWebLifecycleState", json!({"state":"frozen"}))?;
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.target == self.target) {
            tab.info = self.shared.lock().unwrap().info.clone();
            tab.frame = self.main_frame.clone();
            tab.zoom = self.zoom;
        }
        self.target = self.tabs[next].target.clone();
        self.session = Some(self.tabs[next].session.clone());
        self.main_frame = self.tabs[next].frame.clone();
        self.zoom = self.tabs[next].zoom;
        self.pending_info = None;
        self.pending_navigation = None;
        self.info_generation = self.info_generation.wrapping_add(1);
        self.focus_serial = 0;
        self.mouse_down = false;
        self.frame_acks.clear();
        {
            let mut s = self.shared.lock().unwrap();
            s.info = self.tabs[next].info.clone();
            s.info.focus_serial = 0;
            s.frame = None;
            s.revision += 1;
        }
        self.call("Page.setWebLifecycleState", json!({"state":"active"}))?;
        self.call("Page.bringToFront", json!({}))?;
        self.action(Action::Zoom(self.zoom))?;
        if self.visible {
            self.action(Action::Visible(true))?;
        }
        self.info_dirty = true;
        self.publish_tabs();
        Ok(())
    }
    pub(super) fn new_tab(&mut self) -> Result<(), String> {
        if self.tabs.len() >= MAX_TABS {
            return Err("Two tabs maximum. Close a tab first.".into());
        }
        self.creating_tab = true;
        let result = self.call("Target.createTarget", json!({"url":"about:blank"}));
        let target = (match result {
            Ok(value) => value,
            Err(e) => {
                self.creating_tab = false;
                return Err(e);
            }
        })["targetId"]
            .as_str()
            .ok_or("No new tab target")?
            .to_string();
        let attached = self.call(
            "Target.attachToTarget",
            json!({"targetId":target,"flatten":true}),
        );
        let session = match attached.and_then(|v| {
            v["sessionId"]
                .as_str()
                .map(str::to_owned)
                .ok_or("No new tab session".into())
        }) {
            Ok(session) => session,
            Err(error) => {
                self.creating_tab = false;
                self.closing_popups.insert(target.clone());
                let _ = self.send("Target.closeTarget", json!({"targetId":target}));
                return Err(error);
            }
        };
        self.tabs.push(Tab {
            target: target.clone(),
            session,
            frame: String::new(),
            info: PageInfo {
                engine_version: self.shared.lock().unwrap().info.engine_version.clone(),
                ..Default::default()
            },
            zoom: 1.0,
        });
        self.creating_tab = false;
        self.switch_tab(&target)?;
        self.call("Page.enable", json!({}))?;
        self.call("Runtime.enable", json!({}))?;
        let tree = self.call("Page.getFrameTree", json!({}))?;
        self.main_frame = tree["frameTree"]["frame"]["id"]
            .as_str()
            .unwrap_or("")
            .into();
        self.configure_native_page()?;
        Ok(())
    }
    pub(super) fn close_tab(&mut self, id: &str) -> Result<(), String> {
        if self.tabs.len() == 1 {
            return Err("Keep one tab open. Use Select to exit.".into());
        }
        if !self.tabs.iter().any(|t| t.target == id) {
            return Err("That tab was closed.".into());
        }
        if id == self.target {
            let next = self
                .tabs
                .iter()
                .find(|t| t.target != id)
                .unwrap()
                .target
                .clone();
            self.switch_tab(&next)?;
        }
        self.clear_upload_staging(id);
        self.tabs.retain(|t| t.target != id);
        self.closing_popups.insert(id.into());
        self.call("Target.closeTarget", json!({"targetId":id}))?;
        self.publish_tabs();
        Ok(())
    }
    pub(super) fn request_permission(&mut self, permission: Permission) -> Result<(), String> {
        let url = self.shared.lock().unwrap().info.url.clone();
        let origin = crate::model::site_key(&url).ok_or("Open a web page first.")?;
        self.native_serial += 1;
        let mut s = self.shared.lock().unwrap();
        s.info.permission = Some(PermissionPrompt {
            id: self.native_serial,
            origin,
            permission,
        });
        s.revision += 1;
        Ok(())
    }
    pub(super) fn permission_decision(&mut self, id: u64, allow: bool) -> Result<(), String> {
        let prompt = self.shared.lock().unwrap().info.permission.clone();
        if let Some(p) = prompt.filter(|p| p.id == id) {
            if crate::model::site_key(&self.shared.lock().unwrap().info.url).as_deref()
                != Some(&p.origin)
            {
                return Err("Page changed; permission refused.".into());
            }
            self.call("Browser.setPermission", json!({"permission":{"name":p.permission.descriptor()},"setting":if allow {"granted"} else {"denied"},"origin":p.origin,"embeddedOrigin":p.origin}))?;
            let mut s = self.shared.lock().unwrap();
            s.info.permission = None;
            s.info.notice = Some(format!(
                "{} {} for this visit. Retry the website's request.",
                p.permission.label(),
                if allow { "allowed" } else { "blocked" }
            ));
            s.revision += 1;
        }
        Ok(())
    }
    pub(super) fn upload_decision(&mut self, id: u64, names: Vec<String>) -> Result<(), String> {
        let Some(upload) = self.upload.as_ref().filter(|u| u.id == id) else {
            return Ok(());
        };
        let node = upload.node;
        // Interception suppresses the OS picker without pausing JS. Cancel
        // leaves the input's previous FileList untouched.
        if names.is_empty() {
            self.upload = None;
            let mut s = self.shared.lock().unwrap();
            s.info.upload = None;
            s.revision += 1;
            return Ok(());
        }
        let offered = self
            .shared
            .lock()
            .unwrap()
            .info
            .upload
            .clone()
            .ok_or("File chooser closed")?;
        if names.len() > if offered.multiple { 8 } else { 1 } {
            return Err("Select at most eight files.".into());
        }
        if names
            .iter()
            .any(|name| !safe_name(name) || !offered.files.iter().any(|f| &f.name == name))
        {
            return Err("Choose a listed file from Browser Files.".into());
        }
        let unique = names.iter().collect::<HashSet<_>>();
        if unique.len() != names.len() {
            return Err("Select each file once.".into());
        }
        let mut total = 0;
        if !names.is_empty() && self.upload_staging.len() >= 2 {
            return Err("Upload staging full. Navigate to release previous uploads.".into());
        }
        let staging = self
            .files_root
            .join(format!(".upload-{}-{id}", std::process::id()));
        if !names.is_empty() {
            fs::create_dir(&staging).map_err(|e| e.to_string())?;
            fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            self.upload_staging
                .push((self.target.clone(), staging.clone()));
        }
        let selection = (|| -> Result<Vec<String>, String> {
            let mut paths = Vec::new();
            for name in &names {
                if !safe_name(name) || !offered.files.iter().any(|f| &f.name == name) {
                    return Err("Choose a listed file from Browser Files.".into());
                }
                let path = self.files_root.join(name);
                let file = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                let meta = file.metadata().map_err(|e| e.to_string())?;
                if !meta.is_file() || meta.nlink() != 1 || meta.len() > MAX_FILE_BYTES {
                    return Err("File is no longer a regular file within the size limit.".into());
                }
                total += meta.len();
                if total > MAX_FILE_BYTES {
                    return Err("Upload total exceeds 32 MiB.".into());
                }
                // Copy from the opened O_NOFOLLOW descriptor. A later local rename or
                // symlink swap cannot change which bytes Chromium is allowed to read.
                let mut source = file.take(MAX_FILE_BYTES + 1);
                let snapshot = staging.join(name);
                let mut destination = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&snapshot)
                    .map_err(|e| e.to_string())?;
                let copied =
                    std::io::copy(&mut source, &mut destination).map_err(|e| e.to_string())?;
                if copied != meta.len() || copied > MAX_FILE_BYTES {
                    return Err("File changed while selecting; retry upload.".into());
                }
                paths.push(self.profile_in_engine.join("files")
                    .join(staging.file_name().unwrap()).join(name)
                    .to_string_lossy().into_owned());
            }
            Ok(paths)
        })();
        let paths = match selection {
            Ok(paths) => paths,
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                self.upload_staging.retain(|(_, path)| path != &staging);
                return Err(error);
            }
        };
        self.call(
            "DOM.setFileInputFiles",
            json!({"backendNodeId":node,"files":paths}),
        )?;
        self.upload = None;
        let mut s = self.shared.lock().unwrap();
        s.info.upload = None;
        s.revision += 1;
        Ok(())
    }
    pub(super) fn clear_upload_staging(&mut self, target: &str) {
        self.upload_staging.retain(|(tab, path)| {
            if tab == target {
                let _ = fs::remove_dir_all(path);
                false
            } else {
                true
            }
        });
    }
    pub(super) fn native_event(&mut self, value: &Value) -> Result<bool, String> {
        let method = value["method"].as_str().unwrap_or("");
        let p = &value["params"];
        if method.is_empty()
            && value["id"]
                .as_u64()
                .is_some_and(|id| self.canceled_streams.remove(&id))
        {
            if let Some(stream) = value["result"]["stream"].as_str() {
                self.send("IO.close", json!({"handle":stream}))?;
            }
            return Ok(true);
        }
        if method == "Fetch.requestPaused" {
            let request = p["requestId"].as_str().unwrap_or("").to_string();
            let headers = p["responseHeaders"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let mime = header(headers, "content-type")
                .split(';')
                .next()
                .unwrap_or("")
                .trim();
            let attachment = header(headers, "content-disposition")
                .to_ascii_lowercase()
                .starts_with("attachment")
                || matches!(
                    mime,
                    "application/octet-stream"
                        | "application/zip"
                        | "application/pdf"
                        | "application/x-tar"
                        | "application/gzip"
                );
            let code = p["responseStatusCode"].as_u64().unwrap_or(0);
            if !attachment || !(200..300).contains(&code) {
                self.send("Fetch.continueRequest", json!({"requestId":request}))?;
                return Ok(true);
            }
            let url = p["request"]["url"].as_str().unwrap_or("");
            let total = header(headers, "content-length").parse::<u64>().ok();
            if self.download.is_some()
                || total.is_some_and(|n| n > MAX_FILE_BYTES)
                || crate::model::site_key(url).is_none()
            {
                self.send(
                    "Fetch.failRequest",
                    json!({"requestId":request,"errorReason":"Aborted"}),
                )?;
                self.shared.lock().unwrap().info.loading = false;
                self.notice("Download blocked: busy, unsupported address or over 32 MiB.");
                return Ok(true);
            }
            self.native_serial += 1;
            let info = DownloadInfo {
                id: self.native_serial,
                origin: origin(url),
                name: download_name(headers, url),
                received: 0,
                total,
                status: "Save to Browser Files? (32 MiB maximum)".into(),
                pending: true,
                active: false,
            };
            self.download = Some(Download {
                info: info.clone(),
                request,
                stream: None,
                pending: None,
                file: None,
                temporary: PathBuf::new(),
                destination: PathBuf::new(),
                since: Instant::now(),
                last_data: Instant::now(),
            });
            let mut s = self.shared.lock().unwrap();
            s.info.download = Some(info);
            s.info.loading = false;
            s.revision += 1;
            return Ok(true);
        }
        if method == "Page.fileChooserOpened" {
            if self.upload.is_some() {
                return Ok(true);
            }
            let Some(node) = p["backendNodeId"].as_u64() else {
                self.notice("This file picker is unsupported.");
                return Ok(true);
            };
            let frame = p["frameId"].as_str().unwrap_or("").to_string();
            // Frame origins come from CDP frame navigation, never page-supplied text.
            let Some(origin) = self.frame_origins.get(&frame).cloned() else {
                self.send(
                    "DOM.setFileInputFiles",
                    json!({"backendNodeId":node,"files":[]}),
                )?;
                return Ok(true);
            };
            self.native_serial += 1;
            let files = match list_files(&self.files_root) {
                Ok(files) => files,
                Err(error) => {
                    self.send(
                        "DOM.setFileInputFiles",
                        json!({"backendNodeId":node,"files":[]}),
                    )?;
                    self.notice(&format!("Cannot list Browser Files: {error}"));
                    return Ok(true);
                }
            };
            let prompt = UploadPrompt {
                id: self.native_serial,
                origin,
                files,
                multiple: p["mode"] == "selectMultiple",
            };
            self.upload = Some(Upload {
                id: self.native_serial,
                node,
                frame,
                since: Instant::now(),
            });
            let mut s = self.shared.lock().unwrap();
            s.info.upload = Some(prompt);
            s.revision += 1;
            return Ok(true);
        }
        if value.get("method").is_none()
            && self
                .download
                .as_ref()
                .is_some_and(|d| d.pending == value["id"].as_u64() && d.pending.is_some())
        {
            if value.get("error").is_some() {
                self.finish_download("Download failed", false)?;
                return Ok(true);
            }
            let d = self.download.as_mut().unwrap();
            d.pending = None;
            if d.stream.is_none() {
                d.stream = value["result"]["stream"].as_str().map(str::to_owned);
                if d.stream.is_none() {
                    self.finish_download("Download stream unavailable", false)?;
                }
            } else {
                let result = &value["result"];
                let data = result["data"].as_str().unwrap_or("");
                let bytes = if result["base64Encoded"] == true {
                    base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|e| e.to_string())?
                } else {
                    data.as_bytes().to_vec()
                };
                if bytes.len() > 32768 || d.info.received + bytes.len() as u64 > MAX_FILE_BYTES {
                    self.finish_download("Download exceeded 32 MiB; partial file removed", false)?;
                } else {
                    if let Err(e) = d.file.as_mut().unwrap().write_all(&bytes) {
                        self.finish_download(&format!("Cannot save download: {e}"), false)?;
                        return Ok(true);
                    }
                    d.info.received += bytes.len() as u64;
                    if !bytes.is_empty() {
                        d.last_data = Instant::now();
                    }
                    if result["eof"] == true {
                        self.finish_download("Saved to Browser Files", true)?;
                    } else {
                        let mut s = self.shared.lock().unwrap();
                        s.info.download = Some(d.info.clone());
                        s.revision += 1;
                    }
                }
            }
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn notice(&self, text: &str) {
        let mut s = self.shared.lock().unwrap();
        s.info.notice = Some(text.into());
        s.revision += 1;
    }
    pub(super) fn download_decision(&mut self, id: u64, allow: bool) -> Result<(), String> {
        if !self.download.as_ref().is_some_and(|d| d.info.id == id) {
            return Ok(());
        }
        if !allow {
            return self.finish_download("Download canceled", false);
        }
        if !self.download.as_ref().unwrap().info.pending {
            return Ok(());
        }
        let mut used = 0u64;
        let mut count = 0;
        for entry in fs::read_dir(&self.files_root)
            .map_err(|e| e.to_string())?
            .take(MAX_FILES * 4 + 1)
        {
            count += 1;
            used = used.saturating_add(
                entry
                    .map_err(|e| e.to_string())?
                    .metadata()
                    .map_err(|e| e.to_string())?
                    .len(),
            );
        }
        // Reserve the maximum transfer size, including for unknown Content-Length.
        if count >= MAX_FILES || used.saturating_add(MAX_FILE_BYTES) > MAX_FILES_BYTES {
            return self.finish_download("Files folder full (64 MiB / 128 entries)", false);
        }
        let d = self.download.as_mut().unwrap();
        // Unique destination, create_new and exclusive rename prevent path traversal and
        // overwrite even if a local program changes the folder during a transfer.
        d.info.name = format!("{}-{}", d.info.id, d.info.name);
        d.destination = self.files_root.join(&d.info.name);
        d.temporary = self.files_root.join(format!(
            ".download-{}-{}.part",
            std::process::id(),
            d.info.id
        ));
        d.file = Some(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&d.temporary)
                .map_err(|e| e.to_string())?,
        );
        d.info.pending = false;
        d.info.active = true;
        d.since = Instant::now();
        d.last_data = Instant::now();
        d.info.status = "Downloading; Tools > Downloads > X cancels".into();
        let request = d.request.clone();
        let pending = self.send(
            "Fetch.takeResponseBodyAsStream",
            json!({"requestId":request}),
        )?;
        self.download.as_mut().unwrap().pending = Some(pending);
        let mut s = self.shared.lock().unwrap();
        s.info.download = Some(self.download.as_ref().unwrap().info.clone());
        s.revision += 1;
        Ok(())
    }
    fn finish_download(&mut self, status: &str, success: bool) -> Result<(), String> {
        let Some(mut d) = self.download.take() else {
            return Ok(());
        };
        if let Some(stream) = d.stream {
            let _ = self.send("IO.close", json!({"handle":stream}));
        } else if let Some(id) = d.pending {
            // Cancellation can beat the stream-open reply. Close that handle
            // when it arrives rather than leaking it across repeated transfers.
            if self.canceled_streams.len() < 8 {
                self.canceled_streams.insert(id);
            }
        }
        let _ = self.send(
            "Fetch.failRequest",
            json!({"requestId":d.request,"errorReason":"Aborted"}),
        );
        let mut status = status.to_string();
        drop(d.file.take());
        if success {
            if let Err(e) = publish_file(&d.temporary, &d.destination) {
                status = format!("Could not publish download: {e}");
            }
        }
        if !d.temporary.as_os_str().is_empty() {
            let _ = fs::remove_file(&d.temporary);
        }
        d.info.status = status;
        d.info.pending = false;
        d.info.active = false;
        let mut s = self.shared.lock().unwrap();
        s.info.notice = Some(format!("{}: {}", d.info.name, d.info.status));
        s.info.download = Some(d.info);
        s.info.loading = false;
        s.revision += 1;
        Ok(())
    }
    pub(super) fn pump_native(&mut self) -> Result<(), String> {
        if self
            .upload
            .as_ref()
            .is_some_and(|u| u.since.elapsed() > Duration::from_secs(120))
        {
            let id = self.upload.as_ref().unwrap().id;
            self.upload_decision(id, vec![])?;
        }
        if self.download.as_ref().is_some_and(|d| {
            d.since.elapsed() > Duration::from_secs(120)
                || (!d.info.pending && d.last_data.elapsed() > Duration::from_secs(20))
        }) {
            self.finish_download("Download timed out; partial file removed", false)?;
        }
        if let Some(stream) = self
            .download
            .as_ref()
            .filter(|d| d.pending.is_none())
            .and_then(|d| d.stream.clone())
        {
            let id = self.send("IO.read", json!({"handle":stream,"size":32768}))?;
            self.download.as_mut().unwrap().pending = Some(id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_guard_uses_available_memory_and_recovers_from_brief_dips() {
        assert_eq!(
            available_kib("MemTotal: 919240 kB\nMemFree: 10000 kB\nMemAvailable: 250000 kB\n"),
            Some(250000)
        );
        assert_eq!(available_kib("MemFree: 1 kB"), None);
        assert_eq!(available_kib("MemAvailable: 2 MB"), None);
        let mut samples = 0;
        assert!(!under_memory_pressure(219999, &mut samples));
        assert!(!under_memory_pressure(240000, &mut samples));
        assert!(!under_memory_pressure(200000, &mut samples));
        assert!(under_memory_pressure(200000, &mut samples));
        assert!(!under_memory_pressure(240000, &mut samples));
        assert!(under_memory_pressure(189999, &mut samples));
    }
    #[test]
    fn exclusive_publish_does_not_overwrite_existing_files() {
        let root = std::env::temp_dir().join(format!("browser-publish-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source");
        let destination = root.join("destination");
        fs::write(&source, "new").unwrap();
        fs::write(&destination, "keep").unwrap();
        assert!(publish_file(&source, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"keep");
        fs::remove_file(&destination).unwrap();
        publish_file(&source, &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"new");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn download_names_never_escape_files_folder() {
        for name in ["../secret", "/secret", ".hidden", "a\\b", "a\nb"] {
            assert!(!safe_name(name));
            assert_eq!(
                download_name(
                    &[
                        json!({"name":"Content-Disposition","value":format!("attachment; filename=\"{name}\"")})
                    ],
                    "http://localhost/"
                ),
                "download.bin"
            );
        }
        assert_eq!(
            download_name(&[], "https://example.org/report.txt"),
            "report.txt"
        );
    }
}
