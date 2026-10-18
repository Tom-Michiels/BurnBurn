use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use burnburn_types::{Clock, MonotonicTime};

/// A [`Clock`] that only moves when a test calls [`FakeClock::advance`].
///
/// Clones share one time source, so a test can keep a handle while the code under test owns
/// another (or an `Arc<dyn Clock>`).
#[derive(Debug, Clone, Default)]
pub struct FakeClock {
    now: Arc<Mutex<MonotonicTime>>,
}

impl FakeClock {
    /// A clock at [`MonotonicTime::ZERO`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves the shared time forward by `by` and returns the new time.
    ///
    /// Saturates at the largest representable time instead of wrapping.
    pub fn advance(&self, by: Duration) -> MonotonicTime {
        let mut now = self.now.lock().unwrap_or_else(PoisonError::into_inner);
        *now = now.checked_add(by).unwrap_or(MonotonicTime::from_offset(Duration::MAX));
        *now
    }
}

impl Clock for FakeClock {
    fn now(&self) -> MonotonicTime {
        *self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
