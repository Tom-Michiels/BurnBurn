use std::time::Duration;

/// A point on a monotonic clock, as an offset from an arbitrary per-clock origin.
///
/// It never comes from wall-clock time, so tests can drive it deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MonotonicTime(Duration);

impl MonotonicTime {
    pub const ZERO: MonotonicTime = MonotonicTime(Duration::ZERO);

    pub fn from_offset(offset: Duration) -> Self {
        MonotonicTime(offset)
    }

    pub fn offset(self) -> Duration {
        self.0
    }

    /// Time elapsed since `earlier`; zero when `earlier` is later.
    pub fn saturating_since(self, earlier: MonotonicTime) -> Duration {
        self.0.saturating_sub(earlier.0)
    }

    pub fn checked_add(self, d: Duration) -> Option<Self> {
        self.0.checked_add(d).map(MonotonicTime)
    }
}

/// The injected clock port. Production uses a monotonic OS clock adapter (`burnburn-runtime`);
/// tests use `burnburn-test-support::FakeClock`.
pub trait Clock: Send + Sync {
    fn now(&self) -> MonotonicTime;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_never_goes_negative() {
        let a = MonotonicTime::from_offset(Duration::from_millis(5));
        let b = MonotonicTime::from_offset(Duration::from_millis(9));
        assert_eq!(b.saturating_since(a), Duration::from_millis(4));
        assert_eq!(a.saturating_since(b), Duration::ZERO);
    }
}
