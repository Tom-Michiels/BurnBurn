//! Tab Cut Power in G-code (C04, G7-cam): a cut at its own power is one `G1` with its own `S`
//! word. Checked with the independent interpreter of `tests/support`.
#![allow(clippy::unwrap_used, clippy::panic)]

mod support;

use burnburn_gcode::{GrblProfile, LowerError, RefusalKind, lower};
use burnburn_job::{JobPlan, PlanEvent, PlanMode, PowerRatio};
use support::{interp, plans};

fn profile() -> GrblProfile {
    GrblProfile::grbl_1_1_laser()
}

/// A 40 x 20 mm rectangle at 60 % whose right side (20 mm) is cut at `tab` of the rated power.
fn rectangle_with_tab(tab: f64) -> JobPlan {
    let mut b = plans::builder();
    let s = b.add_settings(plans::settings(PlanMode::Line, 20.0, 0.6, false)).unwrap();
    plans::boundary(&mut b, s, 1, 0);
    b.push(PlanEvent::Travel { to: plans::pt(10.0, 10.0) }).unwrap();
    b.push(PlanEvent::Cut { to: plans::pt(50.0, 10.0), origin: None }).unwrap();
    b.push(PlanEvent::CutPower { to: plans::pt(50.0, 30.0), power: PowerRatio::new(tab).unwrap(), origin: None })
        .unwrap();
    b.push(PlanEvent::Cut { to: plans::pt(10.0, 30.0), origin: None }).unwrap();
    b.push(PlanEvent::Cut { to: plans::pt(10.0, 10.0), origin: None }).unwrap();
    b.finish().unwrap()
}

#[test]
fn a_tab_is_burnt_with_its_own_s_word_and_the_rest_keeps_the_operation_power() {
    let text = lower(&rectangle_with_tab(0.15), &profile()).unwrap().to_text();
    let run = interp::run(&text).unwrap();
    // The whole perimeter burns: the tab is cut, not skipped.
    assert!((run.laser_on_length() - 120.0).abs() < 1e-9, "{text}");
    let by_side: Vec<(f64, f64)> = run.burn_segments().iter().map(|s| (s.length(), s.s)).collect();
    assert_eq!(by_side, vec![(40.0, 600.0), (20.0, 150.0), (40.0, 600.0), (20.0, 600.0)], "{text}");
    // Energy by hand: (40 + 40 + 20) mm at 0.6 plus 20 mm at 0.15, all at 20 mm/s.
    let energy = (100.0 * 0.6 + 20.0 * 0.15) / 20.0;
    assert!((run.energy_ratio_seconds(1000.0) - energy).abs() < 1e-9, "{}", run.energy_ratio_seconds(1000.0));
    // S is modal: the tab line states it, the next side states the operation power again.
    assert!(text.contains("G1 Y30 S150"), "{text}");
    assert!(text.contains("G1 X10 S600"), "{text}");
}

#[test]
fn the_spindle_scale_applies_to_the_tab_power_too() {
    let text = lower(&rectangle_with_tab(0.15), &GrblProfile { max_spindle: 255, ..profile() }).unwrap().to_text();
    // 0.15 * 255 = 38.25 -> 38; 0.6 * 255 = 153.
    assert!(text.contains("S38") && text.contains("S153"), "{text}");
}

#[test]
fn a_tab_power_that_rounds_to_zero_is_refused_instead_of_becoming_a_skipped_tab() {
    // 0.0004 of $30=1000 is S0 (GRBL laser mode: off): the tab would be skipped silently.
    let refused = lower(&rectangle_with_tab(0.0004), &profile()).unwrap_err();
    let LowerError::Refused(refusals) = refused else { panic!("expected a refusal") };
    assert!(refusals.iter().any(|r| r.kind == RefusalKind::NotRepresentable("tab power")), "{refusals:?}");
}

#[test]
fn a_tab_outside_the_bed_is_refused_like_any_cut() {
    let mut b = plans::builder();
    let s = b.add_settings(plans::settings(PlanMode::Line, 20.0, 0.6, false)).unwrap();
    plans::boundary(&mut b, s, 1, 0);
    b.push(PlanEvent::CutPower { to: plans::pt(5000.0, 0.0), power: PowerRatio::new(0.2).unwrap(), origin: None })
        .unwrap();
    let refused = lower(&b.finish().unwrap(), &profile()).unwrap_err();
    let LowerError::Refused(refusals) = refused else { panic!("expected a refusal") };
    assert!(refusals.iter().any(|r| r.kind == RefusalKind::OutsideBed), "{refusals:?}");
}
