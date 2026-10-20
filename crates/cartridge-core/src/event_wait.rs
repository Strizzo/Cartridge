//! Input-aware pacing: keep idle update rates low without sleeping through a
//! button press. SDL owns the wait; the event that woke it is delivered once.
use sdl2::{EventPump, event::Event};
use std::time::Duration;

#[derive(Default)]
pub struct EventInbox {
    pending: Option<Event>,
    events: Vec<Event>,
}

impl EventInbox {
    /// Reuses the event buffer and preserves ordering across the pacing wait.
    pub fn collect(&mut self, pump: &mut EventPump) -> &[Event] {
        self.events.clear();
        self.events.extend(self.pending.take());
        self.events.extend(pump.poll_iter());
        &self.events
    }

    /// Wait for an event or the next update deadline, whichever happens first.
    /// The next collect includes the event returned by SDL_WaitEventTimeout.
    pub fn wait(&mut self, pump: &mut EventPump, remaining: Duration) {
        if remaining.is_zero() || self.pending.is_some() {
            return;
        }
        let ms = remaining
            .as_nanos()
            .div_ceil(1_000_000)
            .min(i32::MAX as u128) as u32;
        self.pending = pump.wait_event_timeout(ms);
    }
}
