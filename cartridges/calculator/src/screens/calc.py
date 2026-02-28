"""
Main calculator screen with expression display and button grid.
"""

import pygame
from cartridge_sdk.input import Button, InputEvent
from cartridge_sdk.screen import _get_font
from cartridge_sdk.ui import StatusBar

import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from engine import evaluate, format_result, try_evaluate, DivisionByZeroError, ParseError


# Button types for coloring
TYPE_DIGIT = "digit"
TYPE_OP = "op"
TYPE_EQUAL = "equal"
TYPE_CLEAR = "clear"
TYPE_FUNC = "func"
TYPE_DEL = "del"
TYPE_EMPTY = "empty"


class CalcButton:
    """Represents a single button in the calculator grid."""

    def __init__(self, label: str, action: str, btn_type: str = TYPE_DIGIT):
        self.label = label
        self.action = action  # what to insert/do
        self.btn_type = btn_type


# Define the 5x5 grid layout
GRID = [
    [
        CalcButton("C", "clear", TYPE_CLEAR),
        CalcButton("AC", "allclear", TYPE_CLEAR),
        CalcButton("%", "%", TYPE_FUNC),
        CalcButton("(", "(", TYPE_FUNC),
        CalcButton(")", ")", TYPE_FUNC),
    ],
    [
        CalcButton("7", "7"),
        CalcButton("8", "8"),
        CalcButton("9", "9"),
        CalcButton("\u00f7", "/", TYPE_OP),
        CalcButton("DEL", "del", TYPE_DEL),
    ],
    [
        CalcButton("4", "4"),
        CalcButton("5", "5"),
        CalcButton("6", "6"),
        CalcButton("\u00d7", "*", TYPE_OP),
        CalcButton("", "", TYPE_EMPTY),
    ],
    [
        CalcButton("1", "1"),
        CalcButton("2", "2"),
        CalcButton("3", "3"),
        CalcButton("\u2212", "-", TYPE_OP),
        CalcButton("", "", TYPE_EMPTY),
    ],
    [
        CalcButton("0", "0"),
        CalcButton(".", "."),
        CalcButton("+/\u2212", "negate", TYPE_FUNC),
        CalcButton("+", "+", TYPE_OP),
        CalcButton("=", "equals", TYPE_EQUAL),
    ],
]

GRID_ROWS = len(GRID)
GRID_COLS = len(GRID[0])


class CalcScreen:
    """Main calculator screen with expression display and navigable button grid."""

    def __init__(self, app):
        self.app = app
        self.status_bar = StatusBar("Calculator")

        # Expression state
        self.expression = ""
        self.result_text = ""
        self.error_text = ""
        self.last_result = ""

        # Grid cursor position
        self.cursor_row = 4  # Start on the "0" row
        self.cursor_col = 0

        # Animation
        self._flash_alpha = 0
        self._flash_timer = 0

    def handle_input(self, event: InputEvent) -> None:
        if event.action not in ("press", "repeat"):
            return

        if event.button == Button.DPAD_UP:
            self._move_cursor(-1, 0)
        elif event.button == Button.DPAD_DOWN:
            self._move_cursor(1, 0)
        elif event.button == Button.DPAD_LEFT:
            self._move_cursor(0, -1)
        elif event.button == Button.DPAD_RIGHT:
            self._move_cursor(0, 1)
        elif event.button == Button.A:
            self._press_button()
        elif event.button == Button.B:
            self._backspace()
        elif event.button == Button.Y:
            self.app.push_screen("history")

    def _move_cursor(self, drow: int, dcol: int) -> None:
        new_row = self.cursor_row + drow
        new_col = self.cursor_col + dcol

        # Wrap vertically
        if new_row < 0:
            new_row = GRID_ROWS - 1
        elif new_row >= GRID_ROWS:
            new_row = 0

        # Wrap horizontally
        if new_col < 0:
            new_col = GRID_COLS - 1
        elif new_col >= GRID_COLS:
            new_col = 0

        # Skip empty buttons
        btn = GRID[new_row][new_col]
        if btn.btn_type == TYPE_EMPTY:
            # Try to keep moving in the same direction
            if drow != 0:
                self.cursor_row = new_row
                self._move_cursor(drow, 0)
                return
            elif dcol != 0:
                self.cursor_col = new_col
                self._move_cursor(0, dcol)
                return
            return

        self.cursor_row = new_row
        self.cursor_col = new_col

    def _press_button(self) -> None:
        btn = GRID[self.cursor_row][self.cursor_col]
        if btn.btn_type == TYPE_EMPTY:
            return

        action = btn.action
        self.error_text = ""

        if action == "clear":
            # C: Clear last entry (backspace to last operator or clear all)
            self._clear_last_entry()
        elif action == "allclear":
            self.expression = ""
            self.result_text = ""
            self.error_text = ""
        elif action == "del":
            self._backspace()
        elif action == "negate":
            self._toggle_negate()
        elif action == "equals":
            self._evaluate()
        else:
            # Insert character(s) into expression
            self._insert(action)

        # Update live preview
        self._update_preview()

    def _insert(self, text: str) -> None:
        """Insert text into the expression with smart handling."""
        self.expression += text

    def _backspace(self) -> None:
        """Remove the last character from the expression."""
        if self.expression:
            self.expression = self.expression[:-1]
            self.error_text = ""
            self._update_preview()

    def _clear_last_entry(self) -> None:
        """Clear back to the last operator, or clear everything."""
        if not self.expression:
            return
        # Find the last operator
        ops = set("+-*/(")
        i = len(self.expression) - 1
        # Skip trailing operators
        while i >= 0 and self.expression[i] in ops:
            i -= 1
        # Find previous operator
        while i >= 0 and self.expression[i] not in ops:
            i -= 1
        if i >= 0:
            self.expression = self.expression[: i + 1]
        else:
            self.expression = ""

    def _toggle_negate(self) -> None:
        """Toggle negation on the current number or insert minus for negation."""
        expr = self.expression
        if not expr:
            self.expression = "-"
            return

        # Find the start of the current number
        i = len(expr) - 1
        while i >= 0 and (expr[i].isdigit() or expr[i] == "."):
            i -= 1

        # Check if preceded by a negative sign that is a negation
        if i >= 0 and expr[i] == "-":
            # Check if this minus is a negation (preceded by operator or start)
            if i == 0 or expr[i - 1] in "+-*/(":
                # Remove the negation
                self.expression = expr[:i] + expr[i + 1 :]
                return

        # Insert negation
        if i >= 0 and expr[i] in "+-*/(%":
            self.expression = expr[: i + 1] + "-" + expr[i + 1 :]
        elif i < 0:
            # Entire expression is a number
            if expr.startswith("-"):
                self.expression = expr[1:]
            else:
                self.expression = "-" + expr
        else:
            # Last char is something else, just insert minus
            self.expression += "-"

    def _evaluate(self) -> None:
        """Evaluate the current expression and show result."""
        if not self.expression:
            return

        display_expr = self.expression.replace("/", "\u00f7").replace("*", "\u00d7")

        try:
            result = evaluate(self.expression)
            result_str = format_result(result)
            self.result_text = result_str
            self.error_text = ""

            # Add to history
            self.app.add_history(self.expression, result_str)

            # Replace expression with result for chaining
            self.last_result = result_str
            self.expression = result_str

        except DivisionByZeroError:
            self.error_text = "Cannot divide by zero"
            self.result_text = ""
        except ParseError:
            self.error_text = "Invalid expression"
            self.result_text = ""
        except Exception:
            self.error_text = "Error"
            self.result_text = ""

    def _update_preview(self) -> None:
        """Update the live result preview."""
        if not self.expression:
            self.result_text = ""
            self.error_text = ""
            return

        preview = try_evaluate(self.expression)
        if preview is not None:
            self.result_text = preview
            self.error_text = ""
        else:
            # Don't clear previous result, just show nothing new
            self.result_text = ""

    def set_expression(self, expr: str) -> None:
        """Set the expression (e.g., from history recall)."""
        self.expression = expr
        self.error_text = ""
        self._update_preview()

    def draw(self, screen) -> None:
        theme = screen.theme
        surface = screen.surface
        screen.clear()

        # Draw status bar
        self.status_bar.draw(surface, pygame.Rect(0, 0, 640, 40), theme)

        # Draw expression display area
        self._draw_display(screen, theme, surface)

        # Draw button grid
        self._draw_grid(screen, theme, surface)

        # Draw footer
        self._draw_footer(screen, theme, surface)

    def _draw_display(self, screen, theme, surface):
        """Draw the expression display area at the top."""
        display_y = 44
        display_h = 116
        display_rect = pygame.Rect(10, display_y, 620, display_h)

        # Background card
        screen.draw_card(display_rect, bg=theme.card_bg, border=theme.card_border, radius=10, shadow=True)

        # Format expression for display (replace operators with symbols)
        display_expr = self.expression
        display_expr = display_expr.replace("/", " \u00f7 ")
        display_expr = display_expr.replace("*", " \u00d7 ")
        # Add spaces around + and - but not for negative numbers
        formatted = ""
        i = 0
        raw = self.expression
        while i < len(raw):
            ch = raw[i]
            if ch in "+-" and i > 0 and raw[i - 1] not in "+-*/(":
                formatted += f" {ch} "
            else:
                formatted += ch
            i += 1
        display_expr = formatted.replace("/", " \u00f7 ").replace("*", " \u00d7 ")

        if not display_expr:
            display_expr = "0"
            expr_color = theme.text_dim
        else:
            expr_color = theme.text

        # Draw expression (right-aligned, large font)
        expr_font_size = 28
        expr_font = _get_font("mono_bold", expr_font_size)

        # Measure text width and truncate from left if needed
        max_expr_width = 596
        text_surface = expr_font.render(display_expr, True, expr_color)
        tw = text_surface.get_width()

        if tw > max_expr_width:
            # Truncate from the left: show the rightmost portion
            # Render full, then crop
            crop_x = tw - max_expr_width
            cropped = text_surface.subsurface(pygame.Rect(crop_x, 0, max_expr_width, text_surface.get_height()))
            expr_x = 22
            surface.blit(cropped, (expr_x, display_y + 16))
        else:
            expr_x = 10 + 620 - 12 - tw
            surface.blit(text_surface, (expr_x, display_y + 16))

        # Draw result preview or error below expression
        preview_y = display_y + 60
        if self.error_text:
            screen.draw_text(
                self.error_text, 608, preview_y,
                color=theme.negative, font_size=20, bold=False
            )
            # Right-align the error manually
            ew = screen.get_text_width(self.error_text, 20, False)
            # Redraw right-aligned
            pygame.draw.rect(surface, theme.card_bg, (22, preview_y, 596, 30))
            err_font = _get_font("mono", 20)
            err_surface = err_font.render(self.error_text, True, theme.negative)
            surface.blit(err_surface, (618 - ew, preview_y))
        elif self.result_text and self.expression:
            # Show "= result" in dim accent color
            preview_str = f"= {self.result_text}"
            prev_font = _get_font("mono", 20)
            prev_surface = prev_font.render(preview_str, True, theme.text_dim)
            pw = prev_surface.get_width()
            surface.blit(prev_surface, (618 - pw, preview_y))

        # Subtle divider line in display
        line_y = display_y + 55
        pygame.draw.line(
            surface, (50, 50, 70),
            (22, line_y), (618, line_y)
        )

    def _draw_grid(self, screen, theme, surface):
        """Draw the calculator button grid."""
        grid_y_start = 168
        grid_x_start = 12
        btn_w = 120
        btn_h = 52
        gap_x = 5
        gap_y = 5

        for row_idx in range(GRID_ROWS):
            for col_idx in range(GRID_COLS):
                btn = GRID[row_idx][col_idx]
                if btn.btn_type == TYPE_EMPTY:
                    continue

                x = grid_x_start + col_idx * (btn_w + gap_x)
                y = grid_y_start + row_idx * (btn_h + gap_y)
                rect = pygame.Rect(x, y, btn_w, btn_h)

                is_selected = (row_idx == self.cursor_row and col_idx == self.cursor_col)

                # Determine colors based on button type
                bg, text_color, border_color = self._get_button_colors(btn.btn_type, is_selected, theme)

                # Draw button background
                if is_selected:
                    # Draw selection glow (slightly larger rect behind)
                    glow_rect = rect.inflate(4, 4)
                    pygame.draw.rect(surface, theme.accent, glow_rect, border_radius=10)

                pygame.draw.rect(surface, bg, rect, border_radius=8)

                # Draw border
                if border_color:
                    pygame.draw.rect(surface, border_color, rect, width=2, border_radius=8)

                # Draw label centered
                label = btn.label
                font_size = 20
                if label in ("DEL", "AC"):
                    font_size = 16
                elif label in ("+/\u2212",):
                    font_size = 16

                font = _get_font("mono_bold", font_size)
                text_surf = font.render(label, True, text_color)
                tx = x + (btn_w - text_surf.get_width()) // 2
                ty = y + (btn_h - text_surf.get_height()) // 2
                surface.blit(text_surf, (tx, ty))

    def _get_button_colors(self, btn_type, is_selected, theme):
        """Return (bg_color, text_color, border_color) for a button."""
        if is_selected:
            if btn_type == TYPE_EQUAL:
                return (60, 170, 60), (255, 255, 255), theme.accent
            elif btn_type == TYPE_OP:
                return (50, 85, 160), (255, 255, 255), theme.accent
            elif btn_type == TYPE_CLEAR:
                return (160, 50, 50), (255, 255, 255), theme.accent
            elif btn_type == TYPE_DEL:
                return (140, 80, 40), (255, 255, 255), theme.accent
            elif btn_type == TYPE_FUNC:
                return (55, 65, 95), (255, 255, 255), theme.accent
            else:
                return theme.card_highlight, (255, 255, 255), theme.accent

        # Not selected
        if btn_type == TYPE_EQUAL:
            return (45, 140, 45), (255, 255, 255), (55, 160, 55)
        elif btn_type == TYPE_OP:
            return (40, 65, 130), (180, 210, 255), (50, 80, 155)
        elif btn_type == TYPE_CLEAR:
            return (130, 40, 40), (255, 180, 180), (155, 50, 50)
        elif btn_type == TYPE_DEL:
            return (110, 65, 30), (255, 200, 150), (135, 80, 40)
        elif btn_type == TYPE_FUNC:
            return (38, 48, 72), (160, 190, 230), (50, 62, 90)
        else:
            # Digit
            return theme.card_bg, theme.text, theme.card_border

    def _draw_footer(self, screen, theme, surface):
        """Draw the footer with button hints."""
        y = 444
        pygame.draw.rect(surface, theme.bg_header, (0, y, 640, 36))
        pygame.draw.line(surface, theme.border, (0, y), (640, y))

        hx = 10
        hx += screen.draw_button_hint("A", "Press", hx, y + 8, btn_color=theme.btn_a) + 14
        hx += screen.draw_button_hint("B", "Delete", hx, y + 8, btn_color=theme.btn_b) + 14
        hx += screen.draw_button_hint("Y", "History", hx, y + 8, btn_color=theme.btn_y) + 14
