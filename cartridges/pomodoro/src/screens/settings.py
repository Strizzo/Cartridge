"""Settings screen - customize work/break durations."""

import pygame
from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui import StatusBar

from ..timer import Phase, WORK_PRESETS, SHORT_BREAK_PRESETS, LONG_BREAK_PRESETS


# Each setting: (label, phase, presets_list)
SETTINGS = [
    ("Work Duration", Phase.WORK, WORK_PRESETS),
    ("Short Break", Phase.SHORT_BREAK, SHORT_BREAK_PRESETS),
    ("Long Break", Phase.LONG_BREAK, LONG_BREAK_PRESETS),
]


class SettingsScreen:
    """Screen to customize timer durations from presets."""

    def __init__(self, app):
        self.app = app
        self.status_bar = StatusBar("Settings")
        self._selected_row: int = 0
        self._selected_indices: list[int] = [0, 0, 0]  # preset index per row

        # Initialize selected indices from current engine durations
        self._sync_from_engine()

    @property
    def engine(self):
        return self.app.timer_engine

    def _sync_from_engine(self) -> None:
        """Set selected indices to match current engine durations."""
        for row_idx, (label, phase, presets) in enumerate(SETTINGS):
            current_min = self.engine.durations[phase] // 60
            if current_min in presets:
                self._selected_indices[row_idx] = presets.index(current_min)
            else:
                # Find closest preset
                closest = min(range(len(presets)), key=lambda i: abs(presets[i] - current_min))
                self._selected_indices[row_idx] = closest

    def handle_input(self, event: InputEvent) -> None:
        if event.button == Button.B and event.pressed:
            self.app.pop_screen()
            return

        if event.button == Button.DPAD_DOWN and event.pressed:
            self._selected_row = min(self._selected_row + 1, len(SETTINGS) - 1)
        elif event.button == Button.DPAD_UP and event.pressed:
            self._selected_row = max(self._selected_row - 1, 0)
        elif event.button == Button.DPAD_RIGHT and event.pressed:
            self._change_preset(1)
        elif event.button == Button.DPAD_LEFT and event.pressed:
            self._change_preset(-1)
        elif event.button == Button.A and event.pressed:
            self._apply_current()
        elif event.button == Button.R1 and event.pressed:
            self._change_preset(1)
        elif event.button == Button.L1 and event.pressed:
            self._change_preset(-1)

    def _change_preset(self, direction: int) -> None:
        """Cycle through presets for the selected row."""
        row = self._selected_row
        _, phase, presets = SETTINGS[row]
        idx = self._selected_indices[row]
        idx = (idx + direction) % len(presets)
        self._selected_indices[row] = idx
        # Apply immediately
        self.engine.set_duration(phase, presets[idx])

    def _apply_current(self) -> None:
        """Apply the currently highlighted preset."""
        row = self._selected_row
        _, phase, presets = SETTINGS[row]
        idx = self._selected_indices[row]
        self.engine.set_duration(phase, presets[idx])

    def draw(self, screen) -> None:
        theme = screen.theme
        screen.clear()
        self.status_bar.draw(screen)

        surface = screen.surface
        y = 60

        # Title
        title_font = _get_font("mono_bold", 18)
        title_surf = title_font.render("Timer Durations", True, theme.text)
        surface.blit(title_surf, (20, y))
        y += 36

        # Description
        desc_font = _get_font("mono", 13)
        desc_surf = desc_font.render("Use LEFT/RIGHT to change, A to confirm", True, theme.text_dim)
        surface.blit(desc_surf, (20, y))
        y += 32

        # Settings rows
        for row_idx, (label, phase, presets) in enumerate(SETTINGS):
            selected = row_idx == self._selected_row
            preset_idx = self._selected_indices[row_idx]
            self._draw_setting_row(screen, 20, y, 600, label, phase, presets, preset_idx, selected)
            y += 80

        # Visual preview of current cycle
        y += 20
        self._draw_cycle_preview(screen, 20, y)

        # Footer
        self._draw_footer(screen)

    def _draw_setting_row(self, screen, x, y, w, label, phase, presets, preset_idx, selected):
        """Draw a setting row with label and preset selector."""
        theme = screen.theme
        surface = screen.surface
        h = 64

        # Row card
        border_color = theme.accent if selected else theme.card_border
        bg = theme.card_highlight if selected else theme.card_bg
        screen.draw_card((x, y, w, h), bg=bg, border=border_color, radius=10, shadow=True)

        # Selection indicator
        if selected:
            pygame.draw.rect(surface, theme.accent, (x + 4, y + 14, 3, h - 28), border_radius=2)

        # Label
        label_font = _get_font("mono_bold", 16)
        label_surf = label_font.render(label, True, theme.text)
        surface.blit(label_surf, (x + 18, y + 10))

        # Preset pills
        pill_y = y + 36
        pill_x = x + 18

        from ..timer import PHASE_COLORS
        phase_color = PHASE_COLORS[phase]

        for i, preset in enumerate(presets):
            is_active = i == preset_idx
            text = f"{preset} min"

            if is_active:
                pill_bg = phase_color
                pill_text_color = (18, 18, 24)
            else:
                pill_bg = theme.bg_lighter
                pill_text_color = theme.text_dim

            pw = screen.draw_pill(text, pill_x, pill_y, bg_color=pill_bg, text_color=pill_text_color, font_size=13)
            pill_x += pw + 8

        # Left/right arrows for selected row
        if selected:
            arrow_font = _get_font("mono_bold", 20)
            left_surf = arrow_font.render("<", True, theme.accent)
            right_surf = arrow_font.render(">", True, theme.accent)
            surface.blit(left_surf, (x + w - 50, y + 20))
            surface.blit(right_surf, (x + w - 22, y + 20))

    def _draw_cycle_preview(self, screen, x, y):
        """Draw a visual preview of what one full pomodoro cycle looks like."""
        theme = screen.theme
        surface = screen.surface

        preview_font = _get_font("mono", 13)
        header_surf = preview_font.render("Cycle preview:", True, theme.text_dim)
        surface.blit(header_surf, (x, y))
        y += 22

        from ..timer import PHASE_COLORS

        work_min = self.engine.durations[Phase.WORK] // 60
        short_min = self.engine.durations[Phase.SHORT_BREAK] // 60
        long_min = self.engine.durations[Phase.LONG_BREAK] // 60

        # Draw cycle: W-S-W-S-W-S-W-L
        cycle = [
            (f"W {work_min}m", PHASE_COLORS[Phase.WORK]),
            (f"S {short_min}m", PHASE_COLORS[Phase.SHORT_BREAK]),
            (f"W {work_min}m", PHASE_COLORS[Phase.WORK]),
            (f"S {short_min}m", PHASE_COLORS[Phase.SHORT_BREAK]),
            (f"W {work_min}m", PHASE_COLORS[Phase.WORK]),
            (f"S {short_min}m", PHASE_COLORS[Phase.SHORT_BREAK]),
            (f"W {work_min}m", PHASE_COLORS[Phase.WORK]),
            (f"L {long_min}m", PHASE_COLORS[Phase.LONG_BREAK]),
        ]

        px = x
        for text, color in cycle:
            pw = screen.draw_pill(text, px, y, bg_color=color, text_color=(18, 18, 24), font_size=12)
            px += pw + 4
            if px > 600:
                break

    def _draw_footer(self, screen):
        theme = screen.theme
        y = 444
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, 36))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        hx += screen.draw_button_hint("B", "Back", hx, y + 8, btn_color=theme.btn_b) + 14
        hx += screen.draw_button_hint("A", "Select", hx, y + 8, btn_color=theme.btn_a) + 14

        nav_font = _get_font("mono", 12)
        nav_surf = nav_font.render("L1/R1 or LEFT/RIGHT: Change", True, theme.text_dim)
        screen.surface.blit(nav_surf, (430, y + 10))
