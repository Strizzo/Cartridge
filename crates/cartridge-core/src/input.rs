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
        // Button indices: 0=bottom(B), 1=right(A), 2=top(X→Y), 3=left(Y→X)
        let mut gamepad_map = HashMap::new();
        gamepad_map.insert(0, Button::B);
        gamepad_map.insert(1, Button::A);
        gamepad_map.insert(2, Button::Y);
        gamepad_map.insert(3, Button::X);
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
        }
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
                        result.push(InputEvent {
                            button,
                            action: InputAction::Press,
                        });
                        self.held.insert(button, now);
                        self.last_repeat.insert(button, now);
                    }
                }
                Event::JoyButtonUp { button_idx, .. } => {
                    if let Some(&button) = self.gamepad_map.get(button_idx) {
                        result.push(InputEvent {
                            button,
                            action: InputAction::Release,
                        });
                        self.held.remove(&button);
                        self.last_repeat.remove(&button);
                    }
                }
                Event::JoyAxisMotion { axis_idx, value, .. } => {
                    self.process_joy_axis(*axis_idx, *value, &mut result, now);
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

