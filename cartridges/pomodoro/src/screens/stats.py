"""Stats screen - today's pomodoro statistics and session history."""

import pygame
from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui import StatusBar


class StatsScreen:
    """Display completed pomodoros, focus time, and session history."""

    def __init__(self, app):
        self.app = app
        self.status_bar = StatusBar("Statistics")
        self._scroll_offset: int = 0
        self._max_scroll: int = 0

    @property
    def engine(self):
        return self.app.timer_engine

    def handle_input(self, event: InputEvent) -> None:
        if event.button == Button.B and event.pressed:
            self.app.pop_screen()
        elif event.button == Button.Y and event.pressed:
            self._reset_stats()
        elif event.button == Button.DPAD_DOWN and event.pressed:
            self._scroll_offset = min(self._scroll_offset + 40, self._max_scroll)
        elif event.button == Button.DPAD_UP and event.pressed:
            self._scroll_offset = max(self._scroll_offset - 40, 0)

    def _reset_stats(self) -> None:
        self.engine.total_completed = 0
        self.engine.total_focus_seconds = 0
        self.engine.sessions.clear()
        self.engine.work_count = 0
        self._scroll_offset = 0
        self.app.save_stats()

    def draw(self, screen) -> None:
        theme = screen.theme
        screen.clear()
        self.status_bar.draw(screen)

        surface = screen.surface
        engine = self.engine

        y_start = 52
        y = y_start - self._scroll_offset

        # --- Summary cards ---
        card_h = 80
        card_gap = 12
        card_w = 194
        cards_x = 16

        # Card 1: Completed pomodoros
        self._draw_stat_card(
            screen, cards_x, y, card_w, card_h,
            str(engine.total_completed),
            "Completed",
            (220, 80, 80),
        )

        # Card 2: Total focus time
        focus_min = engine.total_focus_seconds // 60
        if focus_min >= 60:
            hours = focus_min // 60
            mins = focus_min % 60
            focus_str = f"{hours}h {mins}m"
        else:
            focus_str = f"{focus_min}m"

        self._draw_stat_card(
            screen, cards_x + card_w + card_gap, y, card_w, card_h,
            focus_str,
            "Focus Time",
            (80, 200, 120),
        )

        # Card 3: Current streak (work_count toward next long break)
        streak = engine.work_count
        self._draw_stat_card(
            screen, cards_x + 2 * (card_w + card_gap), y, card_w, card_h,
            f"{streak}/4",
            "Streak",
            (80, 140, 240),
        )

        y += card_h + 20

        # --- Session history header ---
        hist_font = _get_font("mono_bold", 16)
        hist_surf = hist_font.render("Session History", True, theme.text)
        surface.blit(hist_surf, (20, y))
        y += 28

        sessions = engine.sessions
        if not sessions:
            empty_font = _get_font("mono", 14)
            empty_surf = empty_font.render("No sessions yet. Start a pomodoro!", True, theme.text_dim)
            surface.blit(empty_surf, (20, y))
            y += 30
        else:
            # Draw sessions in reverse order (newest first)
            for i, session in enumerate(reversed(sessions)):
                if y > 440:
                    break
                if y + 44 > 40:  # Only draw if visible
                    self._draw_session_row(screen, 16, y, 608, session, i)
                y += 50

        # Calculate max scroll
        total_content_height = (y + self._scroll_offset) - y_start
        visible_height = 444 - y_start
        self._max_scroll = max(0, total_content_height - visible_height)

        # Footer
        self._draw_footer(screen)

    def _draw_stat_card(self, screen, x, y, w, h, value, label, accent_color):
        """Draw a statistics summary card."""
        theme = screen.theme

        # Card background
        screen.draw_card(
            (x, y, w, h),
            bg=theme.card_bg,
            border=theme.card_border,
            radius=10,
            shadow=True,
        )

        # Accent bar at top
        pygame.draw.rect(screen.surface, accent_color, (x + 10, y + 6, w - 20, 3), border_radius=2)

        # Value (large)
        val_font = _get_font("mono_bold", 28)
        val_surf = val_font.render(value, True, theme.text)
        val_rect = val_surf.get_rect(center=(x + w // 2, y + 38))
        screen.surface.blit(val_surf, val_rect)

        # Label
        lab_font = _get_font("mono", 13)
        lab_surf = lab_font.render(label, True, theme.text_dim)
        lab_rect = lab_surf.get_rect(center=(x + w // 2, y + 62))
        screen.surface.blit(lab_surf, lab_rect)

    def _draw_session_row(self, screen, x, y, w, session, index):
        """Draw a single session history row."""
        theme = screen.theme
        surface = screen.surface
        h = 42

        # Row background
        bg = theme.card_bg if index % 2 == 0 else theme.bg_lighter
        screen.draw_rounded_rect((x, y, w, h), bg, radius=6)

        # Status indicator dot
        completed = session.get("completed", False)
        dot_color = (80, 200, 120) if completed else (240, 80, 90)
        pygame.draw.circle(surface, dot_color, (x + 18, y + h // 2), 5)

        # Start time
        start = session.get("start", "--:--")
        time_font = _get_font("mono_bold", 15)
        time_surf = time_font.render(start, True, theme.text)
        surface.blit(time_surf, (x + 34, y + 12))

        # Duration
        dur = session.get("duration_min", 0)
        dur_str = f"{dur:.0f} min" if dur == int(dur) else f"{dur:.1f} min"
        dur_font = _get_font("mono", 14)
        dur_surf = dur_font.render(dur_str, True, theme.text_dim)
        surface.blit(dur_surf, (x + 120, y + 13))

        # Status text
        status = "Completed" if completed else "Skipped"
        status_color = (80, 200, 120) if completed else theme.text_dim
        status_font = _get_font("mono", 13)
        status_surf = status_font.render(status, True, status_color)
        status_rect = status_surf.get_rect(right=x + w - 16, centery=y + h // 2)
        surface.blit(status_surf, status_rect)

    def _draw_footer(self, screen):
        theme = screen.theme
        y = 444
        pygame.draw.rect(screen.surface, theme.bg_header, (0, y, 640, 36))
        pygame.draw.line(screen.surface, theme.border, (0, y), (640, y))

        hx = 10
        hx += screen.draw_button_hint("B", "Back", hx, y + 8, btn_color=theme.btn_b) + 14
        hx += screen.draw_button_hint("Y", "Reset", hx, y + 8, btn_color=theme.btn_y) + 14

        # Scroll hint on right
        if self._max_scroll > 0:
            scroll_font = _get_font("mono", 12)
            scroll_surf = scroll_font.render("D-Pad: Scroll", True, theme.text_dim)
            screen.surface.blit(scroll_surf, (540, y + 10))
