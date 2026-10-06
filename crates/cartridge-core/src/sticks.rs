//! Independent sticks for apps that opt into `on_stick`; legacy buttons stay separate.
use sdl2::controller::Axis;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Keycode;
use std::collections::HashSet;

pub const DEADZONE: f32 = 0.18;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stick {
    Left,
    Right,
}
impl Stick {
    fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StickEvent {
    pub stick: Stick,
    pub x: f32,
    pub y: f32,
}

/// One physical controller at a time, plus simulator keys / scripted deflections.
/// Mapped SDL controllers use named axes; the handheld raw fallback uses 0/1 and 2/3.
/// Changes are coalesced once per frame, including recenter, disconnect and focus loss.
pub struct StickManager {
    mapped: HashSet<u32>,
    device: Option<u32>,
    physical: [[f32; 2]; 2],
    keys: [[bool; 4]; 2],
    simulated: [Option<[f32; 2]>; 2],
    sent: [[f32; 2]; 2],
    keyboard: bool,
}
impl StickManager {
    pub fn new(mapped: impl IntoIterator<Item = u32>, keyboard: bool) -> Self {
        Self {
            mapped: mapped.into_iter().collect(),
            device: None,
            physical: [[0.0; 2]; 2],
            keys: [[false; 4]; 2],
            simulated: [None; 2],
            sent: [[0.0; 2]; 2],
            keyboard,
        }
    }
    fn claim(&mut self, id: u32) -> bool {
        if self.device.is_none() {
            self.device = Some(id);
        }
        self.device == Some(id)
    }
    pub fn process_events(&mut self, events: &[Event]) {
        for event in events {
            match *event {
                Event::JoyAxisMotion {
                    which,
                    axis_idx,
                    value,
                    ..
                } if !self.mapped.contains(&which) && axis_idx < 4 => {
                    if self.claim(which) {
                        self.physical[axis_idx as usize / 2][axis_idx as usize % 2] =
                            axis_value(value);
                    }
                }
                Event::ControllerAxisMotion {
                    which, axis, value, ..
                } => {
                    let slot = match axis {
                        Axis::LeftX => Some((0, 0)),
                        Axis::LeftY => Some((0, 1)),
                        Axis::RightX => Some((1, 0)),
                        Axis::RightY => Some((1, 1)),
                        _ => None,
                    };
                    if let Some((stick, component)) = slot {
                        if self.claim(which) {
                            self.physical[stick][component] = axis_value(value);
                        }
                    }
                }
                Event::JoyDeviceRemoved { which, .. }
                | Event::ControllerDeviceRemoved { which, .. } => {
                    self.mapped.remove(&which);
                    if self.device == Some(which) {
                        self.physical = [[0.0; 2]; 2];
                        self.device = None;
                    }
                }
                Event::KeyDown {
                    keycode: Some(key),
                    repeat: false,
                    ..
                } if self.keyboard => {
                    if let Some((stick, direction)) = key_slot(key) {
                        self.keys[stick][direction] = true;
                    }
                }
                Event::KeyUp {
                    keycode: Some(key), ..
                } if self.keyboard => {
                    if let Some((stick, direction)) = key_slot(key) {
                        self.keys[stick][direction] = false;
                    }
                }
                Event::Window {
                    win_event: WindowEvent::FocusLost,
                    ..
                } => self.clear(),
                _ => {}
            }
        }
    }
    /// Raw normalized deflection, shaped by the same dead zone as physical input.
    pub fn inject(&mut self, stick: Stick, x: f32, y: f32) {
        self.simulated[stick.index()] = Some([finite_axis(x), finite_axis(y)]);
    }
    /// Keyboard overlays and hot reload must not retain motion from a previous context.
    pub fn clear(&mut self) {
        self.physical = [[0.0; 2]; 2];
        self.keys = [[false; 4]; 2];
        self.simulated = [None; 2];
    }
    fn value(&self, index: usize) -> [f32; 2] {
        let keys = self.keys[index];
        let raw = if keys.iter().any(|v| *v) {
            [
                (keys[3] as i32 - keys[2] as i32) as f32,
                (keys[1] as i32 - keys[0] as i32) as f32,
            ]
        } else {
            self.simulated[index].unwrap_or(self.physical[index])
        };
        shape(raw[0], raw[1])
    }
    pub fn take_changes(&mut self) -> Vec<StickEvent> {
        let mut changes = Vec::new();
        for stick in [Stick::Left, Stick::Right] {
            let index = stick.index();
            let value = self.value(index);
            if value != self.sent[index] {
                self.sent[index] = value;
                changes.push(StickEvent {
                    stick,
                    x: value[0],
                    y: value[1],
                });
            }
        }
        changes
    }
    pub fn active(&self) -> bool {
        (0..2).any(|i| self.value(i) != [0.0, 0.0])
    }
}
fn finite_axis(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}
fn axis_value(value: i16) -> f32 {
    value as f32 / if value < 0 { 32768.0 } else { 32767.0 }
}
fn shape(x: f32, y: f32) -> [f32; 2] {
    let (x, y) = (finite_axis(x), finite_axis(y));
    let length = x.hypot(y);
    if length <= DEADZONE {
        return [0.0, 0.0];
    }
    let magnitude = ((length - DEADZONE) / (1.0 - DEADZONE)).min(1.0);
    [x / length * magnitude, y / length * magnitude]
}
fn key_slot(key: Keycode) -> Option<(usize, usize)> {
    Some(match key {
        Keycode::I => (0, 0),
        Keycode::K => (0, 1),
        Keycode::J => (0, 2),
        Keycode::L => (0, 3),
        Keycode::T => (1, 0),
        Keycode::G => (1, 1),
        Keycode::F => (1, 2),
        Keycode::H => (1, 3),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw(which: u32, axis_idx: u8, value: i16) -> Event {
        Event::JoyAxisMotion {
            timestamp: 0,
            which,
            axis_idx,
            value,
        }
    }
    fn named(which: u32, axis: Axis, value: i16) -> Event {
        Event::ControllerAxisMotion {
            timestamp: 0,
            which,
            axis,
            value,
        }
    }
    #[test]
    fn raw_and_named_sticks_are_independent_and_duplicates_are_suppressed() {
        let mut m = StickManager::new([], false);
        m.process_events(&[raw(4, 0, 32767), raw(4, 3, -32768)]);
        assert_eq!(
            m.take_changes(),
            vec![
                StickEvent {
                    stick: Stick::Left,
                    x: 1.0,
                    y: 0.0
                },
                StickEvent {
                    stick: Stick::Right,
                    x: 0.0,
                    y: -1.0
                }
            ]
        );
        assert!(m.take_changes().is_empty());
        let mut mapped = StickManager::new([4], false);
        mapped.process_events(&[
            raw(4, 2, 32767),
            named(4, Axis::RightY, -32768),
            named(4, Axis::TriggerLeft, 32767),
        ]);
        assert_eq!(
            mapped.take_changes(),
            vec![StickEvent {
                stick: Stick::Right,
                x: 0.0,
                y: -1.0
            }]
        );
        mapped.process_events(&[raw(8, 0, 32767)]);
        assert!(mapped.take_changes().is_empty());
    }
    #[test]
    fn deadzone_bounds_diagonals_and_recentering_stop_motion() {
        let mut m = StickManager::new([], false);
        m.process_events(&[raw(1, 0, 2000), raw(1, 1, -2000)]);
        assert!(!m.active());
        assert!(m.take_changes().is_empty());
        m.process_events(&[raw(1, 0, 32767), raw(1, 1, 32767)]);
        let v = m.take_changes()[0];
        assert!((v.x.hypot(v.y) - 1.0).abs() < 0.00001);
        m.process_events(&[raw(1, 0, 0), raw(1, 1, 0)]);
        assert_eq!(
            m.take_changes()[0],
            StickEvent {
                stick: Stick::Left,
                x: 0.0,
                y: 0.0
            }
        );
        assert!(!m.active());
        m.inject(Stick::Right, f32::NAN, f32::INFINITY);
        assert!(!m.active());
    }
    #[test]
    fn unplug_focus_loss_and_overlay_reset_deliver_neutral() {
        let mut m = StickManager::new([7], false);
        m.process_events(&[named(7, Axis::LeftY, -32768)]);
        m.take_changes();
        m.process_events(&[Event::ControllerDeviceRemoved {
            timestamp: 0,
            which: 7,
        }]);
        assert_eq!(m.take_changes()[0].y, 0.0);
        m.process_events(&[raw(8, 2, 32767)]);
        assert_eq!(m.take_changes()[0].x, 1.0);
        m.process_events(&[Event::Window {
            timestamp: 0,
            window_id: 1,
            win_event: WindowEvent::FocusLost,
        }]);
        assert_eq!(m.take_changes()[0].x, 0.0);
        m.inject(Stick::Left, 1.0, 0.0);
        m.take_changes();
        m.clear();
        assert_eq!(m.take_changes()[0].x, 0.0);
    }
    #[test]
    fn simulator_keys_do_not_collide_with_face_shoulder_or_volume_keys() {
        let mut m = StickManager::new([], true);
        let key = |keycode, down| {
            if down {
                Event::KeyDown {
                    timestamp: 0,
                    window_id: 1,
                    keycode: Some(keycode),
                    scancode: None,
                    keymod: sdl2::keyboard::Mod::NOMOD,
                    repeat: false,
                }
            } else {
                Event::KeyUp {
                    timestamp: 0,
                    window_id: 1,
                    keycode: Some(keycode),
                    scancode: None,
                    keymod: sdl2::keyboard::Mod::NOMOD,
                    repeat: false,
                }
            }
        };
        m.process_events(&[
            key(Keycode::L, true),
            key(Keycode::T, true),
            key(Keycode::Q, true),
        ]);
        assert_eq!(m.take_changes().len(), 2);
        m.process_events(&[key(Keycode::L, false), key(Keycode::T, false)]);
        assert_eq!(m.take_changes().len(), 2);
        assert!(!m.active());
        let mut device = StickManager::new([], false);
        device.process_events(&[key(Keycode::L, true)]);
        assert!(!device.active());
    }
}
