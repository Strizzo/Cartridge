"""
History screen showing past calculations.
Scrollable list of expression = result entries.
"""

import pygame
from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui import StatusBar


class HistoryScreen:
    """Displays calculation history with scrollable list."""

    def __init__(self, app):
        self.app = app
        self.status_bar = StatusBar("History")
        self.selected_index = 0
        self.scroll_offset = 0

        # Layout constants
        self.content_y = 44
        self.content_h = 400  # 444 - 44
        self.item_h = 62
        self.item_gap = 6
        self.items_visible = 5  # roughly how many fit

    def handle_input(self, event: InputEvent) -> None:
        if event.action not in ("press", "repeat"):
            return

        history = self.app.history

        if event.button == Button.B:
            self.app.pop_screen()
        elif event.button == Button.A:
            # Recall selected expression
            if history:
                idx = self.selected_index
                if 0 <= idx < len(history):
                    entry = history[idx]
                    self.app.calc_screen.set_expression(entry["expr"])
                    self.app.pop_screen()
        elif event.button == Button.X:
            # Clear history
            self.app.clear_history()
            self.selected_index = 0
            self.scroll_offset = 0
        elif event.button == Button.DPAD_UP:
            if history:
                self.selected_index = max(0, self.selected_index - 1)
                self._ensure_visible()
        elif event.button == Button.DPAD_DOWN:
            if history:
                self.selected_index = min(len(history) - 1, self.selected_index + 1)
                self._ensure_visible()

    def _ensure_visible(self):
        """Ensure the selected item is visible in the scroll view."""
        if self.selected_index < self.scroll_offset:
            self.scroll_offset = self.selected_index
        elif self.selected_index >= self.scroll_offset + self.items_visible:
            self.scroll_offset = self.selected_index - self.items_visible + 1

    def on_enter(self):
        """Called when this screen becomes active."""
        history = self.app.history
        if history:
            self.selected_index = min(self.selected_index, len(history) - 1)
            self.selected_index = max(0, self.selected_index)
        else:
            self.selected_index = 0
            self.scroll_offset = 0

    def draw(self, screen) -> None:
        theme = screen.theme
        surface = screen.surface
        screen.clear()

        # Draw status bar
        self.status_bar.draw(surface, pygame.Rect(0, 0, 640, 40), theme)

        history = self.app.history

        if not history:
            self._draw_empty(screen, theme, surface)
        else:
            self._draw_list(screen, theme, surface, history)

        # Draw footer
        self._draw_footer(screen, theme, surface, has_history=bool(history))

    def _draw_empty(self, screen, theme, surface):
        """Draw empty state message."""
        msg = "No calculations yet"
        font = _get_font("mono", 18)
        text_surface = font.render(msg, True, theme.text_dim)
        tx = (640 - text_surface.get_width()) // 2
        ty = 200
        surface.blit(text_surface, (tx, ty))

        hint = "Press B to go back"
        hint_font = _get_font("mono", 14)
        hint_surface = hint_font.render(hint, True, theme.text_dim)
        hx = (640 - hint_surface.get_width()) // 2
        surface.blit(hint_surface, (hx, ty + 36))

    def _draw_list(self, screen, theme, surface, history):
        """Draw the scrollable history list."""
        start_y = self.content_y + 6
        x_pad = 12
        card_w = 616

        for i in range(self.scroll_offset, min(len(history), self.scroll_offset + self.items_visible + 1)):
            entry = history[i]
            local_idx = i - self.scroll_offset
            y = start_y + local_idx * (self.item_h + self.item_gap)

            # Check if this item is within visible bounds
            if y + self.item_h > 444:
                break

            is_selected = (i == self.selected_index)
            rect = pygame.Rect(x_pad, y, card_w, self.item_h)

            # Draw card
            if is_selected:
                bg = theme.card_highlight
                border = theme.accent
            else:
                bg = theme.card_bg
                border = theme.card_border

            screen.draw_card(rect, bg=bg, border=border, radius=8, shadow=is_selected)

            # Format the expression for display
            expr_display = entry["expr"]
            expr_display = expr_display.replace("/", " \u00f7 ").replace("*", " \u00d7 ")
            result_display = entry["result"]

            # Draw expression
            expr_font = _get_font("mono", 16)
            expr_surface = expr_font.render(expr_display, True, theme.text)
            max_expr_w = 400
            if expr_surface.get_width() > max_expr_w:
                # Truncate from left
                crop_x = expr_surface.get_width() - max_expr_w
                expr_surface = expr_surface.subsurface(
                    pygame.Rect(crop_x, 0, max_expr_w, expr_surface.get_height())
                )
            surface.blit(expr_surface, (x_pad + 14, y + 10))

            # Draw "= result" right-aligned
            result_str = f"= {result_display}"
            result_font = _get_font("mono_bold", 16)
            result_surface = result_font.render(result_str, True, theme.text_accent)
            rw = result_surface.get_width()
            surface.blit(result_surface, (x_pad + card_w - 14 - rw, y + 10))

            # Draw index number (dim, bottom-left)
            idx_str = f"#{len(history) - i}"
            idx_font = _get_font("mono", 11)
            idx_surface = idx_font.render(idx_str, True, theme.text_dim)
            surface.blit(idx_surface, (x_pad + 14, y + 38))

        # Draw scroll indicators if needed
        if self.scroll_offset > 0:
            # Up arrow indicator
            arrow_font = _get_font("mono", 14)
            up_surface = arrow_font.render("\u25b2 more", True, theme.text_dim)
            surface.blit(up_surface, (290, self.content_y - 2))

        if self.scroll_offset + self.items_visible < len(history):
            # Down arrow indicator
            arrow_font = _get_font("mono", 14)
            down_surface = arrow_font.render("\u25bc more", True, theme.text_dim)
            surface.blit(down_surface, (290, 430))

    def _draw_footer(self, screen, theme, surface, has_history: bool):
        """Draw the footer with button hints."""
        y = 444
        pygame.draw.rect(surface, theme.bg_header, (0, y, 640, 36))
        pygame.draw.line(surface, theme.border, (0, y), (640, y))

        hx = 10
        if has_history:
            hx += screen.draw_button_hint("A", "Recall", hx, y + 8, btn_color=theme.btn_a) + 14
        hx += screen.draw_button_hint("B", "Back", hx, y + 8, btn_color=theme.btn_b) + 14
        if has_history:
            hx += screen.draw_button_hint("X", "Clear All", hx, y + 8, btn_color=theme.btn_x) + 14
