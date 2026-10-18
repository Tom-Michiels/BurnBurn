use std::sync::Arc;
use std::time::Duration;

use burnburn_test_support::FakeClock;
use burnburn_types::{Clock, MonotonicTime};

fn at(ms: u64) -> MonotonicTime {
    MonotonicTime::from_offset(Duration::from_millis(ms))
}

#[test]
fn fake_clock_stands_still_until_advanced() {
    let clock = FakeClock::new();
    assert_eq!(clock.now(), MonotonicTime::ZERO);
    assert_eq!(clock.now(), MonotonicTime::ZERO);
    assert_eq!(clock.advance(Duration::from_millis(250)), at(250));
    assert_eq!(clock.now(), at(250));
    clock.advance(Duration::from_millis(50));
    assert_eq!(clock.now(), at(300));
}

#[test]
fn fake_clock_clones_and_arc_dyn_clock_share_time() {
    let test_handle = FakeClock::new();
    let owned_by_code_under_test: Arc<dyn Clock> = Arc::new(test_handle.clone());
    test_handle.advance(Duration::from_secs(2));
    assert_eq!(owned_by_code_under_test.now(), at(2000));
}

#[test]
fn fake_clock_saturates_instead_of_wrapping() {
    let clock = FakeClock::new();
    clock.advance(Duration::from_secs(10));
    clock.advance(Duration::MAX);
    let max = clock.now();
    assert!(max >= at(10_000), "time went backwards on overflow: {max:?}");
    clock.advance(Duration::from_secs(1));
    assert_eq!(clock.now(), max);
}
