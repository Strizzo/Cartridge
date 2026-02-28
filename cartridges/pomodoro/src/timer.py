"""Pomodoro timer engine - state machine for work/break cycles."""

import enum
import time
from datetime import datetime


class Phase(enum.Enum):
    WORK = "work"
    SHORT_BREAK = "short_break"
    LONG_BREAK = "long_break"


PHASE_LABELS = {
    Phase.WORK: "Work",
    Phase.SHORT_BREAK: "Short Break",
    Phase.LONG_BREAK: "Long Break",
}

PHASE_COLORS = {
    Phase.WORK: (220, 80, 80),
    Phase.SHORT_BREAK: (80, 200, 120),
    Phase.LONG_BREAK: (80, 140, 240),
}

PHASE_COLORS_DIM = {
    Phase.WORK: (110, 40, 40),
    Phase.SHORT_BREAK: (40, 100, 60),
    Phase.LONG_BREAK: (40, 70, 120),
}

WORK_PRESETS = [15, 20, 25, 30, 45]
SHORT_BREAK_PRESETS = [3, 5, 10]
LONG_BREAK_PRESETS = [10, 15, 20, 30]


class TimerEngine:
    """State machine that manages pomodoro timer phases and transitions."""

    def __init__(self):
        self.phase: Phase = Phase.WORK
        self.running: bool = False
        self.work_count: int = 0  # completed work sessions in current cycle (0-3)
        self.total_completed: int = 0  # total work sessions completed today

        # Durations in seconds
        self.durations = {
            Phase.WORK: 25 * 60,
            Phase.SHORT_BREAK: 5 * 60,
            Phase.LONG_BREAK: 15 * 60,
        }
        self.remaining: float = self.durations[Phase.WORK]

        # Stats tracking
        self.total_focus_seconds: int = 0
        self.sessions: list[dict] = []
        self._session_start: str | None = None
        self._session_elapsed: float = 0.0

        # Callbacks
        self.on_phase_complete: callable = None
        self.on_work_complete: callable = None

    @property
    def duration(self) -> float:
        """Total duration for the current phase."""
        return self.durations[self.phase]

    @property
    def progress(self) -> float:
        """Progress from 0.0 to 1.0 (1.0 = timer finished)."""
        d = self.duration
        if d <= 0:
            return 1.0
        elapsed = d - self.remaining
        return max(0.0, min(1.0, elapsed / d))

    @property
    def phase_label(self) -> str:
        return PHASE_LABELS[self.phase]

    @property
    def phase_color(self) -> tuple:
        return PHASE_COLORS[self.phase]

    @property
    def phase_color_dim(self) -> tuple:
        return PHASE_COLORS_DIM[self.phase]

    def format_time(self) -> str:
        """Format remaining time as MM:SS."""
        total = max(0, int(self.remaining))
        minutes = total // 60
        seconds = total % 60
        return f"{minutes:02d}:{seconds:02d}"

    def update(self, dt: float) -> None:
        """Tick the timer. Called every frame with dt in seconds."""
        if not self.running:
            return

        self.remaining -= dt

        # Track focus time for work phases
        if self.phase == Phase.WORK:
            self._session_elapsed += dt

        if self.remaining <= 0:
            self.remaining = 0
            self._advance_phase()

    def toggle(self) -> None:
        """Start or pause the timer."""
        if not self.running:
            # Starting
            if self.phase == Phase.WORK and self._session_start is None:
                self._session_start = datetime.now().strftime("%H:%M")
                self._session_elapsed = 0.0
            self.running = True
        else:
            self.running = False

    def reset(self) -> None:
        """Reset the current phase timer."""
        self.remaining = self.durations[self.phase]
        self.running = False
        if self.phase == Phase.WORK:
            self._session_start = None
            self._session_elapsed = 0.0

    def skip(self) -> None:
        """Skip to the next phase."""
        was_work = self.phase == Phase.WORK
        if was_work and self._session_start is not None:
            # Record as skipped
            self.sessions.append({
                "start": self._session_start,
                "duration_min": round(self._session_elapsed / 60, 1),
                "completed": False,
            })
            self._session_start = None
            self._session_elapsed = 0.0
        self._advance_phase()

    def _advance_phase(self) -> None:
        """Move to the next phase in the cycle."""
        if self.phase == Phase.WORK:
            # Completed a work session
            self.work_count += 1
            self.total_completed += 1
            focus_secs = int(self._session_elapsed)
            self.total_focus_seconds += focus_secs

            # Record completed session
            if self._session_start is not None:
                self.sessions.append({
                    "start": self._session_start,
                    "duration_min": round(self.durations[Phase.WORK] / 60, 1),
                    "completed": True,
                })
            self._session_start = None
            self._session_elapsed = 0.0

            if self.on_work_complete:
                self.on_work_complete()

            # Decide next break type
            if self.work_count >= 4:
                self.phase = Phase.LONG_BREAK
                self.work_count = 0
            else:
                self.phase = Phase.SHORT_BREAK
        else:
            # Break finished, back to work
            self.phase = Phase.WORK

        self.remaining = self.durations[self.phase]
        self.running = False

        if self.on_phase_complete:
            self.on_phase_complete()

    def set_duration(self, phase: Phase, minutes: int) -> None:
        """Update a phase duration. If currently in that phase and not running, reset."""
        self.durations[phase] = minutes * 60
        if self.phase == phase and not self.running:
            self.remaining = self.durations[phase]

    def get_stats_dict(self) -> dict:
        """Get stats as a serializable dict for storage."""
        return {
            "today": datetime.now().strftime("%Y-%m-%d"),
            "completed": self.total_completed,
            "total_focus_seconds": self.total_focus_seconds,
            "sessions": list(self.sessions),
            "work_count": self.work_count,
        }

    def load_stats(self, data: dict) -> None:
        """Restore stats from storage. Resets if date has changed."""
        if not data:
            return
        today = datetime.now().strftime("%Y-%m-%d")
        if data.get("today") != today:
            # New day, reset stats
            return
        self.total_completed = data.get("completed", 0)
        self.total_focus_seconds = data.get("total_focus_seconds", 0)
        self.sessions = data.get("sessions", [])
        self.work_count = data.get("work_count", 0)
