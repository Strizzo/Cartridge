use crate::{
    engine::{Action, BrowserEngine, PageInfo, discover},
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
}
#[derive(Clone, Copy, PartialEq)]
enum Entry {
    Address,
    Field,
}
#[derive(Clone, Copy)]
struct FocusRequest {
    since: Instant,
    serial: u64,
    activate: bool,
    trigger_held: bool,
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
    left: [f32; 2],
    right: [f32; 2],
    pending_focus: Option<FocusRequest>,
    focus_serial: u64,
}
const QUICK: [(&str, &str); 4] = [
    ("SLASHDOT", "https://slashdot.org/"),
    ("CNN", "https://www.cnn.com/"),
    ("OPENSTREETMAP", "https://www.openstreetmap.org/"),
    ("SEARCH THE WEB", "https://duckduckgo.com/"),
];
const MENU: [&str; 8] = [
    "Reload page",
    "Type in focused field",
    "Press Enter / submit form",
    "Save / remove bookmark",
    "History",
    "Browser home",
    "Stop loading",
    "Exit browser",
];
impl Ui {
    fn new(root: &Path) -> Self {
        let library_path = root.join("library.json");
        Self {
            mode: Mode::Home,
            selected: 0,
            history: false,
            keyboard: TextInput::new("Address or search"),
            keyboard_trigger: None,
            entry: Entry::Address,
            library: Library::load(&library_path),
            library_path,
            info: PageInfo::default(),
            message: None,
            pointer: [360.0, 240.0],
            pointer_mode: false,
            dragging: false,
            zoom: f64::from(DEFAULT_ZOOM) / 100.0,
            zoom_site: None,
            library_dirty: false,
            left: [0.0; 2],
            right: [0.0; 2],
            pending_focus: None,
            focus_serial: 0,
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
        });
        self.keyboard.masked =
            matches!(entry, Entry::Field) && (self.info.password || !self.info.editable);
        self.keyboard.max_len = match entry {
            Entry::Address => 4096,
            Entry::Field => 2048,
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
        if self.info.dialog.is_some() {
            if action == InputAction::Press && (button == Button::A || button == Button::B) {
                self.send(engine, Action::Dialog(button == Button::A));
            }
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
                        if let Err(e) = self.library.save(&self.library_path) {
                            self.message = Some(e);
                        }
                        self.mode = Mode::Page;
                    }
                    4 => {
                        self.mode = Mode::Library;
                        self.history = true;
                        self.selected = 0;
                    }
                    5 => {
                        self.mode = Mode::Home;
                        self.selected = 0;
                    }
                    6 => {
                        self.send(engine, Action::Stop);
                        self.mode = Mode::Page;
                    }
                    _ => return true,
                },
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
                        if let Err(e) = self.library.save(&self.library_path) {
                            self.message = Some(e);
                        }
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
        if self.mode != Mode::Page || self.keyboard.visible {
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
        let status = format!("{}%  {status}", (self.zoom * 100.0).round() as u32);
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
                for (i, title) in MENU.iter().enumerate() {
                    let y = 178 + i as i32 * 59;
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
        if self.info.dialog.is_some() || self.info.error.is_some() || self.message.is_some() {
            s.fill(Rect::new(12, 460, 696, 202), t.bg);
            s.draw_outline(Rect::new(12, 460, 696, 202), t.accent, 2);
            let text = self
                .info
                .dialog
                .as_ref()
                .or(self.message.as_ref())
                .or(self.info.error.as_ref())
                .unwrap();
            for (i, line) in text.as_bytes().chunks(68).take(5).enumerate() {
                s.draw_text(
                    &String::from_utf8_lossy(line),
                    26,
                    478 + i as i32 * 26,
                    Some(t.text),
                    15,
                    false,
                    Some(662),
                );
            }
            if self.info.dialog.is_some() {
                s.draw_text(
                    "A Accept    B Dismiss",
                    26,
                    632,
                    Some(t.accent),
                    14,
                    true,
                    None,
                );
            }
        }
        self.keyboard.draw(s);
        Ok(())
    }
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
    let mut history_dirty = false;
    let mut history_saved = Instant::now();
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
                        ui.keyboard.visible = false;
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
                } => ui.open_keyboard(Entry::Address),
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
                    ui.keyboard.handle_input(&input::InputEvent {
                        button: Button::B,
                        action: InputAction::Press,
                    });
                }
                Event::MouseMotion { x, y, .. }
                    if ui.mode == Mode::Page && !ui.keyboard.visible =>
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
                } if ui.mode == Mode::Page && !ui.keyboard.visible => {
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
                Event::MouseWheel { x, y, .. } if ui.mode == Mode::Page && !ui.keyboard.visible => {
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
        if ui.info.url != last_zoom_url || navigation_finished {
            last_zoom_url = ui.info.url.clone();
            ui.apply_site_zoom(&engine, &last_zoom_url);
        }
        history_dirty |= std::mem::take(&mut ui.library_dirty);
        let visible = ui.mode == Mode::Page && !ui.keyboard.visible;
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
            history_dirty = true;
        }
        if history_dirty && history_saved.elapsed() > Duration::from_secs(2) {
            if let Err(e) = ui.library.save(&ui.library_path) {
                ui.message = Some(e);
            }
            history_dirty = false;
            history_saved = Instant::now();
        }
        if options.capture.is_some()
            && !captured
            && (has_frame || ui.mode != Mode::Page)
            && !ui.info.loading
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
                    && !ui.info.loading
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
    if options.capture.is_some() && !captured {
        return Err("No rendered webpage was available for capture.".into());
    }
    if history_dirty || ui.library_dirty {
        ui.library.save(&ui.library_path)?;
    }
    drop(engine);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ui() -> Ui {
        Ui::new(&std::env::temp_dir().join(format!("browser-input-test-{}", std::process::id())))
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
