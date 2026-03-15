use sdl2::controller::Button as SdlControllerButton;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use std::collections::HashMap;
use std::time::Instant;

/// Abstract hardware buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    A,
    B,
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    Start,
    Select,
}

/// Input action type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    Press,
    Release,
    Repeat,
}

/// A processed input event.
#[derive(Debug, Clone, Copy)]
pub struct InputEvent {
    pub button: Button,
    pub action: InputAction,
}

const REPEAT_DELAY: f64 = 0.4;
const REPEAT_INTERVAL: f64 = 0.08;

/// Translates raw SDL events into InputEvents with key repeat.
pub struct InputManager {
    keyboard_map: HashMap<Keycode, Button>,
    gamepad_map: HashMap<u8, Button>,
    held: HashMap<Button, Instant>,
    last_repeat: HashMap<Button, Instant>,
    /// When true, skip Joystick API events (because GameController API handles them).
    ignore_joystick: bool,
}

impl InputManager {
    pub fn new() -> Self {
        let mut keyboard_map = HashMap::new();
        keyboard_map.insert(Keycode::Up, Button::DpadUp);
        keyboard_map.insert(Keycode::Down, Button::DpadDown);
        keyboard_map.insert(Keycode::Left, Button::DpadLeft);
        keyboard_map.insert(Keycode::Right, Button::DpadRight);
        keyboard_map.insert(Keycode::Z, Button::A);
        keyboard_map.insert(Keycode::X, Button::B);
        keyboard_map.insert(Keycode::C, Button::X);
        keyboard_map.insert(Keycode::V, Button::Y);
        keyboard_map.insert(Keycode::A, Button::L1);
        keyboard_map.insert(Keycode::S, Button::R1);
        keyboard_map.insert(Keycode::Q, Button::L2);
        keyboard_map.insert(Keycode::W, Button::R2);
        keyboard_map.insert(Keycode::Return, Button::Start);
        keyboard_map.insert(Keycode::Space, Button::Select);

        // R36S Plus button mapping (Nintendo layout: right=A, bottom=B)
        // Raw joystick indices: 0=bottom(B), 1=right(A), 2=top(X), 3=left(Y)
        let mut gamepad_map = HashMap::new();
        gamepad_map.insert(0, Button::B);
        gamepad_map.insert(1, Button::A);
        gamepad_map.insert(2, Button::X);
        gamepad_map.insert(3, Button::Y);
        gamepad_map.insert(4, Button::L1);
        gamepad_map.insert(5, Button::R1);
        gamepad_map.insert(6, Button::L2);
        gamepad_map.insert(7, Button::R2);
        gamepad_map.insert(8, Button::DpadUp);
        gamepad_map.insert(9, Button::DpadDown);
        gamepad_map.insert(10, Button::DpadLeft);
        gamepad_map.insert(11, Button::DpadRight);
        gamepad_map.insert(12, Button::Select);
        gamepad_map.insert(13, Button::Start);

        Self {
            keyboard_map,
            gamepad_map,
            held: HashMap::new(),
            last_repeat: HashMap::new(),
            ignore_joystick: false,
        }
    }

    /// Call after opening game controllers to prevent duplicate events.
    pub fn set_ignore_joystick(&mut self, ignore: bool) {
        self.ignore_joystick = ignore;
    }

    pub fn process_events(&mut self, events: &[Event]) -> Vec<InputEvent> {
        let mut result = Vec::new();
        let now = Instant::now();

        for event in events {
            match event {
                Event::KeyDown {
                    keycode: Some(keycode),
                    repeat: false,
                    ..
                } => {
                    if let Some(&button) = self.keyboard_map.get(keycode) {
                        result.push(InputEvent {
                            button,
                            action: InputAction::Press,
                        });
                        self.held.insert(button, now);
                        self.last_repeat.insert(button, now);
                    }
                }
                Event::KeyUp {
                    keycode: Some(keycode),
                    ..
                } => {
                    if let Some(&button) = self.keyboard_map.get(keycode) {
                        result.push(InputEvent {
                            button,
                            action: InputAction::Release,
                        });
                        self.held.remove(&button);
                        self.last_repeat.remove(&button);
                    }
                }
                Event::JoyButtonDown { button_idx, .. } => {
                    if let Some(&button) = self.gamepad_map.get(button_idx) {
                        // Deduplicate: skip if already held (e.g. from GameController API)
                        if !self.held.contains_key(&button) {
                            result.push(InputEvent {
                                button,
                                action: InputAction::Press,
                            });
                            self.held.insert(button, now);
                            self.last_repeat.insert(button, now);
                        }
                    }
                }
                Event::JoyButtonUp { button_idx, .. } => {
                    if let Some(&button) = self.gamepad_map.get(button_idx) {
                        if self.held.contains_key(&button) {
                            result.push(InputEvent {
                                button,
                                action: InputAction::Release,
                            });
                            self.held.remove(&button);
                            self.last_repeat.remove(&button);
                        }
                    }
                }
                Event::JoyAxisMotion { axis_idx, value, .. } if !self.ignore_joystick => {
                    self.process_joy_axis(*axis_idx, *value, &mut result, now);
                }
                // GameController API (used by ArkOS and modern SDL2 setups)
                Event::ControllerButtonDown { button, .. } => {
                    if let Some(btn) = map_controller_button(*button) {
                        if !self.held.contains_key(&btn) {
                            result.push(InputEvent {
                                button: btn,
                                action: InputAction::Press,
                            });
                            self.held.insert(btn, now);
                            self.last_repeat.insert(btn, now);
                        }
                    }
                }
                Event::ControllerButtonUp { button, .. } => {
                    if let Some(btn) = map_controller_button(*button) {
                        if self.held.contains_key(&btn) {
                            result.push(InputEvent {
                                button: btn,
                                action: InputAction::Release,
                            });
                            self.held.remove(&btn);
                            self.last_repeat.remove(&btn);
                        }
                    }
                }
                Event::ControllerAxisMotion { axis, value, .. } => {
                    self.process_controller_axis(*axis, *value, &mut result, now);
                }
                _ => {}
            }
        }

        // Key repeat for held buttons
        let held_snapshot: Vec<(Button, Instant)> =
            self.held.iter().map(|(&b, &t)| (b, t)).collect();
        for (button, press_time) in held_snapshot {
            let held_duration = now.duration_since(press_time).as_secs_f64();
            if held_duration >= REPEAT_DELAY {
                let last = self
                    .last_repeat
                    .get(&button)
                    .copied()
                    .unwrap_or(press_time);
                if now.duration_since(last).as_secs_f64() >= REPEAT_INTERVAL {
                    result.push(InputEvent {
                        button,
                        action: InputAction::Repeat,
                    });
                    self.last_repeat.insert(button, now);
                }
            }
        }

        result
    }

    /// Handle joystick analog stick as D-pad input.
    fn process_joy_axis(
        &mut self,
        axis_idx: u8,
        value: i16,
        result: &mut Vec<InputEvent>,
        now: Instant,
    ) {
        const AXIS_THRESHOLD: i16 = 16000;

        match axis_idx {
            0 => {
                // Left stick X
                self.update_axis_button(Button::DpadLeft, value < -AXIS_THRESHOLD, result, now);
                self.update_axis_button(Button::DpadRight, value > AXIS_THRESHOLD, result, now);
            }
            1 => {
                // Left stick Y
                self.update_axis_button(Button::DpadUp, value < -AXIS_THRESHOLD, result, now);
                self.update_axis_button(Button::DpadDown, value > AXIS_THRESHOLD, result, now);
            }
            _ => {}
        }
    }

    /// Handle GameController analog axes (triggers + left stick as D-pad).
    fn process_controller_axis(
        &mut self,
        axis: sdl2::controller::Axis,
        value: i16,
        result: &mut Vec<InputEvent>,
        now: Instant,
    ) {
        const AXIS_THRESHOLD: i16 = 16000;
        const TRIGGER_THRESHOLD: i16 = 8000;

        match axis {
            sdl2::controller::Axis::LeftX => {
                self.update_axis_button(Button::DpadLeft, value < -AXIS_THRESHOLD, result, now);
                self.update_axis_button(Button::DpadRight, value > AXIS_THRESHOLD, result, now);
            }
            sdl2::controller::Axis::LeftY => {
                self.update_axis_button(Button::DpadUp, value < -AXIS_THRESHOLD, result, now);
                self.update_axis_button(Button::DpadDown, value > AXIS_THRESHOLD, result, now);
            }
            sdl2::controller::Axis::TriggerLeft => {
                self.update_axis_button(Button::L2, value > TRIGGER_THRESHOLD, result, now);
            }
            sdl2::controller::Axis::TriggerRight => {
                self.update_axis_button(Button::R2, value > TRIGGER_THRESHOLD, result, now);
            }
            _ => {}
        }
    }

    fn update_axis_button(
        &mut self,
        button: Button,
        active: bool,
        result: &mut Vec<InputEvent>,
        now: Instant,
    ) {
        if active && !self.held.contains_key(&button) {
            result.push(InputEvent {
                button,
                action: InputAction::Press,
            });
            self.held.insert(button, now);
            self.last_repeat.insert(button, now);
        } else if !active && self.held.contains_key(&button) {
            result.push(InputEvent {
                button,
                action: InputAction::Release,
            });
            self.held.remove(&button);
            self.last_repeat.remove(&button);
        }
    }
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Map SDL2 GameController buttons to our abstract Button type.
/// NOTE: SDL GameController uses Xbox layout (A=bottom, B=right).
/// R36S Plus uses Nintendo layout (A=right, B=bottom), so we swap A↔B and X↔Y.
fn map_controller_button(button: SdlControllerButton) -> Option<Button> {
    match button {
        SdlControllerButton::A => Some(Button::B), // Xbox bottom = Nintendo B
        SdlControllerButton::B => Some(Button::A), // Xbox right = Nintendo A
        SdlControllerButton::X => Some(Button::Y), // Xbox left = Nintendo Y
        SdlControllerButton::Y => Some(Button::X), // Xbox top = Nintendo X
        SdlControllerButton::LeftShoulder => Some(Button::L1),
        SdlControllerButton::RightShoulder => Some(Button::R1),
        SdlControllerButton::Back => Some(Button::Select),
        SdlControllerButton::Guide => Some(Button::Select), // Some controllers map Select to Guide
        SdlControllerButton::Start => Some(Button::Start),
        SdlControllerButton::DPadUp => Some(Button::DpadUp),
        SdlControllerButton::DPadDown => Some(Button::DpadDown),
        SdlControllerButton::DPadLeft => Some(Button::DpadLeft),
        SdlControllerButton::DPadRight => Some(Button::DpadRight),
        _ => None,
    }
}

/// Open all detected joysticks. The returned Vec must be kept alive
/// for the joysticks to remain open and emit events.
pub fn open_all_joysticks(
    subsystem: &sdl2::JoystickSubsystem,
) -> Vec<sdl2::joystick::Joystick> {
    let mut joysticks = Vec::new();
    let n = subsystem.num_joysticks().unwrap_or(0);
    for i in 0..n {
        match subsystem.open(i) {
            Ok(js) => {
                log::info!("Opened joystick {}: {}", i, js.name());
                joysticks.push(js);
            }
            Err(e) => {
                log::warn!("Failed to open joystick {}: {}", i, e);
            }
        }
    }
    joysticks
}

/// Open all detected game controllers. The returned Vec must be kept alive
/// for the controllers to remain open and emit events.
pub fn open_all_controllers(
    subsystem: &sdl2::GameControllerSubsystem,
) -> Vec<sdl2::controller::GameController> {
    let mut controllers = Vec::new();
    let n = subsystem.num_joysticks().unwrap_or(0);
    for i in 0..n {
        if subsystem.is_game_controller(i) {
            match subsystem.open(i) {
                Ok(gc) => {
                    log::info!("Opened game controller {}: {}", i, gc.name());
                    controllers.push(gc);
                }
                Err(e) => {
                    log::warn!("Failed to open game controller {}: {}", i, e);
                }
            }
        }
    }
    controllers
}

