"""Button enum, InputEvent, and InputManager for Cartridge apps."""

from __future__ import annotations

import enum
import time
from dataclasses import dataclass, field

import pygame


class Button(enum.Enum):
    """Abstract hardware buttons. Maps to d-pad, face buttons, shoulders."""

    DPAD_UP = "dpad_up"
    DPAD_DOWN = "dpad_down"
    DPAD_LEFT = "dpad_left"
    DPAD_RIGHT = "dpad_right"
    A = "a"            # Confirm / Open
    B = "b"            # Back / Cancel
    X = "x"            # Action 1
    Y = "y"            # Action 2
    L1 = "l1"          # Page up / Tab left
    R1 = "r1"          # Page down / Tab right
    L2 = "l2"          # Secondary action
    R2 = "r2"          # Secondary action
    START = "start"    # Menu / Settings
    SELECT = "select"  # Alt menu / Toggle


@dataclass
class InputEvent:
    """A processed input event delivered to CartridgeApp.on_input()."""

    button: Button
    action: str  # "press", "release", "repeat"


# Desktop keyboard mapping (simulates gamepad on PC)
KEYBOARD_MAP: dict[int, Button] = {
    pygame.K_UP: Button.DPAD_UP,
    pygame.K_DOWN: Button.DPAD_DOWN,
    pygame.K_LEFT: Button.DPAD_LEFT,
    pygame.K_RIGHT: Button.DPAD_RIGHT,
    pygame.K_z: Button.A,
    pygame.K_x: Button.B,
    pygame.K_c: Button.X,
    pygame.K_v: Button.Y,
    pygame.K_a: Button.L1,
    pygame.K_s: Button.R1,
    pygame.K_q: Button.L2,
    pygame.K_w: Button.R2,
    pygame.K_RETURN: Button.START,
    pygame.K_SPACE: Button.SELECT,
}

# Gamepad button index -> Button (standard SDL2 GameController layout)
GAMEPAD_MAP: dict[int, Button] = {
    0: Button.A,
    1: Button.B,
    2: Button.X,
    3: Button.Y,
    4: Button.L1,
    5: Button.R1,
    6: Button.L2,
    7: Button.R2,
    8: Button.SELECT,
    9: Button.START,
}

# Key repeat settings
_REPEAT_DELAY = 0.4   # seconds before repeat starts
_REPEAT_INTERVAL = 0.08  # seconds between repeats


class InputManager:
    """Translates raw pygame events into InputEvents with key repeat."""

    def __init__(self) -> None:
        self._held: dict[Button, float] = {}  # button -> time first pressed
        self._last_repeat: dict[Button, float] = {}  # button -> time of last repeat

        pygame.joystick.init()
        self._joysticks: list[pygame.joystick.JoystickType] = []
        self._scan_joysticks()

    def _scan_joysticks(self) -> None:
        self._joysticks = []
        for i in range(pygame.joystick.get_count()):
            js = pygame.joystick.Joystick(i)
            js.init()
            self._joysticks.append(js)

    def get_events(self, raw_events: list[pygame.event.Event]) -> list[InputEvent]:
        result: list[InputEvent] = []
        now = time.monotonic()

        for ev in raw_events:
            # Keyboard
            if ev.type in (pygame.KEYDOWN, pygame.KEYUP):
                pressed = ev.type == pygame.KEYDOWN
                button = KEYBOARD_MAP.get(ev.key)
                if button is None:
                    continue
                if pressed:
                    result.append(InputEvent(button, "press"))
                    self._held[button] = now
                    self._last_repeat[button] = now
                else:
                    result.append(InputEvent(button, "release"))
                    self._held.pop(button, None)
                    self._last_repeat.pop(button, None)

            # Gamepad hat (D-pad)
            elif ev.type == pygame.JOYHATMOTION:
                hx, hy = ev.value
                # Release all d-pad buttons first
                for btn in (Button.DPAD_UP, Button.DPAD_DOWN, Button.DPAD_LEFT, Button.DPAD_RIGHT):
                    if btn in self._held and not self._hat_active(hx, hy, btn):
                        result.append(InputEvent(btn, "release"))
                        self._held.pop(btn, None)
                        self._last_repeat.pop(btn, None)
                # Press active directions
                hat_buttons = self._hat_to_buttons(hx, hy)
                for btn in hat_buttons:
                    if btn not in self._held:
                        result.append(InputEvent(btn, "press"))
                        self._held[btn] = now
                        self._last_repeat[btn] = now

            # Gamepad buttons
            elif ev.type in (pygame.JOYBUTTONDOWN, pygame.JOYBUTTONUP):
                pressed = ev.type == pygame.JOYBUTTONDOWN
                button = GAMEPAD_MAP.get(ev.button)
                if button is None:
                    continue
                if pressed:
                    result.append(InputEvent(button, "press"))
                    self._held[button] = now
                    self._last_repeat[button] = now
                else:
                    result.append(InputEvent(button, "release"))
                    self._held.pop(button, None)
                    self._last_repeat.pop(button, None)

        # Key repeat for held buttons
        for button, press_time in list(self._held.items()):
            held_duration = now - press_time
            if held_duration >= _REPEAT_DELAY:
                last = self._last_repeat.get(button, press_time)
                if now - last >= _REPEAT_INTERVAL:
                    result.append(InputEvent(button, "repeat"))
                    self._last_repeat[button] = now

        return result

    def _hat_active(self, hx: int, hy: int, button: Button) -> bool:
        if button == Button.DPAD_UP:
            return hy == 1
        if button == Button.DPAD_DOWN:
            return hy == -1
        if button == Button.DPAD_LEFT:
            return hx == -1
        if button == Button.DPAD_RIGHT:
            return hx == 1
        return False

    def _hat_to_buttons(self, hx: int, hy: int) -> list[Button]:
        buttons = []
        if hy == 1:
            buttons.append(Button.DPAD_UP)
        elif hy == -1:
            buttons.append(Button.DPAD_DOWN)
        if hx == -1:
            buttons.append(Button.DPAD_LEFT)
        elif hx == 1:
            buttons.append(Button.DPAD_RIGHT)
        return buttons
