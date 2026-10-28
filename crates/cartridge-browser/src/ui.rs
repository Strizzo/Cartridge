use crate::{
    engine::{Action, BrowserEngine, PageInfo, Permission, discover},
    model::{
        Bookmark, DEFAULT_ZOOM, Library, VIEW_HEIGHT, VIEW_WIDTH, VIEW_Y, ZOOM_LEVELS, address,
        site_key,
    },
};
use cartridge_core::{
    font::FontCache,
    image_cache::ImageCache,
    input::{self, Button, InputAction, InputManager},
    screen::Screen,
    sticks::{Stick, StickManager},
    text_cache::TextCache,
    theme::Theme,
    ui::text_input::{TextInput, TextInputResult},
};
use sdl2::{
    event::Event,
    keyboard::Keycode,
    pixels::{Color, PixelFormatEnum},
    rect::Rect,
};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Options {
    pub initial: Option<String>,
    pub engine: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    pub capture: Option<PathBuf>,
    pub capture_after: Option<f64>,
    pub seconds: Option<f64>,
    pub hidden: bool,
}
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Home,
    Page,
    Menu,
    Library,
    Tabs,
    Permissions,
    Downloads,
}
#[derive(Clone, Copy, PartialEq)]
enum Entry {
    Address,
    Field,
    Prompt(u64),
}
#[derive(Clone, Copy)]
struct FocusRequest {
    since: Instant,
    serial: u64,
    activate: bool,
    trigger_held: bool,
}
struct SaveRetry {
    next: Instant,
    delay: Duration,
}
impl SaveRetry {
    fn new(now: Instant) -> Self {
        Self {
            next: now + Duration::from_secs(2),
            delay: Duration::from_secs(2),
        }
    }
    fn failed(&mut self, now: Instant) {
        self.next = now + self.delay;
        self.delay = (self.delay * 2).min(Duration::from_secs(60));
    }
    fn saved(&mut self, now: Instant) {
        *self = Self::new(now);
    }
}
struct Ui {
    mode: Mode,
    selected: usize,
    history: bool,
    keyboard: TextInput,
    keyboard_trigger: Option<Button>,
    entry: Entry,
    library: Library,
    library_path: PathBuf,
    info: PageInfo,
    message: Option<String>,
    pointer: [f64; 2],
    pointer_mode: bool,
    dragging: bool,
    zoom: f64,
    zoom_site: Option<String>,
    library_dirty: bool,
    save_retry: SaveRetry,
    left: [f32; 2],
    right: [f32; 2],
    pending_focus: Option<FocusRequest>,
    focus_serial: u64,
    modal_id: u64,
    modal_selected: usize,
    modal_allow: bool,
    upload_selected: Vec<String>,
}
const QUICK: [(&str, &str); 4] = [
    ("SLASHDOT", "https://slashdot.org/"),
    ("CNN", "https://www.cnn.com/"),
    ("OPENSTREETMAP", "https://www.openstreetmap.org/"),
    ("SEARCH THE WEB", "https://duckduckgo.com/"),
];
const MENU: [&str; 11] = [
    "Reload page",
    "Type in focused field",
    "Press Enter / submit form",
    "Save / remove bookmark",
    "History",
    "Tabs (maximum 2)",
    "Site permissions",
    "Downloads / Files",
    "Browser home",
    "Stop loading",
    "Exit browser",
];
const PERMISSIONS: [Permission; 4] = [
    Permission::Location,
    Permission::Notifications,
    Permission::Camera,
    Permission::Microphone,
];
impl Ui {
    fn new(root: &Path) -> Self {
        let library_path = root.join("library.json");
        let library = Library::load(&library_path);
        let recovery_notice = library.recovery_notice.clone();
        Self {
            mode: Mode::Home,
            selected: 0,
            history: false,
            keyboard: TextInput::new("Address or search"),
            keyboard_trigger: None,
            entry: Entry::Address,
            library,
            library_path,
            info: PageInfo::default(),
            message: recovery_notice,
            pointer: [360.0, 240.0],
            pointer_mode: false,
            dragging: false,
            zoom: f64::from(DEFAULT_ZOOM) / 100.0,
            zoom_site: None,
            library_dirty: false,
            save_retry: SaveRetry::new(Instant::now()),
            left: [0.0; 2],
            right: [0.0; 2],
            pending_focus: None,
            focus_serial: 0,
            modal_id: 0,
            modal_selected: 0,
            modal_allow: false,
            upload_selected: Vec::new(),
        }
    }
    fn save_pending(&mut self, now: Instant, force: bool) -> Result<bool, String> {
        if !self.library_dirty || (!force && now < self.save_retry.next) {
            return Ok(false);
        }
        match self.library.save(&self.library_path) {
            Ok(()) => {
                self.library_dirty = false;
                self.save_retry.saved(now);
                Ok(true)
            }
            Err(error) => {
                self.save_retry.failed(now);
                self.message = Some(format!("Browser data not saved; will retry: {error}"));
                Err(error)
            }
        }
    }
    fn send(&mut self, engine: &BrowserEngine, action: Action) {
        if let Err(e) = engine.send(action) {
            self.message = Some(e);
        }
    }
    fn apply_site_zoom(&mut self, engine: &BrowserEngine, url: &str) {
        let key = site_key(url);
        if key.is_some() && key != self.zoom_site {
            self.zoom_site = key;
            self.zoom = f64::from(self.library.zoom_for(url)) / 100.0;
            self.send(engine, Action::Zoom(self.zoom));
        }
    }
    fn set_zoom(&mut self, engine: &BrowserEngine, percent: u16) {
        self.zoom = f64::from(percent) / 100.0;
        self.send(engine, Action::Zoom(self.zoom));
        // The requested origin is already known before navigation metadata arrives.
        let url = self.zoom_site.as_deref().unwrap_or(&self.info.url);
        self.library_dirty |= self.library.remember_zoom(url, percent);
    }
    fn navigate(&mut self, engine: &BrowserEngine, input: &str) {
        self.pending_focus = None;
        match address(input) {
            Ok(url) => {
                self.apply_site_zoom(engine, &url);
                self.send(engine, Action::Navigate(url));
                self.mode = Mode::Page;
                self.message = None;
            }
            Err(e) => self.message = Some(e),
        }
    }
    fn open_keyboard(&mut self, entry: Entry) {
        self.entry = entry;
        self.pending_focus = None;
        self.keyboard_trigger = None;
        self.keyboard.show(match entry {
            Entry::Address => "Address or search",
            Entry::Field => "Replace field text / R2 also sends Enter",
            Entry::Prompt(_) => "Website prompt / Start OK / Select Cancel",
        });
        self.keyboard.masked =
            matches!(entry, Entry::Field) && (self.info.password || !self.info.editable);
        self.keyboard.max_len = match entry {
            Entry::Address => 4096,
            Entry::Field | Entry::Prompt(_) => 2048,
        };
        self.left = [0.0; 2];
        self.right = [0.0; 2];
    }
    fn request_focus(&mut self, engine: &BrowserEngine, activate: bool) {
        self.focus_serial = self.focus_serial.wrapping_add(1).max(1);
        self.pending_focus = Some(FocusRequest {
            since: Instant::now(),
            serial: self.focus_serial,
            activate,
            trigger_held: activate,
        });
        self.send(engine, Action::FocusInfo(self.focus_serial));
    }
    fn submit(&mut self, engine: &BrowserEngine, text: String) {
        match self.entry {
            Entry::Address => self.navigate(engine, &text),
            Entry::Field => self.send(engine, Action::ReplaceText(text)),
            Entry::Prompt(id) => self.send(
                engine,
                Action::Prompt {
                    id,
                    text: Some(text),
                },
            ),
        }
    }
    fn cancel_keyboard(&mut self, engine: &BrowserEngine) {
        self.keyboard.visible = false;
        if let Entry::Prompt(id) = self.entry {
            self.send(engine, Action::Prompt { id, text: None });
        }
    }
    fn modal(&self) -> bool {
        self.info.javascript_dialog.is_some()
            || self.info.permission.is_some()
            || self.info.upload.is_some()
            || self.info.download.as_ref().is_some_and(|d| d.pending)
    }
    fn sync_modal(&mut self) {
        let id = self
            .info
            .javascript_dialog
            .as_ref()
            .map(|p| p.id)
            .or(self.info.permission.as_ref().map(|p| p.id))
            .or(self.info.upload.as_ref().map(|p| p.id))
            .or(self
                .info
                .download
                .as_ref()
                .filter(|d| d.pending)
                .map(|p| p.id))
            .unwrap_or(0);
        if id != self.modal_id {
            self.modal_id = id;
            self.modal_selected = 0;
            self.modal_allow = false;
            self.upload_selected.clear();
            self.pending_focus = None;
            self.left = [0.0; 2];
            self.right = [0.0; 2];
        }
        if matches!(self.entry,Entry::Prompt(id) if self.info.javascript_dialog.as_ref().is_none_or(|p|p.id!=id))
        {
            self.keyboard.visible = false;
        }
    }
    fn press_modal(&mut self, engine: &BrowserEngine, button: Button) {
        if let Some(p) = self.info.javascript_dialog.clone() {
            match button {
                Button::B => self.send(
                    engine,
                    Action::Prompt {
                        id: p.id,
                        text: None,
                    },
                ),
                Button::A if p.kind == "prompt" => {
                    self.open_keyboard(Entry::Prompt(p.id));
                    self.keyboard.label = format!("{} / Website prompt", p.origin);
                    self.keyboard.set_text(&p.default_text);
                    self.keyboard_trigger = Some(Button::A);
                }
                Button::A => self.send(
                    engine,
                    Action::Prompt {
                        id: p.id,
                        text: Some(String::new()),
                    },
                ),
                _ => {}
            }
        } else if let Some(p) = self.info.permission.clone() {
            match button {
                Button::DpadLeft => self.modal_allow = false,
                Button::DpadRight => self.modal_allow = true,
                Button::B => self.send(
                    engine,
                    Action::PermissionDecision {
                        id: p.id,
                        allow: false,
                    },
                ),
                Button::A => self.send(
                    engine,
                    Action::PermissionDecision {
                        id: p.id,
                        allow: self.modal_allow,
                    },
                ),
                _ => {}
            }
        } else if let Some(p) = self.info.upload.clone() {
            match button {
                Button::DpadUp => self.modal_selected = self.modal_selected.saturating_sub(1),
                Button::DpadDown => {
                    self.modal_selected = (self.modal_selected + 1).min(p.files.len())
                }
                Button::B => self.send(
                    engine,
                    Action::Upload {
                        id: p.id,
                        names: vec![],
                    },
                ),
                Button::A if self.modal_selected == 0 => self.send(
                    engine,
                    Action::Upload {
                        id: p.id,
                        names: vec![],
                    },
                ),
                Button::A | Button::X => {
                    if let Some(file) = p.files.get(self.modal_selected.saturating_sub(1)) {
                        if !p.multiple {
                            self.send(
                                engine,
                                Action::Upload {
                                    id: p.id,
                                    names: vec![file.name.clone()],
                                },
                            );
                        } else if let Some(i) =
                            self.upload_selected.iter().position(|n| n == &file.name)
                        {
                            self.upload_selected.remove(i);
                        } else if self.upload_selected.len() < 8 {
                            self.upload_selected.push(file.name.clone());
                        }
                    }
                }
                Button::Start if p.multiple => self.send(
                    engine,
                    Action::Upload {
                        id: p.id,
                        names: self.upload_selected.clone(),
                    },
                ),
                _ => {}
            }
        } else if let Some(p) = self.info.download.clone().filter(|d| d.pending) {
            match button {
                Button::DpadLeft => self.modal_allow = false,
                Button::DpadRight => self.modal_allow = true,
                Button::B => self.send(
                    engine,
                    Action::Download {
                        id: p.id,
                        allow: false,
                    },
                ),
                Button::A => self.send(
                    engine,
                    Action::Download {
                        id: p.id,
                        allow: self.modal_allow,
                    },
                ),
                _ => {}
            }
        }
    }
    fn entries(&self) -> &[Bookmark] {
        if self.history {
            &self.library.history
        } else {
            &self.library.bookmarks
        }
    }
    fn press(&mut self, engine: &BrowserEngine, button: Button, action: InputAction) -> bool {
        if button == Button::A && action == InputAction::Release {
            if let Some(request) = &mut self.pending_focus {
                request.trigger_held = false;
            }
        }
        if !self.keyboard.visible && action == InputAction::Press && button != Button::A {
            self.pending_focus = None;
        }
        if self.keyboard.visible {
            if self.keyboard_trigger == Some(button) {
                if action == InputAction::Release {
                    self.keyboard_trigger = None;
                }
                return false;
            }
            if action == InputAction::Repeat
                && matches!(button, Button::Start | Button::Select | Button::X)
            {
                return false;
            }
            if self.entry == Entry::Field && button == Button::R2 && action == InputAction::Press {
                let text = self.keyboard.text.clone();
                self.keyboard.visible = false;
                self.submit(engine, text);
                self.send(engine, Action::Key("Enter".into(), false));
                return false;
            }
            match self
                .keyboard
                .handle_input(&input::InputEvent { button, action })
            {
                TextInputResult::Submitted(s) => self.submit(engine, s),
                TextInputResult::Cancelled => self.cancel_keyboard(engine),
                _ => {}
            }
            return false;
        }
        if action == InputAction::Repeat
            && !matches!(
                button,
                Button::DpadUp | Button::DpadDown | Button::DpadLeft | Button::DpadRight
            )
        {
            return false;
        }
        if button == Button::Select && action == InputAction::Press {
            return true;
        }
        if self.modal() {
            if action != InputAction::Release {
                self.press_modal(engine, button);
            }
            return false;
        }
        if (self.message.is_some() || self.info.notice.is_some() || self.info.error.is_some())
            && button == Button::B
            && action == InputAction::Press
        {
            self.message = None;
            self.send(engine, Action::DismissNotice);
            return false;
        }
        if self.mode == Mode::Page && self.pointer_mode && button == Button::A {
            if action == InputAction::Press || action == InputAction::Release {
                self.dragging = action == InputAction::Press;
                self.send(
                    engine,
                    Action::Button(self.pointer[0], self.pointer[1], self.dragging),
                );
                if !self.dragging {
                    self.request_focus(engine, false);
                }
            }
            return false;
        }
        if action == InputAction::Release {
            return false;
        }
        if button == Button::Select {
            return true;
        }
        match self.mode {
            Mode::Home => match button {
                Button::DpadUp => self.selected = self.selected.saturating_sub(1),
                Button::DpadDown => self.selected = (self.selected + 1).min(QUICK.len()),
                Button::A => {
                    if self.selected == QUICK.len() {
                        self.open_keyboard(Entry::Address);
                    } else {
                        self.navigate(engine, QUICK[self.selected].1);
                    }
                }
                Button::X => self.open_keyboard(Entry::Address),
                Button::Y => {
                    self.mode = Mode::Library;
                    self.selected = 0;
                }
                Button::B if self.info.url.starts_with("http") => self.mode = Mode::Page,
                Button::Start => {
                    self.mode = Mode::Menu;
                    self.selected = 0;
                }
                _ => {}
            },
            Mode::Page => match button {
                Button::DpadUp => self.send(
                    engine,
                    Action::Scroll(self.pointer[0], self.pointer[1], 0.0, -130.0),
                ),
                Button::DpadDown => self.send(
                    engine,
                    Action::Scroll(self.pointer[0], self.pointer[1], 0.0, 130.0),
                ),
                Button::DpadLeft | Button::DpadRight => {
                    self.pointer_mode = false;
                    self.send(
                        engine,
                        Action::Key("Tab".into(), button == Button::DpadLeft),
                    );
                }
                Button::A => self.request_focus(engine, true),
                Button::B | Button::L1 => self.send(engine, Action::Back),
                Button::R1 => self.send(engine, Action::Forward),
                Button::L2 | Button::R2 => {
                    let values = ZOOM_LEVELS;
                    let index = values
                        .iter()
                        .position(|z| f64::from(*z) / 100.0 == self.zoom)
                        .unwrap_or(3);
                    let index = if button == Button::R2 {
                        (index + 1).min(values.len() - 1)
                    } else {
                        index.saturating_sub(1)
                    };
                    self.set_zoom(engine, values[index]);
                }
                Button::X => self.open_keyboard(Entry::Address),
                Button::Y => {
                    self.mode = Mode::Library;
                    self.selected = 0;
                    self.history = false;
                }
                Button::Start => {
                    self.mode = Mode::Menu;
                    self.selected = 0;
                }
                _ => {}
            },
            Mode::Menu => match button {
                Button::DpadUp => self.selected = self.selected.saturating_sub(1),
                Button::DpadDown => self.selected = (self.selected + 1).min(MENU.len() - 1),
                Button::B | Button::Start => self.mode = Mode::Page,
                Button::A => match self.selected {
                    0 => {
                        self.send(engine, Action::Reload);
                        self.mode = Mode::Page;
                    }
                    1 => {
                        self.mode = Mode::Page;
                        self.open_keyboard(Entry::Field);
                    }
                    2 => {
                        self.send(engine, Action::Key("Enter".into(), false));
                        self.mode = Mode::Page;
                    }
                    3 => {
                        self.library
                            .toggle_bookmark(&self.info.title, &self.info.url);
                        self.library_dirty = true;
                        self.mode = Mode::Page;
                    }
                    4 => {
                        self.mode = Mode::Library;
                        self.history = true;
                        self.selected = 0;
                    }
                    5 => {
                        self.mode = Mode::Tabs;
                        self.selected = 0;
                    }
                    6 => {
                        self.mode = Mode::Permissions;
                        self.selected = 0;
                    }
                    7 => {
                        self.mode = Mode::Downloads;
                        self.selected = 0;
                    }
                    8 => {
                        self.mode = Mode::Home;
                        self.selected = 0;
                    }
                    9 => {
                        self.send(engine, Action::Stop);
                        self.mode = Mode::Page;
                    }
                    _ => return true,
                },
                _ => {}
            },
            Mode::Tabs => match button {
                Button::DpadUp => self.selected = self.selected.saturating_sub(1),
                Button::DpadDown => self.selected = (self.selected + 1).min(self.info.tabs.len()),
                Button::B => self.mode = Mode::Page,
                Button::A if self.selected == self.info.tabs.len() => {
                    self.send(engine, Action::NewTab);
                    self.mode = Mode::Page;
                    self.zoom_site = None;
                }
                Button::A => {
                    if let Some(tab) = self.info.tabs.get(self.selected).cloned() {
                        self.send(engine, Action::SwitchTab(tab.id));
                        self.mode = Mode::Page;
                        self.zoom_site = None;
                    }
                }
                Button::X => {
                    if let Some(tab) = self.info.tabs.get(self.selected).cloned() {
                        self.send(engine, Action::CloseTab(tab.id));
                        self.selected = 0;
                    }
                }
                _ => {}
            },
            Mode::Permissions => match button {
                Button::DpadUp => self.selected = self.selected.saturating_sub(1),
                Button::DpadDown => self.selected = (self.selected + 1).min(PERMISSIONS.len()),
                Button::B => self.mode = Mode::Page,
                Button::A if self.selected == PERMISSIONS.len() => {
                    self.send(engine, Action::RevokePermissions)
                }
                Button::A => self.send(
                    engine,
                    Action::RequestPermission(PERMISSIONS[self.selected]),
                ),
                _ => {}
            },
            Mode::Downloads => match button {
                Button::B => self.mode = Mode::Page,
                Button::X => {
                    if let Some(d) = self.info.download.clone().filter(|d| d.active) {
                        self.send(
                            engine,
                            Action::Download {
                                id: d.id,
                                allow: false,
                            },
                        );
                    }
                }
                _ => {}
            },
            Mode::Library => match button {
                Button::DpadUp => self.selected = self.selected.saturating_sub(1),
                Button::DpadDown => {
                    self.selected = (self.selected + 1).min(self.entries().len().saturating_sub(1))
                }
                Button::R1 | Button::L1 => {
                    self.history = !self.history;
                    self.selected = 0;
                }
                Button::A => {
                    if let Some(b) = self.entries().get(self.selected).cloned() {
                        self.navigate(engine, &b.url);
                    }
                }
                Button::B => {
                    self.mode = if self.info.url.starts_with("http") {
                        Mode::Page
                    } else {
                        Mode::Home
                    }
                }
                Button::X if !self.history => {
                    if self.selected < self.library.bookmarks.len() {
                        self.library.bookmarks.remove(self.selected);
                        self.selected = self
                            .selected
                            .min(self.library.bookmarks.len().saturating_sub(1));
                        self.library_dirty = true;
                    }
                }
                _ => {}
            },
        }
        if self.keyboard.visible {
            self.keyboard_trigger = Some(button);
        }
        false
    }
    fn update(&mut self, engine: &BrowserEngine, dt: f64) -> bool {
        if self.mode != Mode::Page || self.keyboard.visible || self.modal() {
            return false;
        }
        let mut dirty = false;
        if self.left != [0.0; 2] {
            self.pointer[0] = (self.pointer[0] + self.left[0] as f64 * 420.0 * dt)
                .clamp(1.0, VIEW_WIDTH as f64 - 2.0);
            self.pointer[1] = (self.pointer[1] + self.left[1] as f64 * 420.0 * dt)
                .clamp(1.0, VIEW_HEIGHT as f64 - 2.0);
            self.pointer_mode = true;
            self.send(engine, Action::Pointer(self.pointer[0], self.pointer[1]));
            dirty = true;
        }
        if self.right != [0.0; 2] {
            self.send(
                engine,
                Action::Scroll(
                    self.pointer[0],
                    self.pointer[1],
                    self.right[0] as f64 * 650.0 * dt,
                    self.right[1] as f64 * 650.0 * dt,
                ),
            );
            dirty = true;
        }
        if let Some(request) = self.pending_focus {
            if self.info.focus_serial == request.serial {
                self.pending_focus = None;
                if self.info.editable {
                    self.open_keyboard(Entry::Field);
                    self.keyboard_trigger = request.trigger_held.then_some(Button::A);
                    dirty = true;
                } else if request.activate {
                    self.send(engine, Action::Key("Enter".into(), false));
                    dirty = true;
                }
            } else if request.since.elapsed() > Duration::from_secs(2) {
                self.pending_focus = None;
            }
        }
        dirty
    }
    fn draw(&self, s: &mut Screen, frame: Option<&sdl2::render::Texture>) -> Result<(), String> {
        let t = s.theme;
        s.clear(Some(t.bg));
        s.draw_display_text("BROWSER", 18, 14, t.text, 36);
        let status = if self.info.loading {
            "LOADING".to_string()
        } else if let Some(code) = self.info.http_status.filter(|s| *s >= 400) {
            format!("HTTP {code}")
        } else {
            "WEB".to_string()
        };
        let status = if let Some(d) = self.info.download.as_ref().filter(|d| d.active) {
            format!("SAVE {} KiB", d.received / 1024)
        } else {
            format!(
                "{}% {status} {}/2",
                (self.zoom * 100.0).round() as u32,
                self.info.tabs.len().max(1)
            )
        };
        s.draw_text(&status, 546, 25, Some(t.accent), 13, true, Some(160));
        s.fill(Rect::new(18, 62, 684, 30), t.card_bg);
        s.draw_text(
            if self.info.url.starts_with("http") {
                &self.info.url
            } else {
                "Address or search  /  X"
            },
            28,
            69,
            Some(t.text),
            14,
            false,
            Some(660),
        );
        s.fill(Rect::new(0, 96, 720, 4), t.accent);
        match self.mode {
            Mode::Page => {
                s.fill(
                    Rect::new(0, VIEW_Y, VIEW_WIDTH, VIEW_HEIGHT),
                    Color::RGB(245, 244, 240),
                );
                if let Some(texture) = frame {
                    s.canvas
                        .copy(texture, None, Rect::new(0, VIEW_Y, VIEW_WIDTH, VIEW_HEIGHT))?;
                } else {
                    s.draw_text(
                        "Opening the page...",
                        28,
                        VIEW_Y + 36,
                        Some(Color::RGB(40, 40, 40)),
                        20,
                        true,
                        None,
                    );
                }
                if self.pointer_mode {
                    let x = self.pointer[0] as i32;
                    let y = VIEW_Y + self.pointer[1] as i32;
                    s.fill(Rect::new(x - 8, y - 2, 19, 5), Color::BLACK);
                    s.fill(Rect::new(x - 2, y - 8, 5, 19), Color::BLACK);
                    s.fill(Rect::new(x - 7, y - 1, 17, 3), t.accent);
                    s.fill(Rect::new(x - 1, y - 7, 3, 17), t.accent);
                }
            }
            Mode::Home => {
                s.draw_display_text("THE WEB. IN YOUR HANDS.", 24, 126, t.text, 34);
                s.draw_text(
                    "Open a site, follow a link, or search for something.",
                    24,
                    173,
                    Some(t.text_dim),
                    15,
                    false,
                    Some(670),
                );
                for (i, (name, url)) in QUICK
                    .iter()
                    .chain(std::iter::once(&(
                        "ADDRESS / SEARCH",
                        "Enter a web address or any search terms",
                    )))
                    .enumerate()
                {
                    let y = 222 + i as i32 * 75;
                    let active = i == self.selected;
                    s.fill(
                        Rect::new(18, y, 684, 64),
                        if active { t.accent } else { t.card_bg },
                    );
                    let c = if active { t.bg } else { t.text };
                    s.draw_display_text(name, 32, y + 6, c, 28);
                    s.draw_text(
                        url,
                        34,
                        y + 40,
                        Some(if active { t.bg } else { t.text_dim }),
                        13,
                        false,
                        Some(645),
                    );
                }
            }
            Mode::Menu => {
                s.draw_display_text("PAGE TOOLS", 24, 124, t.text, 34);
                let offset = self.selected.saturating_sub(7);
                for (i, title) in MENU.iter().enumerate().skip(offset).take(8) {
                    let y = 178 + (i - offset) as i32 * 59;
                    let active = i == self.selected;
                    s.fill(
                        Rect::new(18, y, 684, 54),
                        if active { t.accent } else { t.card_bg },
                    );
                    s.draw_text(
                        title,
                        32,
                        y + 16,
                        Some(if active { t.bg } else { t.text }),
                        19,
                        active,
                        Some(650),
                    );
                }
            }
            Mode::Tabs => {
                s.draw_display_text("TABS / 2 MAXIMUM", 24, 124, t.text, 34);
                s.draw_text(
                    "Background tabs are frozen. X closes the selected tab.",
                    24,
                    170,
                    Some(t.text_dim),
                    15,
                    false,
                    Some(670),
                );
                for (i, tab) in self.info.tabs.iter().enumerate() {
                    row(
                        s,
                        i,
                        self.selected,
                        220 + 90 * i as i32,
                        &format!(
                            "{} {}",
                            if tab.active { "[ACTIVE]" } else { "[FROZEN]" },
                            if tab.title.is_empty() {
                                "New tab"
                            } else {
                                &tab.title
                            }
                        ),
                        &tab.url,
                    );
                }
                row(
                    s,
                    self.info.tabs.len(),
                    self.selected,
                    440,
                    "New tab",
                    "Close a tab first when both slots are occupied.",
                );
            }
            Mode::Permissions => {
                s.draw_display_text("SITE PERMISSIONS", 24, 124, t.text, 34);
                wrapped(s, &self.info.url, 24, 174, 64, 2, 14, t.text_dim);
                for (i, p) in PERMISSIONS.iter().enumerate() {
                    row(
                        s,
                        i,
                        self.selected,
                        242 + 74 * i as i32,
                        p.label(),
                        "Ask to allow for this visit",
                    );
                }
                row(
                    s,
                    4,
                    self.selected,
                    538,
                    "Block all permissions",
                    "Permissions reset on navigation, tab switch and exit.",
                );
                s.draw_text(
                    "Allow here, then retry the website's request.",
                    24,
                    628,
                    Some(t.text_dim),
                    14,
                    false,
                    Some(660),
                );
            }
            Mode::Downloads => {
                s.draw_display_text("DOWNLOADS / FILES", 24, 124, t.text, 34);
                let folder = self
                    .library_path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join("profile/files");
                wrapped(
                    s,
                    &format!("Files folder: {}", folder.display()),
                    24,
                    180,
                    64,
                    3,
                    15,
                    t.text_dim,
                );
                s.draw_text(
                    "32 MiB per transfer / 64 MiB download quota / 128 files",
                    24,
                    270,
                    Some(t.text),
                    15,
                    false,
                    Some(670),
                );
                if let Some(d) = &self.info.download {
                    wrapped(s, &d.name, 24, 322, 62, 2, 18, t.text);
                    wrapped(s, &d.origin, 24, 380, 64, 3, 14, t.accent);
                    s.draw_text(
                        &format!(
                            "{} KiB / {}",
                            d.received / 1024,
                            d.total
                                .map(|n| format!("{} KiB", n / 1024))
                                .unwrap_or_else(|| "unknown size".into())
                        ),
                        24,
                        470,
                        Some(t.text),
                        17,
                        true,
                        Some(670),
                    );
                    wrapped(s, &d.status, 24, 510, 64, 3, 16, t.text);
                    if d.active {
                        s.draw_text("X Cancel download", 24, 612, Some(t.accent), 17, true, None);
                    }
                } else {
                    s.draw_text(
                        "No downloads this session.",
                        24,
                        340,
                        Some(t.text),
                        18,
                        false,
                        None,
                    );
                }
            }
            Mode::Library => {
                s.draw_display_text(
                    if self.history { "HISTORY" } else { "BOOKMARKS" },
                    24,
                    124,
                    t.text,
                    34,
                );
                s.draw_text(
                    "L1 / R1  Switch view",
                    24,
                    168,
                    Some(t.text_dim),
                    14,
                    false,
                    None,
                );
                if self.entries().is_empty() {
                    s.draw_text(
                        "Nothing saved yet. Use Start > Save bookmark on a page.",
                        24,
                        230,
                        Some(t.text),
                        16,
                        false,
                        Some(664),
                    );
                }
                let offset = self.selected.saturating_sub(5);
                for (i, b) in self.entries().iter().enumerate().skip(offset).take(6) {
                    let y = 210 + (i - offset) as i32 * 74;
                    let active = i == self.selected;
                    s.fill(
                        Rect::new(18, y, 684, 64),
                        if active { t.accent } else { t.card_bg },
                    );
                    let c = if active { t.bg } else { t.text };
                    s.draw_text(
                        if b.title.is_empty() { &b.url } else { &b.title },
                        30,
                        y + 8,
                        Some(c),
                        17,
                        true,
                        Some(654),
                    );
                    s.draw_text(&b.url, 30, y + 38, Some(c), 12, false, Some(654));
                }
            }
        }
        s.fill(Rect::new(0, 676, 720, 44), t.bg);
        let hints = match self.mode {
            Mode::Page => "A Open / click  B Back  X Address  Y Saved  Start Tools",
            Mode::Home => "D-pad Choose   A Open   X Address   Y Saved   Select Exit",
            Mode::Menu => "D-pad Choose   A Select   B Back   Select Exit",
            Mode::Library => "A Open  B Back  L1/R1 View  X Remove  Select Exit",
            Mode::Tabs => "A Switch / new   X Close   B Back   Select Exit",
            Mode::Permissions => "A Review permission   B Back   Select Exit",
            Mode::Downloads => "X Cancel transfer   B Back   Select Exit",
        };
        if self.mode == Mode::Page {
            s.draw_text(
                "A Open/click   B Back   L2/R2 Page zoom   Select Exit",
                18,
                683,
                Some(t.text),
                13,
                false,
                Some(690),
            );
            s.draw_text(
                "Left stick Point  Right stick Scroll  X Address  Y Saved  Start Tools",
                18,
                703,
                Some(t.text_dim),
                12,
                false,
                Some(690),
            );
        } else {
            s.draw_text(hints, 18, 692, Some(t.text), 13, false, Some(690));
        }
        if self.modal() {
            s.fill(Rect::new(12, 150, 696, 516), t.bg);
            s.draw_outline(Rect::new(12, 150, 696, 516), t.accent, 2);
            let (title, origin) = if let Some(p) = &self.info.javascript_dialog {
                ("WEBSITE DIALOG", p.origin.as_str())
            } else if let Some(p) = &self.info.permission {
                ("SITE PERMISSION", p.origin.as_str())
            } else if let Some(p) = &self.info.upload {
                ("UPLOAD FROM FILES", p.origin.as_str())
            } else {
                (
                    "DOWNLOAD",
                    self.info.download.as_ref().unwrap().origin.as_str(),
                )
            };
            s.draw_display_text(title, 26, 168, t.text, 28);
            wrapped(s, origin, 26, 210, 62, 5, 14, t.accent);
            if let Some(p) = &self.info.javascript_dialog {
                wrapped(s, &p.message, 26, 340, 60, 8, 17, t.text);
                s.draw_text(
                    if p.kind == "prompt" {
                        "A Enter text    B Cancel"
                    } else {
                        "A Accept    B Dismiss"
                    },
                    26,
                    628,
                    Some(t.accent),
                    16,
                    true,
                    Some(660),
                );
            } else if let Some(p) = &self.info.permission {
                s.draw_text(
                    &format!("Allow {} for this visit?", p.permission.label()),
                    26,
                    350,
                    Some(t.text),
                    20,
                    true,
                    Some(660),
                );
                wrapped(
                    s,
                    "Only this origin. Other permissions stay blocked. Navigation or changing tabs revokes approval.",
                    26,
                    404,
                    58,
                    5,
                    17,
                    t.text_dim,
                );
                s.draw_text(
                    if self.modal_allow {
                        "Left Deny   [Right Allow]   A Choose   B Deny"
                    } else {
                        "[Left Deny]   Right Allow   A Choose   B Deny"
                    },
                    26,
                    628,
                    Some(t.accent),
                    15,
                    true,
                    Some(660),
                );
            } else if let Some(p) = &self.info.upload {
                let offset = self.modal_selected.saturating_sub(4);
                for i in offset..(p.files.len() + 1).min(offset + 5) {
                    let text = if i == 0 {
                        "Cancel upload".into()
                    } else {
                        let f = &p.files[i - 1];
                        format!(
                            "{} {} ({} KiB)",
                            if self.upload_selected.contains(&f.name) {
                                "[x]"
                            } else {
                                "[ ]"
                            },
                            f.name,
                            f.bytes.div_ceil(1024)
                        )
                    };
                    let y = 346 + (i - offset) as i32 * 47;
                    s.fill(
                        Rect::new(24, y, 672, 43),
                        if self.modal_selected == i {
                            t.accent
                        } else {
                            t.card_bg
                        },
                    );
                    s.draw_text(
                        &text,
                        32,
                        y + 12,
                        Some(if self.modal_selected == i {
                            t.bg
                        } else {
                            t.text
                        }),
                        16,
                        false,
                        Some(650),
                    );
                }
                if p.files.is_empty() {
                    s.draw_text(
                        "Add regular files to profile/files, then reopen picker.",
                        26,
                        440,
                        Some(t.text_dim),
                        15,
                        false,
                        Some(656),
                    );
                }
                s.draw_text(
                    if p.multiple {
                        "A Mark   Start Upload marked (max 8)   B Cancel"
                    } else {
                        "A Upload selected file   B Cancel"
                    },
                    26,
                    628,
                    Some(t.accent),
                    15,
                    true,
                    Some(660),
                );
            } else if let Some(d) = &self.info.download {
                wrapped(s, &d.name, 26, 352, 58, 3, 20, t.text);
                wrapped(s, &d.status, 26, 444, 60, 4, 17, t.text_dim);
                s.draw_text(
                    if self.modal_allow {
                        "Left Cancel   [Right Save]   A Choose   B Cancel"
                    } else {
                        "[Left Cancel]   Right Save   A Choose   B Cancel"
                    },
                    26,
                    628,
                    Some(t.accent),
                    15,
                    true,
                    Some(660),
                );
            }
        } else if let Some(text) = self
            .message
            .as_ref()
            .or(self.info.notice.as_ref())
            .or(self.info.error.as_ref())
        {
            s.fill(Rect::new(12, 460, 696, 202), t.bg);
            s.draw_outline(Rect::new(12, 460, 696, 202), t.accent, 2);
            wrapped(s, text, 26, 478, 65, 5, 15, t.text);
            s.draw_text("B Dismiss", 26, 636, Some(t.accent), 14, true, None);
        }
        if self.modal() {
            if let Some(error) = self.message.as_ref().or(self.info.error.as_ref()) {
                s.draw_text(error, 26, 597, Some(t.accent), 13, false, Some(660));
            }
        }
        self.keyboard.draw(s);
        Ok(())
    }
}

fn wrapped(
    s: &mut Screen,
    text: &str,
    x: i32,
    y: i32,
    width: usize,
    lines: usize,
    size: u16,
    color: Color,
) {
    let chars = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<Vec<_>>();
    for (i, line) in chars.chunks(width).take(lines).enumerate() {
        s.draw_text(
            &line.iter().collect::<String>(),
            x,
            y + i as i32 * (size as i32 + 8),
            Some(color),
            size,
            false,
            Some(666),
        );
    }
}
fn row(s: &mut Screen, index: usize, selected: usize, y: i32, title: &str, detail: &str) {
    let t = s.theme;
    let active = index == selected;
    let c = if active { t.bg } else { t.text };
    s.fill(
        Rect::new(18, y, 684, 64),
        if active { t.accent } else { t.card_bg },
    );
    s.draw_text(title, 30, y + 8, Some(c), 18, true, Some(654));
    s.draw_text(detail, 30, y + 39, Some(c), 13, false, Some(654));
}

pub fn run(assets: &Path, options: Options) -> Result<(), String> {
    let executable = discover(options.engine);
    let data = options
        .profile
        .unwrap_or_else(|| cartridge_core::paths::cartridges_dir().join("dev.cartridge.browser"));
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let engine = match executable {
        Ok(executable) => {
            BrowserEngine::start(executable, data.join("profile"), options.initial.clone())
        }
        Err(error) => BrowserEngine::unavailable(error),
    };
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let mut canvas = cartridge_core::window::create_canvas(
        &video,
        "CartridgeOS / Browser",
        cartridge_core::window::WindowOptions {
            hidden: options.hidden,
            ..Default::default()
        },
    )?;
    let creator = canvas.texture_creator();
    let mut fonts = FontCache::new(assets)?;
    let theme = Theme::user_selected();
    fonts.set_family(theme.font_regular, theme.font_bold);
    fonts.set_display(theme.font_display);
    fonts.prewarm();
    let mut images = ImageCache::new(&creator)?;
    let mut text_cache = TextCache::new(&creator);
    let controllers = sdl.game_controller()?;
    input::load_controller_mappings(&controllers);
    let _controllers = input::open_all_controllers(&controllers);
    let joystick = sdl.joystick()?;
    let _joysticks = input::open_all_joysticks(&joystick);
    let mut input = InputManager::new();
    input.set_ignore_joystick(!_controllers.is_empty());
    input.set_stick_dpad_enabled(false);
    let mut sticks = StickManager::new(
        _controllers.iter().map(|c| c.instance_id()),
        cartridge_core::sim::is_sim(),
    );
    video.text_input().start();
    let mut pump = sdl.event_pump()?;
    let mut inbox = cartridge_core::event_wait::EventInbox::default();
    let mut ui = Ui::new(&data);
    if let Some(url) = &options.initial {
        ui.apply_site_zoom(&engine, url);
    } else {
        ui.send(&engine, Action::Zoom(ui.zoom));
    }
    if options.initial.is_some() {
        ui.mode = Mode::Page;
    }
    let mut texture = creator
        .create_texture_streaming(PixelFormatEnum::RGB24, VIEW_WIDTH, VIEW_HEIGHT)
        .map_err(|e| e.to_string())?;
    let mut has_frame = false;
    let mut revision = u64::MAX;
    let mut dirty = true;
    let mut captured = false;
    let mut pending_capture = false;
    let start = Instant::now();
    let mut last = Instant::now();
    let mut last_history = String::new();
    let mut last_zoom_url = String::new();

    let mut page_visible = true;
    let mut last_active = Instant::now();
    let mut frame_rate = 24;
    'running: loop {
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f64().min(0.05);
        last = now;
        let events = inbox.collect(&mut pump).to_vec();
        pending_capture |= cartridge_core::screenshot::requested(&events);
        if !events.is_empty() {
            last_active = Instant::now();
            dirty = true;
        }
        for event in &events {
            match event {
                Event::Quit { .. } => break 'running,
                Event::Window {
                    win_event: sdl2::event::WindowEvent::FocusLost,
                    ..
                } => {
                    if ui.dragging {
                        ui.dragging = false;
                        ui.send(&engine, Action::Button(ui.pointer[0], ui.pointer[1], false));
                    }
                    ui.left = [0.0; 2];
                    ui.right = [0.0; 2];
                    sticks.capture();
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    repeat: false,
                    ..
                } => {
                    if ui.keyboard.visible {
                        ui.cancel_keyboard(&engine);
                    } else if ui.modal() {
                        ui.press_modal(&engine, Button::B);
                    } else if ui.mode != Mode::Page {
                        ui.mode = Mode::Page;
                    } else {
                        break 'running;
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::F12),
                    repeat: false,
                    ..
                } => pending_capture = true,
                Event::KeyDown {
                    keycode: Some(Keycode::F2),
                    repeat: false,
                    ..
                } if !ui.modal() => ui.open_keyboard(Entry::Address),
                Event::TextInput { text, .. } if ui.keyboard.visible => {
                    let remaining = ui
                        .keyboard
                        .max_len
                        .saturating_sub(ui.keyboard.text.chars().count());
                    let value = text.chars().take(remaining).collect::<String>();
                    let mut cursor = ui.keyboard.cursor_pos.min(ui.keyboard.text.len());
                    while !ui.keyboard.text.is_char_boundary(cursor) {
                        cursor -= 1;
                    }
                    ui.keyboard.text.insert_str(cursor, &value);
                    ui.keyboard.cursor_pos = cursor + value.len();
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Backspace),
                    ..
                } if ui.keyboard.visible => {
                    let result = ui.keyboard.handle_input(&input::InputEvent {
                        button: Button::B,
                        action: InputAction::Press,
                    });
                    if result == TextInputResult::Cancelled {
                        ui.cancel_keyboard(&engine);
                    }
                }
                Event::MouseMotion { x, y, .. }
                    if ui.mode == Mode::Page && !ui.keyboard.visible && !ui.modal() =>
                {
                    if *y >= VIEW_Y && *y < VIEW_Y + VIEW_HEIGHT as i32 {
                        ui.pointer = [*x as f64, (*y - VIEW_Y) as f64];
                        ui.pointer_mode = true;
                        ui.send(&engine, Action::Pointer(ui.pointer[0], ui.pointer[1]));
                    }
                }
                Event::MouseButtonDown {
                    x,
                    y,
                    mouse_btn: sdl2::mouse::MouseButton::Left,
                    ..
                } if ui.mode == Mode::Page && !ui.keyboard.visible && !ui.modal() => {
                    if *y >= VIEW_Y && *y < VIEW_Y + VIEW_HEIGHT as i32 {
                        ui.pointer = [*x as f64, (*y - VIEW_Y) as f64];
                        ui.dragging = true;
                        ui.send(&engine, Action::Button(ui.pointer[0], ui.pointer[1], true));
                    }
                }
                Event::MouseButtonUp {
                    mouse_btn: sdl2::mouse::MouseButton::Left,
                    ..
                } if ui.dragging => {
                    ui.dragging = false;
                    ui.send(&engine, Action::Button(ui.pointer[0], ui.pointer[1], false));
                    ui.request_focus(&engine, false);
                }
                Event::MouseWheel { x, y, .. }
                    if ui.mode == Mode::Page && !ui.keyboard.visible && !ui.modal() =>
                {
                    ui.send(
                        &engine,
                        Action::Scroll(
                            ui.pointer[0],
                            ui.pointer[1],
                            -*x as f64 * 80.0,
                            -*y as f64 * 80.0,
                        ),
                    )
                }
                _ => {}
            }
        }
        let filtered=events.iter().filter(|e|!ui.keyboard.visible || !matches!(e,Event::KeyDown{keycode:Some(k),..} if !matches!(*k,Keycode::Up|Keycode::Down|Keycode::Left|Keycode::Right|Keycode::Return))).cloned().collect::<Vec<_>>();
        for event in input.process_events(&filtered) {
            if ui.press(&engine, event.button, event.action) {
                break 'running;
            }
        }
        if !ui.keyboard.visible {
            sticks.process_events(&events);
            for change in sticks.take_changes() {
                match change.stick {
                    Stick::Left => ui.left = [change.x, change.y],
                    Stick::Right => ui.right = [change.x, change.y],
                }
            }
        } else {
            sticks.capture();
            ui.left = [0.0; 2];
            ui.right = [0.0; 2];
        }
        dirty |= ui.update(&engine, dt);
        let fps = if ui.left != [0.0; 2]
            || ui.right != [0.0; 2]
            || last_active.elapsed() < Duration::from_secs(2)
        {
            24
        } else {
            8
        };
        if fps != frame_rate {
            ui.send(&engine, Action::FrameRate(fps));
            frame_rate = fps;
        }
        let mut navigation_finished = false;
        {
            let mut state = engine.shared.lock().unwrap();
            if state.revision != revision {
                revision = state.revision;
                if ui.info.tabs.iter().find(|t| t.active).map(|t| &t.id)
                    != state.info.tabs.iter().find(|t| t.active).map(|t| &t.id)
                {
                    has_frame = false;
                    ui.zoom_site = None;
                }
                if ui.info.url != state.info.url {
                    ui.pending_focus = None;
                }
                navigation_finished = ui.info.loading && !state.info.loading;
                ui.info = state.info.clone();
                dirty = true;
            }
            if let Some(frame) = state.frame.take() {
                texture
                    .update(None, &frame.rgb, frame.width as usize * 3)
                    .map_err(|e| e.to_string())?;
                has_frame = true;
                dirty = true;
            }
        }
        ui.sync_modal();
        if ui.info.url != last_zoom_url || navigation_finished || ui.zoom_site.is_none() {
            last_zoom_url = ui.info.url.clone();
            ui.apply_site_zoom(&engine, &last_zoom_url);
        }

        let visible = ui.mode == Mode::Page && !ui.keyboard.visible && !ui.modal();
        if visible != page_visible {
            ui.send(&engine, Action::Visible(visible));
            page_visible = visible;
        }
        if !ui.info.loading
            && ui.info.url.starts_with("http")
            && ui.info.url != last_history
            && !ui.info.title.is_empty()
        {
            last_history = ui.info.url.clone();
            ui.library.remember(&ui.info.title, &ui.info.url);
            ui.library_dirty = true;
        }
        match ui.save_pending(Instant::now(), false) {
            Ok(changed) => dirty |= changed,
            Err(_) => dirty = true,
        }
        if options.capture.is_some()
            && !captured
            && (has_frame || ui.mode != Mode::Page)
            // A diagnostic capture records the visible UI at the requested
            // time. Background requests may keep a usable page loading.
            && start.elapsed().as_secs_f64() >= options.capture_after.unwrap_or(2.0)
        {
            pending_capture = true;
        }
        if ui.dragging && (ui.mode != Mode::Page || ui.keyboard.visible) {
            ui.dragging = false;
            ui.send(&engine, Action::Button(ui.pointer[0], ui.pointer[1], false));
        }
        if dirty || pending_capture {
            {
                let mut screen = Screen {
                    canvas: &mut canvas,
                    theme: &theme,
                    fonts: &mut fonts,
                    images: &mut images,
                    text_cache: &mut text_cache,
                    texture_creator: &creator,
                };
                ui.draw(&mut screen, if has_frame { Some(&texture) } else { None })?;
            }
            if pending_capture
                || (!captured
                    && options.capture.is_some()
                    && (has_frame || ui.mode != Mode::Page)
                    && start.elapsed().as_secs_f64() >= options.capture_after.unwrap_or(2.0))
            {
                let path = options
                    .capture
                    .clone()
                    .unwrap_or_else(|| data.join("browser.png"));
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                cartridge_core::screenshot::capture_frame_to_png(&canvas, &path)?;
                println!("Browser screenshot: {}", path.display());
                captured = true;
                pending_capture = false;
            }
            canvas.present();
            dirty = false;
        }
        if options
            .seconds
            .is_some_and(|s| start.elapsed().as_secs_f64() >= s)
        {
            break;
        }
        let active = ui.left != [0.0; 2] || ui.right != [0.0; 2];
        // No redraw on an unchanged page. Poll only the bounded engine state;
        // SDL interrupts this wait immediately for new controller input.
        inbox.wait(
            &mut pump,
            if active {
                Duration::from_millis(25)
            } else {
                Duration::from_millis(80)
            },
        );
    }
    let save_result = ui.save_pending(Instant::now(), true);
    engine.begin_shutdown();
    // Chromium may need several seconds to flush its profile on SD storage.
    // Keep SDL pumping and paint a native closing panel throughout that wait.
    while !engine.shutdown_finished() {
        let _ = inbox.collect(&mut pump);
        {
            let mut screen = Screen {
                canvas: &mut canvas,
                theme: &theme,
                fonts: &mut fonts,
                images: &mut images,
                text_cache: &mut text_cache,
                texture_creator: &creator,
            };
            screen.clear(Some(theme.bg));
            screen.draw_display_text("CLOSING BROWSER", 24, 240, theme.text, 36);
            screen.draw_text(
                "Saving website data. Please wait...",
                24,
                310,
                Some(theme.text_dim),
                18,
                false,
                Some(670),
            );
        }
        canvas.present();
        inbox.wait(&mut pump, Duration::from_millis(80));
    }
    drop(engine);
    save_result?;
    if options.capture.is_some() && !captured {
        return Err(match ui.info.error.as_ref() {
            Some(error) => format!("No webpage capture: {error}"),
            None => "No rendered webpage was available for capture.".into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ui() -> Ui {
        Ui::new(&std::env::temp_dir().join(format!("browser-input-test-{}", std::process::id())))
    }
    #[test]
    fn failed_library_saves_stay_dirty_and_retry_with_capped_backoff() {
        let root = std::env::temp_dir().join(format!("browser-save-retry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let blocker = root.join("blocked");
        std::fs::write(&blocker, "not a directory").unwrap();
        let mut ui = Ui::new(&root);
        ui.library_path = blocker.join("library.json");
        ui.library_dirty = true;
        let now = Instant::now();
        assert!(ui.save_pending(now, true).is_err());
        assert!(ui.library_dirty);
        assert!(!ui.save_pending(now, false).unwrap());
        for _ in 0..10 {
            ui.save_retry.failed(now);
        }
        assert_eq!(ui.save_retry.delay, Duration::from_secs(60));
        std::fs::remove_file(&blocker).unwrap();
        std::fs::create_dir(&blocker).unwrap();
        ui.library.remember("saved", "https://example.org/");
        assert!(ui.save_pending(now, true).unwrap());
        assert!(!ui.library_dirty);
        assert_eq!(Library::load(&ui.library_path).history.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "visual QA: run with SDL_VIDEODRIVER=dummy and --ignored --test-threads=1"]
    fn render_native_feature_panels_at_720_square() {
        assert_eq!(
            std::env::var("SDL_VIDEODRIVER").as_deref(),
            Ok("dummy"),
            "Run visual QA with SDL_VIDEODRIVER=dummy; Cocoa requires the application main thread"
        );
        let sdl = sdl2::init().unwrap();
        let video = sdl.video().unwrap();
        let mut canvas = cartridge_core::window::create_canvas(
            &video,
            "Browser UI fixtures",
            cartridge_core::window::WindowOptions {
                hidden: true,
                software: true,
            },
        )
        .unwrap();
        let creator = canvas.texture_creator();
        let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut fonts = FontCache::new(&assets).unwrap();
        let mut images = ImageCache::new(&creator).unwrap();
        let mut cache = TextCache::new(&creator);
        let theme = Theme::user_selected();
        let out = std::env::var_os("CARTRIDGE_BROWSER_UI_OUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("browser-ui-feature-captures"));
        std::fs::create_dir_all(&out).unwrap();
        let mut ui = ui();
        ui.info.url = "https://example.org/account".into();
        ui.info.title = "Example real webpage".into();
        ui.info.tabs = vec![
            crate::engine::TabInfo {
                id: "1".into(),
                title: "Example real webpage".into(),
                url: ui.info.url.clone(),
                active: true,
            },
            crate::engine::TabInfo {
                id: "2".into(),
                title: "Another site".into(),
                url: "https://other.example.org/".into(),
                active: false,
            },
        ];
        for (name, mode) in [
            ("tools", Mode::Menu),
            ("tabs", Mode::Tabs),
            ("permissions", Mode::Permissions),
            ("downloads", Mode::Downloads),
        ] {
            ui.mode = mode;
            let mut screen = Screen {
                canvas: &mut canvas,
                theme: &theme,
                fonts: &mut fonts,
                images: &mut images,
                text_cache: &mut cache,
                texture_creator: &creator,
            };
            ui.draw(&mut screen, None).unwrap();
            cartridge_core::screenshot::capture_frame_to_png(
                &canvas,
                &out.join(format!("{name}.png")),
            )
            .unwrap();
        }
        ui.info.permission = Some(crate::engine::PermissionPrompt {
            id: 1,
            origin: "https://example.org:8443".into(),
            permission: Permission::Camera,
        });
        ui.mode = Mode::Page;
        ui.sync_modal();
        {
            let mut screen = Screen {
                canvas: &mut canvas,
                theme: &theme,
                fonts: &mut fonts,
                images: &mut images,
                text_cache: &mut cache,
                texture_creator: &creator,
            };
            ui.draw(&mut screen, None).unwrap();
        }
        cartridge_core::screenshot::capture_frame_to_png(
            &canvas,
            &out.join("permission-prompt.png"),
        )
        .unwrap();
        ui.info.permission = None;
        ui.info.upload = Some(crate::engine::UploadPrompt {
            id: 2,
            origin: "https://upload.example.org".into(),
            files: vec![
                crate::engine::FileChoice {
                    name: "holiday-photo.jpg".into(),
                    bytes: 12345,
                },
                crate::engine::FileChoice {
                    name: "report.txt".into(),
                    bytes: 128,
                },
            ],
            multiple: true,
        });
        ui.sync_modal();
        {
            let mut screen = Screen {
                canvas: &mut canvas,
                theme: &theme,
                fonts: &mut fonts,
                images: &mut images,
                text_cache: &mut cache,
                texture_creator: &creator,
            };
            ui.draw(&mut screen, None).unwrap();
        }
        cartridge_core::screenshot::capture_frame_to_png(&canvas, &out.join("upload-picker.png"))
            .unwrap();
        println!("Native UI captures: {}", out.display());
    }
    #[test]
    fn permission_and_download_prompts_start_on_deny_and_reset_for_each_request() {
        let mut ui = ui();
        ui.info.permission = Some(crate::engine::PermissionPrompt {
            id: 1,
            origin: "https://example.org".into(),
            permission: Permission::Location,
        });
        ui.sync_modal();
        assert!(!ui.modal_allow);
        assert!(ui.modal());
        ui.modal_allow = true;
        ui.info.permission.as_mut().unwrap().id = 2;
        ui.sync_modal();
        assert!(!ui.modal_allow);
        ui.info.permission = None;
        ui.info.download = Some(crate::engine::DownloadInfo {
            id: 3,
            origin: "https://example.org".into(),
            name: "file.txt".into(),
            received: 0,
            total: None,
            status: "Save?".into(),
            pending: true,
            active: false,
        });
        ui.sync_modal();
        assert!(!ui.modal_allow);
        assert_eq!(ui.modal_selected, 0);
    }
    #[test]
    fn prompt_keyboard_prefills_unicode_and_cannot_outlive_its_dialog() {
        let engine = BrowserEngine::unavailable("control test".into());
        let mut ui = ui();
        ui.info.javascript_dialog = Some(crate::engine::JavascriptDialog {
            id: 9,
            origin: "https://example.org".into(),
            kind: "prompt".into(),
            message: "Text?".into(),
            default_text: "日本語".into(),
        });
        ui.sync_modal();
        ui.press_modal(&engine, Button::A);
        assert!(ui.keyboard.visible);
        assert_eq!(ui.keyboard.text, "日本語");
        assert_eq!(ui.keyboard_trigger, Some(Button::A));
        ui.info.javascript_dialog = None;
        ui.sync_modal();
        assert!(!ui.keyboard.visible);
    }
    #[test]
    fn modal_blocks_page_stick_input_and_upload_starts_on_cancel() {
        let engine = BrowserEngine::unavailable("control test".into());
        let mut ui = ui();
        ui.mode = Mode::Page;
        ui.info.upload = Some(crate::engine::UploadPrompt {
            id: 1,
            origin: "https://example.org".into(),
            files: vec![],
            multiple: true,
        });
        ui.left = [1.0, 0.0];
        let pointer = ui.pointer;
        ui.sync_modal();
        assert_eq!(ui.modal_selected, 0);
        assert!(!ui.update(&engine, 0.1));
        assert_eq!(ui.pointer, pointer);
    }
    #[test]
    fn site_zoom_survives_navigation_and_held_shoulders_do_not_repeat() {
        let engine = BrowserEngine::unavailable("control test".into());
        let mut ui = ui();
        ui.library.remember_zoom("https://a.example/", 130);
        ui.library.remember_zoom("https://b.example/", 90);
        ui.navigate(&engine, "https://a.example/article");
        assert_eq!(ui.zoom, 1.3);
        ui.press(&engine, Button::R2, InputAction::Press);
        ui.press(&engine, Button::R2, InputAction::Repeat);
        assert_eq!(ui.zoom, 1.4);
        assert_eq!(ui.library.zoom_for("https://a.example/next"), 140);
        ui.navigate(&engine, "https://b.example/next");
        assert_eq!(ui.zoom, 0.9);
        // Back/redirect metadata selects the destination's preference.
        ui.apply_site_zoom(&engine, "https://a.example/article");
        assert_eq!(ui.zoom, 1.4);
        assert!(ui.library_dirty);
    }
    #[test]
    fn opening_keyboard_does_not_type_until_trigger_is_released() {
        let engine = BrowserEngine::unavailable("not needed by this control test".into());
        let mut ui = ui();
        ui.mode = Mode::Page;
        ui.info.editable = true;
        ui.press(&engine, Button::A, InputAction::Press);
        ui.info.focus_serial = ui.focus_serial;
        ui.update(&engine, 0.0);
        assert!(ui.keyboard.visible);
        ui.press(&engine, Button::A, InputAction::Repeat);
        assert!(ui.keyboard.text.is_empty());
        ui.press(&engine, Button::A, InputAction::Release);
        ui.press(&engine, Button::A, InputAction::Press);
        assert_eq!(ui.keyboard.text.chars().count(), 1);
        ui.press(&engine, Button::A, InputAction::Repeat);
        assert_eq!(ui.keyboard.text.chars().count(), 2);
    }
    #[test]
    fn stale_editable_state_does_not_open_keyboard_after_clicking_away() {
        let engine = BrowserEngine::unavailable("not needed by this control test".into());
        let mut ui = ui();
        ui.mode = Mode::Page;
        ui.info.editable = true;
        ui.request_focus(&engine, false);
        ui.update(&engine, 0.0);
        assert!(!ui.keyboard.visible);
        ui.info.focus_serial = ui.focus_serial;
        ui.info.editable = false;
        ui.update(&engine, 0.0);
        assert!(!ui.keyboard.visible);
        assert!(ui.pending_focus.is_none());
    }
    #[test]
    fn held_buttons_do_not_reopen_tools_or_flicker_library_tabs() {
        let engine = BrowserEngine::unavailable("not needed by this control test".into());
        let mut ui = ui();
        ui.mode = Mode::Page;
        ui.open_keyboard(Entry::Field);
        ui.keyboard.set_text("query");
        ui.press(&engine, Button::Start, InputAction::Press);
        assert!(!ui.keyboard.visible);
        ui.press(&engine, Button::Start, InputAction::Repeat);
        assert!(ui.mode == Mode::Page);
        ui.press(&engine, Button::Start, InputAction::Release);
        ui.press(&engine, Button::Start, InputAction::Press);
        assert!(ui.mode == Mode::Menu);
        ui.mode = Mode::Library;
        ui.history = false;
        ui.press(&engine, Button::R1, InputAction::Press);
        ui.press(&engine, Button::R1, InputAction::Repeat);
        assert!(ui.history);
    }
}
