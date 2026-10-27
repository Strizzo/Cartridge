//! Chromium DevTools transport over two inherited pipes. No listening socket,
//! browser extensions, page privileged bridge, shell invocation or sandbox bypass.
use crate::model::{VIEW_HEIGHT, VIEW_WIDTH, address};
use base64::Engine as _;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_MESSAGE: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, Default)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub error: Option<String>,
    pub editable: bool,
    pub password: bool,
    pub dialog: Option<String>,
    pub engine_version: String,
    pub http_status: Option<u16>,
}
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}
#[derive(Default)]
pub struct Snapshot {
    pub info: PageInfo,
    pub frame: Option<Frame>,
    pub revision: u64,
    pub frame_count: u64,
}
pub type Shared = Arc<Mutex<Snapshot>>;
#[derive(Clone, Debug)]
pub enum Action {
    Navigate(String),
    Back,
    Forward,
    Reload,
    Stop,
    Pointer(f64, f64),
    Button(f64, f64, bool),
    Click(f64, f64),
    Scroll(f64, f64, f64, f64),
    Key(String, bool),
    Text(String),
    ReplaceText(String),
    Zoom(f64),
    Visible(bool),
    FrameRate(u8),
    Dialog(bool),
    Quit,
}

// Preserve every click/key boundary, but do not replay stale continuous motion
// after a heavy page stalls the renderer. A batch never exceeds the channel cap.
fn queue_action(batch: &mut Vec<Action>, action: Action) {
    match (batch.last_mut(), action) {
        (Some(Action::Pointer(px, py)), Action::Pointer(x, y)) => {
            *px = x;
            *py = y;
        }
        (Some(Action::Scroll(px, py, pdx, pdy)), Action::Scroll(x, y, dx, dy)) => {
            *px = x;
            *py = y;
            *pdx = (*pdx + dx).clamp(-2048.0, 2048.0);
            *pdy = (*pdy + dy).clamp(-2048.0, 2048.0);
        }
        (_, action) => batch.push(action),
    }
}

pub fn discover(override_path: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) =
        override_path.or_else(|| std::env::var_os("CARTRIDGE_BROWSER_ENGINE").map(PathBuf::from))
    {
        return path
            .is_file()
            .then_some(path)
            .ok_or("The configured browser engine executable is missing.".into());
    }
    let mut candidates =
        vec![cartridge_core::paths::cartridges_dir().join("components/browser-engine/chromium")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("browser-engine/chromium"));
        }
    }
    candidates.extend(
        [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/google-chrome",
            "/opt/google/chrome/chrome",
        ]
        .into_iter()
        .map(PathBuf::from),
    );
    candidates.into_iter().find(|p|p.is_file()).ok_or("No local Chromium engine found. Install the browser engine bundle or set CARTRIDGE_BROWSER_ENGINE.".into())
}

fn pipe() -> std::io::Result<(File, File)> {
    let mut raw = [-1; 2];
    if unsafe { libc::pipe(raw.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // Move every pipe descriptor out of 3/4 before dup2 in the child. This also
    // avoids collisions when the parent initially had those descriptors free.
    let mut files = Vec::new();
    let originals = unsafe { [File::from_raw_fd(raw[0]), File::from_raw_fd(raw[1])] };
    for original in originals {
        let duplicate = unsafe { libc::fcntl(original.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 10) };
        if duplicate < 0 {
            return Err(std::io::Error::last_os_error());
        }
        files.push(unsafe { File::from_raw_fd(duplicate) });
    }
    let write = files.pop().unwrap();
    let read = files.pop().unwrap();
    Ok((read, write))
}

pub struct Session {
    child: Child,
    reader: File,
    writer: File,
    bytes: Vec<u8>,
    next_id: u64,
    session: Option<String>,
    target: String,
    main_frame: String,
    popups: HashMap<String, Instant>,
    closing_popups: HashSet<String>,
    pub shared: Shared,
    stopped: Arc<AtomicBool>,
    info_dirty: bool,
    mouse_down: bool,
    zoom: f64,
    frame_acks: VecDeque<u64>,
    next_ack: Instant,
    frame_interval: Duration,
    pending_navigation: Option<(u64, Instant)>,
    pending_info: Option<(u64, u64)>,
    info_generation: u64,
}
impl Session {
    pub fn start(executable: &Path, profile: &Path, shared: Shared) -> Result<Self, String> {
        Self::start_controlled(
            executable,
            profile,
            shared,
            Arc::new(AtomicBool::new(false)),
        )
    }
    fn start_controlled(
        executable: &Path,
        profile: &Path,
        shared: Shared,
        stopped: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        #[cfg(target_os = "linux")]
        if unsafe { libc::geteuid() } == 0 {
            return Err("Run the browser as the regular Cartridge user; Chromium's sandbox must remain enabled.".into());
        }
        std::fs::create_dir_all(profile).map_err(|e| e.to_string())?;
        std::fs::set_permissions(profile, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let log = profile.join("engine.log");
        if log.metadata().is_ok_and(|s| s.len() > 1024 * 1024) {
            let _ = std::fs::rename(&log, profile.join("engine.previous.log"));
        }
        let errors = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .map_err(|e| e.to_string())?;
        let (child_read, writer) = pipe().map_err(|e| e.to_string())?;
        let (reader, child_write) = pipe().map_err(|e| e.to_string())?;
        let read_fd = child_read.as_raw_fd();
        let write_fd = child_write.as_raw_fd();
        let mut command = Command::new(executable);
        // Do not pass SSH agent sockets, API keys or unrelated application
        // environment to the engine. macOS frameworks require the real HOME:
        // substituting a profile path makes Chrome's certificate/network startup
        // stall. --user-data-dir still isolates all browsing data. The Linux
        // component instead mounts only its private root and profile.
        #[cfg(target_os = "macos")]
        let engine_home =
            std::env::var_os("HOME").unwrap_or_else(|| profile.as_os_str().to_owned());
        #[cfg(not(target_os = "macos"))]
        let engine_home = profile.as_os_str();
        command
            .env_clear()
            .env("HOME", engine_home)
            .env("PATH", "/usr/bin:/bin:/opt/homebrew/bin")
            .env("LANG", "C.UTF-8");
        for name in ["TMPDIR", "TZ"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command
            .args([
                "--headless=new",
                "--remote-debugging-pipe",
                "--no-first-run",
                "--no-default-browser-check",
                "--disable-extensions",
                "--disable-sync",
                "--disable-gpu",
                "--disk-cache-size=33554432",
                "--media-cache-size=8388608",
            ])
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!("--window-size={VIEW_WIDTH},{VIEW_HEIGHT}"))
            .arg("about:blank")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(errors);
        unsafe {
            command.pre_exec(move || {
                if libc::setpgid(0, 0) != 0
                    || libc::dup2(read_fd, 3) < 0
                    || libc::dup2(write_fd, 4) < 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command
            .spawn()
            .map_err(|e| format!("Cannot start browser engine: {e}"))?;
        drop(child_read);
        drop(child_write);
        let mut s = Self {
            child,
            reader,
            writer,
            bytes: Vec::new(),
            next_id: 0,
            session: None,
            target: String::new(),
            main_frame: String::new(),
            popups: HashMap::new(),
            closing_popups: HashSet::new(),
            shared,
            stopped,
            info_dirty: false,
            mouse_down: false,
            zoom: 1.0,
            frame_acks: VecDeque::new(),
            next_ack: Instant::now(),
            frame_interval: Duration::from_millis(40),
            pending_navigation: None,
            pending_info: None,
            info_generation: 0,
        };
        let version = s.call("Browser.getVersion", json!({}))?;
        s.shared.lock().unwrap().info.engine_version =
            version["product"].as_str().unwrap_or("Chromium").into();
        let targets = s.call("Target.getTargets", json!({}))?;
        let target = targets["targetInfos"]
            .as_array()
            .and_then(|list| list.iter().find(|t| t["type"] == "page"))
            .map(|t| t["targetId"].clone());
        let target = match target {
            Some(t) => t,
            None => {
                s.call("Target.createTarget", json!({"url":"about:blank"}))?["targetId"].clone()
            }
        };
        let attached = s.call(
            "Target.attachToTarget",
            json!({"targetId":target,"flatten":true}),
        )?;
        s.target = target.as_str().ok_or("Missing page target")?.into();
        s.session = Some(
            attached["sessionId"]
                .as_str()
                .ok_or("Engine did not attach a page")?
                .into(),
        );
        s.call("Target.setDiscoverTargets", json!({"discover":true}))?;
        s.call("Page.enable", json!({}))?;
        let tree = s.call("Page.getFrameTree", json!({}))?;
        s.main_frame = tree["frameTree"]["frame"]["id"]
            .as_str()
            .ok_or("Missing main page frame")?
            .into();
        s.call("Runtime.enable", json!({}))?;
        s.call(
            "Emulation.setDeviceMetricsOverride",
            json!({"width":VIEW_WIDTH,"height":VIEW_HEIGHT,"deviceScaleFactor":1,"mobile":false}),
        )?;
        s.call(
            "Emulation.setEmulatedMedia",
            json!({"features":[{"name":"prefers-reduced-motion","value":"reduce"}]}),
        )?;
        // Downloads require a dedicated user-confirmed UI; deny them in this version.
        s.call("Browser.setDownloadBehavior", json!({"behavior":"deny"}))?;
        s.call("Page.startScreencast",json!({"format":"jpeg","quality":75,"maxWidth":VIEW_WIDTH,"maxHeight":VIEW_HEIGHT,"everyNthFrame":1}))?;
        Ok(s)
    }
    fn send(&mut self, method: &str, params: Value) -> Result<u64, String> {
        self.next_id += 1;
        let mut message = json!({"id":self.next_id,"method":method,"params":params});
        if !(method.starts_with("Browser.") || method.starts_with("Target.")) {
            if let Some(session) = &self.session {
                message["sessionId"] = json!(session);
            }
        }
        let bytes = serde_json::to_vec(&message).map_err(|e| e.to_string())?;
        self.writer
            .write_all(&bytes)
            .and_then(|_| self.writer.write_all(&[0]))
            .map_err(|e| format!("Browser pipe closed: {e}"))?;
        Ok(self.next_id)
    }
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.send(method, params)?;
        log::debug!("CDP call {id}: {method}");
        let deadline = Instant::now() + Duration::from_secs(12);
        while Instant::now() < deadline && !self.stopped.load(Ordering::Relaxed) {
            if let Some(v) = self.receive(Duration::from_millis(50))? {
                if v["id"].as_u64() != Some(id)
                    && self.shared.lock().unwrap().info.dialog.is_some()
                    && (method.starts_with("Input.") || method == "Page.navigate")
                {
                    // A page dialog may defer the input reply. Return control to
                    // the UI so it can send the user's dialog decision.
                    return Ok(json!({}));
                }
                if v["id"].as_u64() == Some(id) {
                    return if v.get("error").is_some() {
                        Err(format!("{method}: {}", v["error"]["message"]))
                    } else {
                        Ok(v["result"].clone())
                    };
                }
            }
        }
        Err(format!(
            "Browser did not answer {method}; check engine.log (sandbox and library requirements)."
        ))
    }
    pub fn evaluate(&mut self, expression: &str) -> Result<Value, String> {
        let result = self.call(
            "Runtime.evaluate",
            json!({"expression":expression,"returnByValue":true,"awaitPromise":true}),
        )?;
        if result.get("exceptionDetails").is_some() {
            return Err(format!(
                "Page script failed: {}",
                result["exceptionDetails"]
            ));
        }
        Ok(result["result"]["value"].clone())
    }
    pub fn pump(&mut self, timeout: Duration) -> Result<(), String> {
        self.receive(timeout)?;
        if self
            .pending_navigation
            .is_some_and(|(_, start)| start.elapsed() > Duration::from_secs(45))
        {
            self.pending_navigation = None;
            self.send("Page.stopLoading", json!({}))?;
            let mut state = self.shared.lock().unwrap();
            state.info.loading = false;
            state.info.error = Some("This page took too long to connect. Reload to retry.".into());
            state.revision += 1;
        }
        let expired = self
            .popups
            .iter()
            .filter(|(_, t)| t.elapsed() > Duration::from_secs(1))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in expired {
            self.popups.remove(&id);
            self.closing_popups.insert(id.clone());
            self.send("Target.closeTarget", json!({"targetId":id}))?;
        }
        if self.info_dirty
            && self.pending_info.is_none()
            && self.shared.lock().unwrap().info.dialog.is_none()
        {
            self.info_dirty = false;
            // A provisional navigation can defer Runtime.evaluate until HTTP
            // responds. Never wait synchronously for metadata in the input loop.
            let id=self.send("Runtime.evaluate",json!({"expression":"({title:document.title,status:performance.getEntriesByType('navigation')[0]?.responseStatus,password:document.activeElement?.type==='password',editable:!!(document.activeElement&&(document.activeElement.matches('input:not([type=button]):not([type=submit]),textarea')||document.activeElement.isContentEditable))})", "returnByValue":true}))?;
            self.pending_info = Some((id, self.info_generation));
        }
        Ok(())
    }
    fn receive(&mut self, timeout: Duration) -> Result<Option<Value>, String> {
        // Backpressure animated pages to the frontend's active/idle frame budget.
        // Input replies continue to be handled while a frame acknowledgement waits.
        if Instant::now() >= self.next_ack {
            if let Some(id) = self.frame_acks.pop_front() {
                self.send("Page.screencastFrameAck", json!({"sessionId":id}))?;
                self.next_ack = Instant::now() + self.frame_interval;
            }
        }
        if let Some(index) = self.bytes.iter().position(|c| *c == 0) {
            let message = self.bytes.drain(..=index).collect::<Vec<_>>();
            let value: Value = serde_json::from_slice(&message[..index])
                .map_err(|e| format!("Invalid browser protocol: {e}"))?;
            log::trace!(
                "CDP received id={:?} method={:?}",
                value["id"],
                value["method"]
            );
            self.event(&value)?;
            return Ok(Some(value));
        }
        let mut fd = libc::pollfd {
            fd: self.reader.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut fd, 1, timeout.as_millis().min(1000) as i32) };
        if ready < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if ready == 0 {
            return Ok(None);
        }
        let mut chunk = [0u8; 32768];
        let count = self.reader.read(&mut chunk).map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("Browser engine stopped. See the private profile's engine.log.".into());
        }
        if self.bytes.len() + count > MAX_MESSAGE {
            return Err("Browser message exceeded the bounded transport limit.".into());
        }
        self.bytes.extend_from_slice(&chunk[..count]);
        self.receive(Duration::ZERO)
    }
    fn event(&mut self, value: &Value) -> Result<(), String> {
        if value.get("method").is_none() {
            if self
                .pending_info
                .is_some_and(|(id, _)| value["id"].as_u64() == Some(id))
            {
                let (_, generation) = self.pending_info.take().unwrap();
                let info = &value["result"]["result"]["value"];
                if generation == self.info_generation && info.is_object() {
                    let mut state = self.shared.lock().unwrap();
                    state.info.title = info["title"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(512)
                        .collect();
                    state.info.editable = info["editable"].as_bool().unwrap_or(false);
                    state.info.password = info["password"].as_bool().unwrap_or(false);
                    state.info.http_status =
                        info["status"].as_u64().and_then(|n| u16::try_from(n).ok());
                    state.revision += 1;
                }
            }
            if self
                .pending_navigation
                .as_ref()
                .is_some_and(|(id, _)| value["id"].as_u64() == Some(*id))
            {
                self.pending_navigation = None;
                if let Some(error) = value["result"]["errorText"]
                    .as_str()
                    .or_else(|| value["error"]["message"].as_str())
                {
                    let mut state = self.shared.lock().unwrap();
                    state.info.loading = false;
                    state.info.error = Some(format!("Page load failed: {error}"));
                    state.revision += 1;
                }
            }
            return Ok(());
        }
        let method = value["method"].as_str().unwrap_or("");
        if method == "Target.targetDestroyed" {
            if let Some(id) = value["params"]["targetId"].as_str() {
                self.popups.remove(id);
                self.closing_popups.remove(id);
            }
            return Ok(());
        }
        if method == "Target.targetCreated" || method == "Target.targetInfoChanged" {
            let info = &value["params"]["targetInfo"];
            if info["type"] == "page" {
                if let Some(id) = info["targetId"]
                    .as_str()
                    .filter(|id| *id != self.target && !self.closing_popups.contains(*id))
                {
                    let id = id.to_string();
                    let url = info["url"].as_str().unwrap_or("");
                    if let Some(url) = url
                        .starts_with("http://")
                        .then_some(url)
                        .or_else(|| url.starts_with("https://").then_some(url))
                        .and_then(|u| address(u).ok())
                    {
                        // Keep one active page: follow a new-window web link in
                        // this page, preserving its normal Back history.
                        self.send("Page.navigate", json!({"url":url}))?;
                        self.send("Target.closeTarget", json!({"targetId":id}))?;
                        self.popups.remove(&id);
                        self.closing_popups.insert(id);
                    } else if self.popups.len() < 4 {
                        self.popups.entry(id).or_insert_with(Instant::now);
                    } else {
                        self.send("Target.closeTarget", json!({"targetId":id}))?;
                    }
                }
            }
            return Ok(());
        }
        if let Some(session) = &self.session {
            if value["sessionId"].as_str() != Some(session) {
                return Ok(());
            }
        }
        let p = &value["params"];
        match value["method"].as_str().unwrap_or("") {
            "Page.screencastFrame" => {
                if let Some(id) = p["sessionId"].as_u64() {
                    if self.frame_acks.len() >= 4 {
                        return Err("Browser exceeded its bounded frame window.".into());
                    }
                    self.frame_acks.push_back(id);
                }
                if let Some(data) = p["data"].as_str().filter(|s| s.len() < 2 * 1024 * 1024) {
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) {
                        let mut reader = image::ImageReader::with_format(
                            Cursor::new(bytes),
                            image::ImageFormat::Jpeg,
                        );
                        let mut limits = image::Limits::default();
                        limits.max_image_width = Some(VIEW_WIDTH);
                        limits.max_image_height = Some(VIEW_HEIGHT);
                        limits.max_alloc = Some(16 * 1024 * 1024);
                        reader.limits(limits);
                        if let Ok(decoded) = reader.decode() {
                            log::trace!(
                                "Web frame {}x{} at zoom {}",
                                decoded.width(),
                                decoded.height(),
                                self.zoom
                            );
                            if decoded.width() == VIEW_WIDTH && decoded.height() == VIEW_HEIGHT {
                                let mut state = self.shared.lock().unwrap();
                                state.frame = Some(Frame {
                                    width: VIEW_WIDTH,
                                    height: VIEW_HEIGHT,
                                    rgb: decoded.into_rgb8().into_raw(),
                                });
                                state.frame_count += 1;
                                state.revision += 1;
                            }
                        }
                    }
                }
            }
            "Page.frameStartedLoading" if p["frameId"].as_str() == Some(&self.main_frame) => {
                let mut s = self.shared.lock().unwrap();
                s.info.loading = true;
                s.revision += 1;
            }
            "Page.loadEventFired" | "Page.frameStoppedLoading"
                if p.get("frameId").is_none()
                    || p["frameId"].as_str() == Some(&self.main_frame) =>
            {
                let mut s = self.shared.lock().unwrap();
                s.info.loading = false;
                s.revision += 1;
                self.info_dirty = true;
            }
            "Page.navigatedWithinDocument" if p["frameId"].as_str() == Some(&self.main_frame) => {
                let mut s = self.shared.lock().unwrap();
                s.info.url = p["url"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(crate::model::MAX_URL)
                    .collect();
                s.revision += 1;
                self.info_dirty = true;
            }
            "Page.frameNavigated" if p["frame"].get("parentId").is_none() => {
                self.info_generation = self.info_generation.wrapping_add(1);
                if let Some(id) = p["frame"]["id"].as_str() {
                    self.main_frame = id.into();
                }
                let mut s = self.shared.lock().unwrap();
                s.info.url = p["frame"]["url"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(crate::model::MAX_URL)
                    .collect();
                s.info.error = None;
                s.revision += 1;
                self.info_dirty = true;
            }
            "Page.javascriptDialogOpening" => {
                let mut s = self.shared.lock().unwrap();
                s.info.dialog = Some(
                    p["message"]
                        .as_str()
                        .unwrap_or("Website dialog")
                        .chars()
                        .take(800)
                        .collect(),
                );
                s.revision += 1;
            }
            "Page.javascriptDialogClosed" => {
                let mut s = self.shared.lock().unwrap();
                s.info.dialog = None;
                s.revision += 1;
            }
            "Inspector.targetCrashed" => {
                return Err("This page stopped responding. Reopen the browser to retry.".into());
            }
            _ => {}
        }
        Ok(())
    }
    pub fn action(&mut self, action: Action) -> Result<(), String> {
        match action {
            Action::Navigate(url) => {
                let url = address(&url)?;
                self.info_generation = self.info_generation.wrapping_add(1);
                {
                    let mut s = self.shared.lock().unwrap();
                    s.info.loading = true;
                    s.info.error = None;
                    s.info.http_status = None;
                    s.info.title.clear();
                    s.info.editable = false;
                    s.info.password = false;
                    s.revision += 1;
                }
                let id = self.send("Page.navigate", json!({"url":url}))?;
                self.pending_navigation = Some((id, Instant::now()));
            }
            Action::Back | Action::Forward => {
                let delta = if matches!(action, Action::Back) {
                    -1
                } else {
                    1
                };
                let history = self.call("Page.getNavigationHistory", json!({}))?;
                let index = history["currentIndex"].as_i64().unwrap_or(0) + delta;
                if let Some(entry) = history["entries"]
                    .as_array()
                    .and_then(|e| usize::try_from(index).ok().and_then(|i| e.get(i)))
                {
                    self.call(
                        "Page.navigateToHistoryEntry",
                        json!({"entryId":entry["id"]}),
                    )?;
                }
            }
            Action::Reload => {
                self.call("Page.reload", json!({}))?;
            }
            Action::Stop => {
                self.pending_navigation = None;
                self.call("Page.stopLoading", json!({}))?;
                self.shared.lock().unwrap().info.loading = false;
            }
            // Positions use the native view's pixels. Chromium applies the
            // emulated view scale; dividing here a second time misplaces clicks.
            Action::Pointer(x, y) => {
                self.call(
                    "Input.dispatchMouseEvent",
                    json!({"type":"mouseMoved","x":x,"y":y,"buttons":if self.mouse_down{1}else{0}}),
                )?;
            }
            Action::Button(x, y, down) => {
                self.mouse_down = down;
                self.call("Input.dispatchMouseEvent",json!({"type":if down{"mousePressed"}else{"mouseReleased"},"x":x,"y":y,"button":"left","buttons":if down{1}else{0},"clickCount":1}))?;
                if !down {
                    self.info_dirty = true;
                }
            }
            Action::Click(x, y) => {
                for kind in ["mousePressed", "mouseReleased"] {
                    self.call(
                        "Input.dispatchMouseEvent",
                        json!({"type":kind,"x":x,"y":y,"button":"left","clickCount":1}),
                    )?;
                }
                self.info_dirty = true;
            }
            Action::Scroll(x, y, dx, dy) => {
                self.call("Input.dispatchMouseEvent",json!({"type":"mouseWheel","x":x,"y":y,"deltaX":dx/self.zoom,"deltaY":dy/self.zoom}))?;
            }
            Action::Key(key, shift) => {
                let code = match key.as_str() {
                    "Tab" => 9,
                    "Enter" => 13,
                    "Backspace" => 8,
                    "Escape" => 27,
                    _ => 0,
                };
                for kind in ["keyDown", "keyUp"] {
                    self.call("Input.dispatchKeyEvent",json!({"type":kind,"key":key,"code":key,"windowsVirtualKeyCode":code,"text":if kind=="keyDown" && key=="Enter" {"\r"}else{""},"unmodifiedText":if kind=="keyDown" && key=="Enter" {"\r"}else{""},"modifiers":if shift {8} else {0}}))?;
                }
                self.info_dirty = true;
            }
            Action::Text(text) => {
                if text.len() > 8192 {
                    return Err("Text entry is too long.".into());
                }
                self.call("Input.insertText", json!({"text":text}))?;
            }
            Action::ReplaceText(text) => {
                if text.len() > 8192 {
                    return Err("Text entry is too long.".into());
                }
                // Browser editing command, not a script assignment: native
                // input/beforeinput handlers keep working and passwords are
                // never copied from the page into the controller UI.
                self.call("Input.dispatchKeyEvent",json!({"type":"rawKeyDown","key":"a","code":"KeyA","windowsVirtualKeyCode":65,"commands":["selectAll"]}))?;
                self.call(
                    "Input.dispatchKeyEvent",
                    json!({"type":"keyUp","key":"a","code":"KeyA","windowsVirtualKeyCode":65}),
                )?;
                self.call("Input.insertText", json!({"text":text}))?;
                self.info_dirty = true;
            }
            Action::FrameRate(fps) => {
                self.frame_interval = Duration::from_secs_f64(1.0 / fps.clamp(1, 30) as f64);
            }
            Action::Visible(visible) => {
                self.frame_acks.clear();
                if visible {
                    self.call("Page.startScreencast",json!({"format":"jpeg","quality":75,"maxWidth":VIEW_WIDTH,"maxHeight":VIEW_HEIGHT,"everyNthFrame":1}))?;
                } else {
                    self.call("Page.stopScreencast", json!({}))?;
                }
            }
            Action::Zoom(zoom) => {
                let allowed = [0.75_f64, 0.9, 1.0, 1.2, 1.5, 1.8, 2.0];
                if !zoom.is_finite() {
                    return Err("Invalid zoom".into());
                }
                self.zoom = *allowed
                    .iter()
                    .min_by(|a, b| (**a - zoom).abs().total_cmp(&(**b - zoom).abs()))
                    .unwrap();
                // Keep the compositor at native resolution while reflowing the
                // layout. Otherwise screencast shrinks to CSS dimensions and
                // the UI either freezes or has to upscale blurry text.
                self.call("Emulation.setDeviceMetricsOverride",json!({"width":(VIEW_WIDTH as f64/self.zoom).round() as u32,"height":(VIEW_HEIGHT as f64/self.zoom).round() as u32,"deviceScaleFactor":self.zoom,"mobile":false,"scale":self.zoom,"dontSetVisibleSize":true}))?;
                self.call(
                    "Emulation.setVisibleSize",
                    json!({"width":VIEW_WIDTH,"height":VIEW_HEIGHT}),
                )?;
            }
            Action::Dialog(accept) => {
                self.call("Page.handleJavaScriptDialog", json!({"accept":accept}))?;
                let mut s = self.shared.lock().unwrap();
                s.info.dialog = None;
                s.revision += 1;
            }
            Action::Quit => {
                self.stopped.store(true, Ordering::Relaxed);
            }
        }
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.send("Browser.close", json!({}));
        let deadline = Instant::now() + Duration::from_millis(800);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGTERM);
        }
        thread::sleep(Duration::from_millis(100));
        if self.child.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            let _ = self.child.wait();
        }
    }
}

pub struct BrowserEngine {
    pub shared: Shared,
    commands: mpsc::SyncSender<Action>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl BrowserEngine {
    pub fn unavailable(message: String) -> Self {
        let (commands, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        let shared = Shared::default();
        {
            let mut state = shared.lock().unwrap();
            state.info.error = Some(message);
            state.revision = 1;
        }
        Self {
            shared,
            commands,
            stopped: Arc::new(AtomicBool::new(true)),
            worker: None,
        }
    }
    pub fn start(executable: PathBuf, profile: PathBuf, initial: Option<String>) -> Self {
        let shared = Shared::default();
        let state = shared.clone();
        let (tx, rx) = mpsc::sync_channel(64);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let worker = thread::spawn(move || {
            let run = || -> Result<(), String> {
                let mut session =
                    Session::start_controlled(&executable, &profile, state.clone(), stop.clone())?;
                if let Some(url) = initial {
                    session.action(Action::Navigate(url))?;
                }
                let mut batch = Vec::with_capacity(64);
                while !stop.load(Ordering::Relaxed) {
                    for _ in 0..64 {
                        match rx.try_recv() {
                            Ok(Action::Quit) => return Ok(()),
                            Ok(action) => queue_action(&mut batch, action),
                            Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                            Err(mpsc::TryRecvError::Empty) => break,
                        }
                    }
                    for action in batch.drain(..) {
                        if stop.load(Ordering::Relaxed) {
                            return Ok(());
                        }
                        if let Err(e) = session.action(action) {
                            let mut s = state.lock().unwrap();
                            s.info.error = Some(e);
                            s.info.loading = false;
                            s.revision += 1;
                        }
                    }
                    session.pump(Duration::from_millis(20))?;
                }
                Ok(())
            };
            if let Err(e) = run() {
                let mut s = state.lock().unwrap();
                s.info.error = Some(e);
                s.info.loading = false;
                s.revision += 1;
            }
        });
        Self {
            shared,
            commands: tx,
            stopped,
            worker: Some(worker),
        }
    }
    pub fn send(&self, action: Action) -> Result<(), String> {
        self.commands.try_send(action).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => "Browser is busy; retry in a moment.".into(),
            mpsc::TrySendError::Disconnected(_) => self
                .shared
                .lock()
                .unwrap()
                .info
                .error
                .clone()
                .unwrap_or_else(|| {
                    "Browser engine stopped. Exit and reopen Browser to retry.".into()
                }),
        })
    }
}
impl Drop for BrowserEngine {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = self.commands.try_send(Action::Quit);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_motion_is_coalesced_without_losing_click_order() {
        let mut batch = vec![];
        for action in [
            Action::Pointer(1.0, 2.0),
            Action::Pointer(4.0, 5.0),
            Action::Button(4.0, 5.0, true),
            Action::Pointer(8.0, 9.0),
            Action::Pointer(10.0, 11.0),
            Action::Button(10.0, 11.0, false),
            Action::Scroll(1.0, 1.0, 0.0, 130.0),
            Action::Scroll(2.0, 3.0, 0.0, 130.0),
        ] {
            queue_action(&mut batch, action);
        }
        assert!(matches!(
            batch.as_slice(),
            [
                Action::Pointer(4.0, 5.0),
                Action::Button(4.0, 5.0, true),
                Action::Pointer(10.0, 11.0),
                Action::Button(10.0, 11.0, false),
                Action::Scroll(2.0, 3.0, 0.0, 260.0)
            ]
        ));
    }
}
