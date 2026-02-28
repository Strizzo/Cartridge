"""Pomodoro Timer - A productivity timer with work/break cycles and session statistics."""

from cartridge_sdk import CartridgeApp, AppContext, Screen, InputEvent
from cartridge_sdk.input import Button, InputEvent

from .timer import TimerEngine
from .screens.timer_screen import TimerScreen
from .screens.stats import StatsScreen
from .screens.settings import SettingsScreen


class PomodoroApp(CartridgeApp):
    """Main pomodoro application with screen stack navigation."""

    async def on_init(self, ctx: AppContext) -> None:
        await super().on_init(ctx)

        # Timer engine (shared state)
        self.timer_engine = TimerEngine()

        # Load saved stats from storage
        self._load_stats()

        # Register callback to save stats when a work session completes
        self.timer_engine.on_work_complete = self._on_work_complete
        self.timer_engine.on_phase_complete = self._on_phase_complete

        # Screens
        self.timer_screen = TimerScreen(self)
        self.stats_screen = StatsScreen(self)
        self.settings_screen = SettingsScreen(self)

        # Navigation stack
        self._screen_stack: list[str] = ["timer"]

    def _load_stats(self) -> None:
        """Load stats from storage, reset if date changed."""
        data = self.ctx.storage.load("stats")
        if data:
            self.timer_engine.load_stats(data)

    def save_stats(self) -> None:
        """Persist current stats to storage."""
        self.ctx.storage.save("stats", self.timer_engine.get_stats_dict())

    def _on_work_complete(self) -> None:
        """Called when a work session completes."""
        self.save_stats()

    def _on_phase_complete(self) -> None:
        """Called when any phase transition happens."""
        self.save_stats()

    def push_screen(self, name: str) -> None:
        """Push a screen onto the navigation stack."""
        if self._screen_stack[-1] != name:
            self._screen_stack.append(name)
            # Sync settings screen when entering it
            if name == "settings":
                self.settings_screen._sync_from_engine()

    def pop_screen(self) -> None:
        """Pop the top screen, returning to the previous one."""
        if len(self._screen_stack) > 1:
            self._screen_stack.pop()

    def on_input(self, event: InputEvent) -> None:
        current = self._screen_stack[-1]
        if current == "timer":
            self.timer_screen.handle_input(event)
        elif current == "stats":
            self.stats_screen.handle_input(event)
        elif current == "settings":
            self.settings_screen.handle_input(event)

    async def on_update(self, dt: float) -> None:
        # Timer must tick every frame
        self.timer_engine.update(dt)

        # Update screen animations
        current = self._screen_stack[-1]
        if current == "timer":
            self.timer_screen.update(dt)

    def on_render(self, screen: Screen) -> None:
        current = self._screen_stack[-1]
        if current == "timer":
            self.timer_screen.draw(screen)
        elif current == "stats":
            self.stats_screen.draw(screen)
        elif current == "settings":
            self.settings_screen.draw(screen)
