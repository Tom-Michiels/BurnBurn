//! OS clocks. [`SystemClock`] is the only production [`Clock`]: monotonic, with a fixed origin
//! at construction, never wall time (ADR 0005 §5).

use std::time::Instant;

use burnburn_types::{Clock, MonotonicTime};

/// Monotonic OS clock over `std::time::Instant`. Cloning shares the origin.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        SystemClock { origin: Instant::now() }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> MonotonicTime {
        MonotonicTime::from_offset(self.origin.elapsed())
    }
}
