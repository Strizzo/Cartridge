"""
Calculator App - Main entry point.
A calculator with expression display and history for the Cartridge handheld platform.
"""

from cartridge_sdk import CartridgeApp, AppContext, Screen, InputEvent
from cartridge_sdk.input import Button, InputEvent

import sys
import os

# Ensure local imports work
sys.path.insert(0, os.path.dirname(__file__))

from screens.calc import CalcScreen
from screens.history import HistoryScreen


MAX_HISTORY = 20


class CalculatorApp(CartridgeApp):
    """Main calculator application."""

    async def on_init(self, ctx: AppContext) -> None:
        await super().on_init(ctx)
        self.ctx = ctx

        # Screen stack for navigation
        self._screen_stack: list[str] = ["calc"]

        # History entries: list of {"expr": str, "result": str}
        self.history: list[dict] = []

        # Create screens
        self.calc_screen = CalcScreen(self)
        self.history_screen = HistoryScreen(self)

        # Load history from storage
        self._load_history()

    def _load_history(self) -> None:
        """Load calculation history from persistent storage."""
        try:
            data = self.ctx.storage.load("history")
            if data and isinstance(data, dict) and "entries" in data:
                entries = data["entries"]
                if isinstance(entries, list):
                    self.history = entries[:MAX_HISTORY]
        except Exception:
            self.history = []

    def _save_history(self) -> None:
        """Save calculation history to persistent storage."""
        try:
            self.ctx.storage.save("history", {"entries": self.history})
        except Exception:
            pass

    def add_history(self, expr: str, result: str) -> None:
        """Add a calculation to history."""
        # Don't add duplicates of the most recent entry
        if self.history and self.history[0]["expr"] == expr and self.history[0]["result"] == result:
            return

        entry = {"expr": expr, "result": result}
        self.history.insert(0, entry)

        # Trim to max size
        if len(self.history) > MAX_HISTORY:
            self.history = self.history[:MAX_HISTORY]

        self._save_history()

    def clear_history(self) -> None:
        """Clear all history entries."""
        self.history = []
        self._save_history()

    def push_screen(self, name: str) -> None:
        """Push a screen onto the navigation stack."""
        self._screen_stack.append(name)
        if name == "history":
            self.history_screen.on_enter()

    def pop_screen(self) -> None:
        """Pop the top screen from the navigation stack."""
        if len(self._screen_stack) > 1:
            self._screen_stack.pop()

    def on_input(self, event: InputEvent) -> None:
        """Route input to the active screen."""
        current = self._screen_stack[-1]
        if current == "calc":
            self.calc_screen.handle_input(event)
        elif current == "history":
            self.history_screen.handle_input(event)

    def on_render(self, screen: Screen) -> None:
        """Render the active screen."""
        current = self._screen_stack[-1]
        if current == "calc":
            self.calc_screen.draw(screen)
        elif current == "history":
            self.history_screen.draw(screen)
