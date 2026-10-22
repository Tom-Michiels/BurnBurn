//! Feeder planning (FE01, G5-cam): repeat, feed before, feed after and pause around a compiled
//! plan. The result is another frozen plan; the original is never touched.

#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::{FeederError, FeederSettings, compile, plan_feeder};
use burnburn_job::{JobPlan, PlanEvent, preview};
use burnburn_types::{CancellationSource, CancellationToken, Length};
use common::*;

fn settings(repeat: u32, before: f64, after: f64, pause: f64, speed: f64) -> FeederSettings {
    FeederSettings::new(repeat, Length::from_mm(before).unwrap(), Length::from_mm(after).unwrap(), pause, speed)
        .unwrap()
}

fn base_plan() -> JobPlan {
    let mut s = session();
    let g = rectangle(&mut s, 100.0, 100.0, 50.0, 50.0);
    add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![g]);
    compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap()
}

/// The feeder events of `plan` in order: `("feed", mm)` and `("pause", s)`.
fn feeder_events(plan: &JobPlan) -> Vec<(&'static str, f64)> {
    plan.events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::Feed { mm, .. } => Some(("feed", mm)),
            PlanEvent::Pause { seconds } => Some(("pause", seconds)),
            _ => None,
        })
        .collect()
}

#[test]
fn repeat_feeds_before_and_after_and_pauses_between_the_repeats() {
    let base = base_plan();
    let cancel = CancellationToken::never();
    let fed = plan_feeder(&base, &settings(3, 20.0, 30.0, 2.0, 50.0), &cancel).unwrap();

    // Three copies of the job, each wrapped in its feeds; a pause only between repeats.
    assert_eq!(
        feeder_events(&fed),
        vec![
            ("feed", 20.0),
            ("feed", 30.0),
            ("pause", 2.0),
            ("feed", 20.0),
            ("feed", 30.0),
            ("pause", 2.0),
            ("feed", 20.0),
            ("feed", 30.0)
        ]
    );
    // The motion of every repeat is exactly the motion of the original.
    let want = motion(base.events());
    let got = motion(fed.events());
    assert_eq!(got.len(), 3 * want.len());
    for (k, chunk) in got.chunks(want.len()).enumerate() {
        assert_eq!(chunk, want.as_slice(), "repeat {k}");
    }
    // The job inside each wrapper starts with its own operation boundary.
    let boundaries = fed.events().iter().filter(|e| matches!(e, PlanEvent::OperationBoundary { .. })).count();
    assert_eq!(boundaries, 3);
    assert_eq!(fed.settings(), base.settings());
    assert_eq!(fed.source(), base.source());
    assert_ne!(fed.id(), base.id(), "another content, another plan");
}

#[test]
fn the_estimate_adds_every_feed_and_pause_to_every_repeat() {
    let base = base_plan();
    let once = preview(&base, &profile());
    let one = once.estimated_seconds;
    let fed = plan_feeder(&base, &settings(3, 20.0, 30.0, 2.0, 50.0), &CancellationToken::never()).unwrap();
    let pv = preview(&fed, &profile());
    // 3 jobs, 3 × (20 + 30) mm at 50 mm/s, 2 pauses of 2 s. The head ends a job where the next
    // one begins to cut, so only the first job has the approach travel (400 mm/s).
    let approach = once.travel_length_mm / 400.0;
    let want = 3.0 * one - 2.0 * approach + 3.0 * 50.0 / 50.0 + 2.0 * 2.0;
    assert!((pv.estimated_seconds - want).abs() < 1e-9, "{} vs {want}", pv.estimated_seconds);
    assert_eq!((pv.feed_length_mm, pv.pause_seconds), (150.0, 4.0));
    assert!((pv.cut_length_mm - 3.0 * preview(&base, &profile()).cut_length_mm).abs() < 1e-9);
}

#[test]
fn a_plain_setting_changes_nothing_not_even_the_plan_id() {
    let base = base_plan();
    let same = plan_feeder(&base, &FeederSettings::default(), &CancellationToken::never()).unwrap();
    assert_eq!(same.id(), base.id());
    assert_eq!(same.events(), base.events());
}

#[test]
fn a_single_repeat_with_a_pause_needs_no_pause() {
    let fed = plan_feeder(&base_plan(), &settings(1, 0.0, 15.0, 9.0, 40.0), &CancellationToken::never()).unwrap();
    assert_eq!(feeder_events(&fed), vec![("feed", 15.0)]);
}

#[test]
fn settings_outside_their_range_are_refused_with_the_field_named() {
    let mm = |v| Length::from_mm(v).unwrap();
    let field = |r: Result<FeederSettings, FeederError>| match r {
        Err(FeederError::OutOfRange { field }) => field,
        other => panic!("{other:?}"),
    };
    assert_eq!(field(FeederSettings::new(0, mm(0.0), mm(0.0), 0.0, 50.0)), "repeat");
    assert_eq!(field(FeederSettings::new(1001, mm(0.0), mm(0.0), 0.0, 50.0)), "repeat");
    assert_eq!(field(FeederSettings::new(2, mm(-1.0), mm(0.0), 0.0, 50.0)), "feed before");
    assert_eq!(field(FeederSettings::new(2, mm(0.0), mm(10_001.0), 0.0, 50.0)), "feed after");
    assert_eq!(field(FeederSettings::new(2, mm(0.0), mm(0.0), -1.0, 50.0)), "pause");
    assert_eq!(field(FeederSettings::new(2, mm(0.0), mm(0.0), 3601.0, 50.0)), "pause");
    assert_eq!(field(FeederSettings::new(2, mm(0.0), mm(0.0), f64::NAN, 50.0)), "pause");
    assert_eq!(field(FeederSettings::new(2, mm(0.0), mm(0.0), 0.0, 0.0)), "feed speed");
    assert_eq!(field(FeederSettings::new(2, mm(0.0), mm(0.0), 0.0, 5000.0)), "feed speed");
}

#[test]
fn a_plan_that_would_grow_beyond_the_bound_is_refused_before_it_is_built() {
    // 20 000 squares are well over 10 000 events; 1000 repeats of that is beyond the bound.
    let mut s = session();
    let all: Vec<_> =
        (0..4000).map(|i| rectangle(&mut s, f64::from(i % 100) * 10.0, f64::from(i / 100) * 10.0, 5.0, 5.0)).collect();
    add_op(&mut s, line_op("x", 100.0, 20.0), all);
    let base = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    assert!(base.events().len() > 10_000);
    let r = plan_feeder(&base, &settings(1000, 1.0, 0.0, 0.0, 50.0), &CancellationToken::never());
    assert!(matches!(r, Err(FeederError::TooLarge { .. })), "{r:?}");
}

#[test]
fn a_cancelled_feeder_plan_returns_cancelled() {
    let source = CancellationSource::new();
    source.cancel();
    let r = plan_feeder(&base_plan(), &settings(5, 1.0, 1.0, 0.0, 50.0), &source.token());
    assert!(matches!(r, Err(FeederError::Cancelled)), "{r:?}");
}
