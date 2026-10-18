//! The monotonic OS clock adapter.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use burnburn_runtime::SystemClock;
use burnburn_types::Clock;

#[test]
fn system_clock_is_monotonic_and_advances() {
    let clock = SystemClock::new();
    let mut last = clock.now();
    for _ in 0..10_000 {
        let now = clock.now();
        assert!(now >= last, "clock went backwards: {now:?} < {last:?}");
        last = now;
    }
    let before = clock.now();
    std::thread::sleep(Duration::from_millis(20));
    let elapsed = clock.now().saturating_since(before);
    assert!(elapsed >= Duration::from_millis(20), "clock did not advance with real time: {elapsed:?}");
    assert!(elapsed < Duration::from_secs(5), "implausible jump: {elapsed:?}");
}

#[test]
fn clones_share_the_origin_across_threads() {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
    std::thread::sleep(Duration::from_millis(5));
    let c = Arc::clone(&clock);
    let from_thread = std::thread::spawn(move || c.now()).join().unwrap();
    let here = clock.now();
    assert!(from_thread >= Duration::from_millis(5).into_time(), "{from_thread:?}");
    assert!(here >= from_thread);
}

trait IntoTime {
    fn into_time(self) -> burnburn_types::MonotonicTime;
}

impl IntoTime for Duration {
    fn into_time(self) -> burnburn_types::MonotonicTime {
        burnburn_types::MonotonicTime::from_offset(self)
    }
}
