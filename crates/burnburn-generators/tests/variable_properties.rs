//! Invariants of serial counters and the job clock over random inputs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_generators::{JobClock, OnEnd, SerialCounter, SerialSpec};
use proptest::prelude::*;

/// Days since 1970-01-01 of a civil date: the *forward* formula (Hinnant), independent of the
/// inverse the clock uses.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

proptest! {
    #[test]
    fn serial_values_stay_on_the_lattice_and_offsets_equal_repeated_next(
        start in -1000i64..1000, steps in 0i64..50, by in 1i64..9, down in any::<bool>(), k in 0u32..120, wrap in any::<bool>(),
    ) {
        let (by, end) = if down { (-by, start - steps * by) } else { (by, start + steps * by) };
        let on_end = if wrap { OnEnd::Wrap } else { OnEnd::Stop };
        let spec = SerialSpec::new(start, end, by, 0, on_end).unwrap();
        let mut c = SerialCounter::new(spec);
        let predicted = c.value_at_offset(i64::from(k));
        let mut stepped = Ok(());
        for _ in 0..k {
            stepped = c.advance();
            if stepped.is_err() { break; }
        }
        let (lo, hi) = (start.min(end), start.max(end));
        prop_assert!((lo..=hi).contains(&c.current()));
        prop_assert_eq!((c.current() - start) % by, 0);
        match predicted {
            Ok(v) => { prop_assert!(stepped.is_ok()); prop_assert_eq!(v, c.current()); }
            Err(_) => prop_assert!(stepped.is_err(), "an offset past a stopping end is refused like stepping to it"),
        }
    }

    #[test]
    fn the_clock_prints_a_date_that_maps_back_to_the_same_local_second(
        utc in -62_135_596_800i64 + 86_400..253_402_300_799 - 86_400, zone in -1439i32..1440,
    ) {
        let clock = JobClock::new(utc, zone).unwrap();
        let text = clock.format("%Y %m %d %H %M %S").unwrap();
        let v: Vec<i64> = text.split(' ').map(|p| p.parse().unwrap()).collect();
        let local = days_from_civil(v[0], v[1], v[2]) * 86_400 + v[3] * 3600 + v[4] * 60 + v[5];
        prop_assert_eq!(local, utc + i64::from(zone) * 60);
        prop_assert!((1..=12).contains(&v[1]) && (1..=31).contains(&v[2]) && v[3] < 24 && v[4] < 60 && v[5] < 60);
    }
}
