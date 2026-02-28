"""Timer screen - main pomodoro timer display with circular ring."""

import math
import pygame
from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui import StatusBar

from ..timer import Phase, PHASE_COLORS, PHASE_COLORS_DIM, PHASE_LABELS


class TimerScreen:
    """Main timer screen with circular progress ring and session dots."""

    def __init__(self, app):
        self.app = app
        self.status_bar = StatusBar("Pomodoro")

        # Animation state
        self._pulse_t: float = 0.0

    @property
    def engine(self):
        return self.app.timer_engine

    def handle_input(self, event: InputEvent) -> None:
        if event.button == Button.A and event.pressed:
            self.engine.toggle()
        elif event.button == Button.X and event.pressed:
            self.engine.reset()
        elif event.button == Button.Y and event.pressed:
            self.engine.skip()
        elif event.button == Button.R1 and event.pressed:
            self.app.push_screen("stats")
        elif event.button == Button.L1 and event.pressed:
            self.app.push_screen("settings")

    def update(self, dt: float) -> None:
        self._pulse_t += dt

    def draw(self, screen) -> None:
        theme = screen.theme
        screen.clear()
        self.status_bar.draw(screen)

        surface = screen.surface
        engine = self.engine

        # Center of content area
        content_top = 40
        content_bottom = 444
        content_center_y = (content_top + content_bottom) // 2 - 10
        cx = 320
        cy = content_center_y - 20

        phase_color = engine.phase_color
        phase_color_dim = engine.phase_color_dim
        track_color = (50, 50, 65)

        # Draw large circular ring
        radius = 130
        ring_width = 12
        self._draw_ring(surface, cx, cy, radius, engine.progress,
                        phase_color, track_color, ring_width)

        # Inner glow ring (subtle)
        inner_glow_color = (
            phase_color_dim[0] // 2,
            phase_color_dim[1] // 2,
            phase_color_dim[2] // 2,
        )
        pygame.draw.circle(surface, inner_glow_color, (cx, cy), radius - ring_width - 2, 1)

        # Time display inside ring
        time_str = engine.format_time()
        time_font = _get_font("mono_bold", 52)
        time_surf = time_font.render(time_str, True, theme.text)
        time_rect = time_surf.get_rect(center=(cx, cy - 8))
        surface.blit(time_surf, time_rect)

        # Phase label below time
        label = engine.phase_label
        label_font = _get_font("mono", 18)
        label_surf = label_font.render(label, True, phase_color)
        label_rect = label_surf.get_rect(center=(cx, cy + 32))
        surface.blit(label_surf, label_rect)

        # Running indicator (pulsing dot)
        if engine.running:
            pulse = (math.sin(self._pulse_t * 4) + 1) / 2  # 0..1
            dot_alpha = int(100 + 155 * pulse)
            dot_color = (
                min(255, phase_color[0]),
                min(255, phase_color[1]),
                min(255, phase_color[2]),
            )
            dot_radius = int(4 + 2 * pulse)
            pygame.draw.circle(surface, dot_color, (cx, cy + 52), dot_radius)
        else:
            # Paused text
            if engine.remaining < engine.duration:
                paused_font = _get_font("mono", 13)
                paused_surf = paused_font.render("PAUSED", True, theme.text_dim)
                paused_rect = paused_surf.get_rect(center=(cx, cy + 54))
                surface.blit(paused_surf, paused_rect)

        # Session dots (4 circles below ring showing progress toward long break)
        self._draw_session_dots(surface, cx, cy + radius + 40, engine.work_count, phase_color, theme)

        # Completed count
        count_text = f"{engine.total_completed} pomodoro{'s' if engine.total_completed != 1 else ''} today"
        count_font = _get_font("mono", 14)
        count_surf = count_font.render(count_text, True, theme.text_dim)
        count_rect = count_surf.get_rect(center=(cx, cy + radius + 70))
        surface.blit(count_surf, count_rect)

        # Footer
        self._draw_footer(screen)

    def _draw_ring(self, surface, cx, cy, radius, progress, color, track_color, width=12):
        """Draw circular progress ring that fills clockwise from top."""
        # Track (full circle, slightly transparent feel)
        pygame.draw.circle(surface, track_color, (cx, cy), radius, width)

        if progress <= 0:
            return

        if progress >= 1.0:
            pygame.draw.circle(surface, color, (cx, cy), radius, width)
            return

        # Draw filled arc from top, clockwise
        # pygame.draw.arc uses counter-clockwise angles from the right (+x axis)
        # Top = pi/2 in pygame coords (y-axis inverted)
        # We want to go clockwise, which is decreasing angle in standard math
        start_angle_top = math.pi / 2
        sweep = 2 * math.pi * progress
        end_angle = start_angle_top - sweep

        rect = pygame.Rect(cx - radius, cy - radius, radius * 2, radius * 2)

        # Draw the arc with thick width
        # pygame.draw.arc can look jagged at thick widths, so we draw multiple
        # thin arcs to create a smoother thick arc
        for w_offset in range(-width // 2, width // 2 + 1):
            r = radius + w_offset
            if r <= 0:
                continue
            arc_rect = pygame.Rect(cx - r, cy - r, r * 2, r * 2)
            pygame.draw.arc(surface, color, arc_rect, end_angle, start_angle_top, 2)

        # Draw rounded end caps for polished look
        # Start cap (at top)
        start_x = cx + int(radius * math.cos(start_angle_top))
        start_y = cy - int(radius * math.sin(start_angle_top))
        pygame.draw.circle(surface, color, (start_x, start_y), width // 2)

        # End cap (at current progress point)
        end_x = cx + int(radius * math.cos(end_angle))
        end_y = cy - int(radius * math.sin(end_angle))
        pygame.draw.circle(surface, color, (end_x, end_y), width // 2)

    def _draw_session_dots(self, surface, cx, y, work_count, phase_color, theme):
        """Draw 4 session dots showing progress toward long break."""
        dot_radius = 8
        spacing = 30
        total_width = 3 * spacing
        start_x = cx - total_width // 2

        for i in range(4):
            dx = start_x + i * spacing
            if i < work_count:
                # Filled dot
                pygame.draw.circle(surface, phase_color, (dx, y), dot_radius)
            else:
                # Empty dot (outline)
                pygame.draw.circle(surface, theme.border, (dx, y), dot_radius, 2)

    def _draw_footer(self, screen):
        theme = screen.theme
        y = 444
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, 36))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        if self.engine.running:
            hx += screen.draw_button_hint("A", "Pause", hx, y + 8, btn_color=theme.btn_a) + 14
        else:
            hx += screen.draw_button_hint("A", "Start", hx, y + 8, btn_color=theme.btn_a) + 14

        hx += screen.draw_button_hint("X", "Reset", hx, y + 8, btn_color=theme.btn_x) + 14
        hx += screen.draw_button_hint("Y", "Skip", hx, y + 8, btn_color=theme.btn_y) + 14

        # Right side nav hints
        rx = 630
        w = screen.draw_button_hint("R1", "Stats", rx - 80, y + 8, btn_color=theme.btn_l)
        screen.draw_button_hint("L1", "Settings", rx - 200, y + 8, btn_color=theme.btn_l)
