//! Lowering `JobPlan` -> G-code, checked against analytical fixtures and an independent
//! interpreter (`tests/support/interp.rs`). See `tests/fixtures/README.md` for fixture origins.
#![allow(clippy::unwrap_used)]

mod support;

use burnburn_gcode::{GrblProfile, LowerError, PowerMode, RefusalKind, WorkCoordinate, lower};
use support::{interp, plans};

fn profile() -> GrblProfile {
    GrblProfile::grbl_1_1_laser()
}

fn text(plan: &burnburn_job::JobPlan, p: &GrblProfile) -> String {
    lower(plan, p).unwrap().to_text()
}

fn approx(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b} (tol {tol})");
}

#[test]
fn rectangle_cut_burns_exactly_the_perimeter_at_the_scaled_power() {
    let t = text(&plans::rectangle_cut(false), &profile());
    let run = interp::run(&t).unwrap();
    // 40 + 20 + 40 + 20 mm burnt; the only other motion is the 10,10 approach from the origin.
    approx(run.laser_on_length(), 120.0, 1e-9);
    approx(run.tool_path_length(), 120.0 + 200f64.sqrt(), 1e-9);
    // 60 % of $30=1000 is S600; 20 mm/s is F1200 mm/min: 120 mm take 6 s -> 3.6 ratio-seconds.
    approx(run.energy_ratio_seconds(1000.0), 3.6, 1e-9);
    assert!(run.burn_segments().iter().all(|s| s.s == 600.0 && s.feed == 1200.0));
    assert_eq!(run.burn_bounds(), Some((10.0, 10.0, 50.0, 30.0)));
    assert!(run.ended && !run.laser_on_at_end && !run.air_on_at_end);
}

#[test]
fn program_text_is_minimal_modal_g_code() {
    let plan = plans::rectangle_cut(false);
    let program = lower(&plan, &profile()).unwrap();
    let id = plan.id().to_string();
    let expected = [
        format!("; BurnBurn {id}"),
        "G21".into(),
        "G90".into(),
        "G94".into(),
        "M5".into(),
        "; operation 1 pass 0".into(),
        "G0 X10 Y10".into(),
        "M4 S0".into(),
        "G1 X50 S600 F1200".into(),
        "G1 Y30".into(),
        "G1 X10".into(),
        "G1 Y10".into(),
        "M5".into(),
        "M2".into(),
    ];
    assert_eq!(program.lines(), expected);
    assert_eq!(program.plan(), plan.id());
    assert_eq!(program.to_text(), expected.join("\n") + "\n");
}

#[test]
fn constant_power_uses_m3_and_dynamic_power_uses_m4() {
    let plan = plans::rectangle_cut(false);
    let dynamic = text(&plan, &profile());
    assert!(dynamic.contains("M4 S0") && !dynamic.contains("M3"), "{dynamic}");
    let constant = text(&plan, &GrblProfile { power_mode: PowerMode::Constant, ..profile() });
    assert!(constant.contains("M3 S0") && !constant.contains("M4"), "{constant}");
    // Both are laser-on in the interpreter and burn the same length.
    approx(interp::run(&constant).unwrap().laser_on_length(), 120.0, 1e-9);
}

#[test]
fn spindle_scale_follows_dollar_30() {
    // 0.6 * 255 = 153 exactly; 0.6 * 10000 = 6000.
    let t = text(&plans::rectangle_cut(false), &GrblProfile { max_spindle: 255, ..profile() });
    assert!(t.contains("S153"), "{t}");
    let t = text(&plans::rectangle_cut(false), &GrblProfile { max_spindle: 10_000, ..profile() });
    assert!(t.contains("S6000"), "{t}");
}

#[test]
fn power_never_exceeds_the_profile_maximum() {
    // 0.9999 of $30=255 rounds to 255, never 256.
    let mut b = plans::builder();
    let s = b.add_settings(plans::settings(burnburn_job::PlanMode::Line, 10.0, 1.0, false)).unwrap();
    plans::boundary(&mut b, s, 1, 0);
    b.push(burnburn_job::PlanEvent::Cut { to: plans::pt(5.0, 0.0), origin: None }).unwrap();
    let plan = b.finish().unwrap();
    let t = text(&plan, &GrblProfile { max_spindle: 255, ..profile() });
    assert!(t.contains("S255") && !t.contains("S256"), "{t}");
}

#[test]
fn multiple_passes_repeat_the_burn() {
    let run = interp::run(&text(&plans::rectangle_passes(2, false), &profile())).unwrap();
    approx(run.laser_on_length(), 240.0, 1e-9);
    approx(run.energy_ratio_seconds(1000.0), 7.2, 1e-9);
}

#[test]
fn fill_scan_burns_only_inside_the_rows_and_overscans_with_the_laser_off() {
    let run = interp::run(&text(&plans::fill_scan(true), &profile())).unwrap();
    approx(run.laser_on_length(), 120.0, 1e-9);
    assert_eq!(run.burn_bounds(), Some((50.0, 100.0, 70.0, 105.0)));
    // 30 % at 100 mm/s: 120 mm = 1.2 s -> 0.36 ratio-seconds.
    approx(run.energy_ratio_seconds(1000.0), 0.36, 1e-9);
    // The overscan run-in/out (2 mm each side) is laser-off motion and widens the bounds.
    let feed_xs = run.segments.iter().filter(|s| !s.rapid).flat_map(|s| [s.from.0, s.to.0]);
    let (x0, x1) = feed_xs.fold((f64::MAX, f64::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
    assert_eq!((x0, x1), (48.0, 72.0));
    assert!(run.segments.iter().any(|s| !s.rapid && !s.laser_on && s.length() > 0.0));
}

#[test]
fn y_axis_scan_runs_along_y() {
    let run = interp::run(&text(&plans::fill_scan(false), &profile())).unwrap();
    approx(run.laser_on_length(), 120.0, 1e-9);
    assert_eq!(run.burn_bounds(), Some((50.0, 100.0, 55.0, 120.0)));
}

#[test]
fn grayscale_runs_carry_their_own_power() {
    let run = interp::run(&text(&plans::grayscale(), &profile())).unwrap();
    approx(run.laser_on_length(), 30.0, 1e-9);
    approx(run.energy_ratio_seconds(1000.0), 0.17, 1e-9);
    let powers: Vec<f64> = run.burn_segments().iter().map(|s| s.s).collect();
    assert_eq!(powers, vec![200.0, 500.0, 1000.0]);
}

#[test]
fn angled_scan_is_a_plain_diagonal_move() {
    let run = interp::run(&text(&plans::angled(), &profile())).unwrap();
    approx(run.laser_on_length(), 100.0, 1e-9);
    assert_eq!(run.burn_bounds(), Some((3.0, 4.0, 63.0, 84.0)));
}

#[test]
fn air_assist_is_switched_around_the_burn_when_the_machine_has_it() {
    let p = GrblProfile { air_assist: true, ..profile() };
    let run = interp::run(&text(&plans::rectangle_cut(true), &p)).unwrap();
    assert!(!run.burn_segments().is_empty() && run.burn_segments().iter().all(|s| s.air));
    assert!(!run.air_on_at_end, "air must be off again at program end");
    // Not requested: never switched on.
    let run = interp::run(&text(&plans::rectangle_cut(false), &p)).unwrap();
    assert!(run.segments.iter().all(|s| !s.air));
}

#[test]
fn air_assist_requested_without_the_output_is_refused_not_dropped() {
    let e = lower(&plans::rectangle_cut(true), &profile()).unwrap_err();
    let LowerError::Refused(r) = e else { panic!("{e:?}") };
    assert!(r.iter().any(|x| x.kind == RefusalKind::AirAssistUnavailable), "{r:?}");
}

#[test]
fn work_coordinate_system_is_selected_in_the_header() {
    let p = GrblProfile { work_coordinate: Some(WorkCoordinate::G55), ..profile() };
    let t = text(&plans::rectangle_cut(false), &p);
    assert_eq!(interp::run(&t).unwrap().work_system, Some(55));
    assert!(t.lines().position(|l| l == "G55").unwrap() < t.lines().position(|l| l.starts_with("G0")).unwrap());
    assert_eq!(interp::run(&text(&plans::rectangle_cut(false), &profile())).unwrap().work_system, None);
}

#[test]
fn coordinates_are_rounded_half_away_from_zero_to_the_profile_decimals() {
    let mut b = plans::builder();
    let s = b.add_settings(plans::settings(burnburn_job::PlanMode::Line, 10.0, 0.5, false)).unwrap();
    plans::boundary(&mut b, s, 1, 0);
    b.push(burnburn_job::PlanEvent::Travel { to: plans::pt(10.25, 0.5) }).unwrap();
    b.push(burnburn_job::PlanEvent::Cut { to: plans::pt(20.0, 0.5), origin: None }).unwrap();
    let plan = b.finish().unwrap();
    let t = text(&plan, &GrblProfile { decimals: 1, ..profile() });
    assert!(t.contains("G0 X10.3 Y0.5"), "{t}");
    let t = text(&plan, &profile());
    assert!(t.contains("G0 X10.25 Y0.5"), "{t}");
    // 4 decimals keep a 0.1 um position.
    let mut b = plans::builder();
    let s = b.add_settings(plans::settings(burnburn_job::PlanMode::Line, 10.0, 0.5, false)).unwrap();
    plans::boundary(&mut b, s, 1, 0);
    b.push(burnburn_job::PlanEvent::Travel { to: plans::pt(1.2345, 0.0) }).unwrap();
    let t = text(&b.finish().unwrap(), &GrblProfile { decimals: 4, ..profile() });
    assert!(t.contains("G0 X1.2345"), "{t}");
}

#[test]
fn lowering_is_deterministic_and_absolute() {
    let plan = plans::fill_scan(true);
    assert_eq!(text(&plan, &profile()), text(&plan, &profile()));
    let t = text(&plan, &profile());
    assert!(t.contains("G90") && !t.contains("G91") && !t.contains("G20"));
}

#[test]
fn unsupported_features_are_refused_with_their_event_and_never_dropped() {
    let cases: [(burnburn_job::JobPlan, RefusalKind); 3] = [
        (plans::dots(), RefusalKind::Feature(burnburn_job::Feature::DotMode)),
        (plans::ramp(), RefusalKind::Feature(burnburn_job::Feature::RampLength)),
        (plans::focus_offset(), RefusalKind::Feature(burnburn_job::Feature::ZStep)),
    ];
    for (plan, kind) in cases {
        let LowerError::Refused(r) = lower(&plan, &profile()).unwrap_err() else { panic!() };
        assert!(r.iter().any(|x| x.kind == kind && x.event.is_some()), "{kind:?}: {r:?}");
    }
}

#[test]
fn every_refusal_is_listed_at_once() {
    let mut b = plans::builder();
    let s = b.add_settings(plans::settings(burnburn_job::PlanMode::Line, 10.0, 0.5, true)).unwrap();
    plans::boundary(&mut b, s, 1, 0);
    b.push(burnburn_job::PlanEvent::FocusOffset { z_mm: 1.0 }).unwrap();
    b.push(burnburn_job::PlanEvent::Travel { to: plans::pt(5.0, 5.0) }).unwrap();
    let plan = b.finish().unwrap();
    let r = burnburn_gcode::check(&plan, &profile());
    let kinds: Vec<_> = r.iter().map(|x| x.kind.clone()).collect();
    assert!(kinds.contains(&RefusalKind::AirAssistUnavailable));
    assert!(kinds.contains(&RefusalKind::Feature(burnburn_job::Feature::ZStep)));
}

#[test]
fn a_profile_without_laser_mode_is_refused() {
    let p = GrblProfile { laser_mode: false, ..profile() };
    let LowerError::Refused(r) = lower(&plans::rectangle_cut(false), &p).unwrap_err() else { panic!() };
    assert!(r.iter().any(|x| x.kind == RefusalKind::LaserModeRequired && x.event.is_none()), "{r:?}");
}

#[test]
fn motion_outside_the_bed_is_refused() {
    let LowerError::Refused(r) = lower(&plans::outside_bed(), &profile()).unwrap_err() else { panic!() };
    assert!(r.iter().any(|x| x.kind == RefusalKind::OutsideBed), "{r:?}");
}

#[test]
fn an_invalid_profile_is_refused() {
    let p = GrblProfile { max_spindle: 0, ..profile() };
    let LowerError::Refused(r) = lower(&plans::rectangle_cut(false), &p).unwrap_err() else { panic!() };
    assert!(r.iter().any(|x| matches!(x.kind, RefusalKind::InvalidProfile(_))), "{r:?}");
}

proptest::proptest! {
    /// Any polyline cut on the bed burns exactly its own length (within the quantisation of
    /// each vertex) and never leaves the bed or the laser on.
    #[test]
    fn random_polylines_burn_their_own_length(
        pts in proptest::collection::vec((0.0f64..400.0, 0.0f64..300.0), 2..40),
        speed in 1.0f64..500.0,
        power in 0.01f64..1.0,
    ) {
        use burnburn_job::{PlanEvent, PlanMode};
        let mut b = plans::builder();
        let s = b.add_settings(plans::settings(PlanMode::Line, speed, power, false)).unwrap();
        plans::boundary(&mut b, s, 1, 0);
        b.push(PlanEvent::Travel { to: plans::pt(pts[0].0, pts[0].1) }).unwrap();
        let mut expected = 0.0;
        for w in pts.windows(2) {
            expected += (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            b.push(PlanEvent::Cut { to: plans::pt(w[1].0, w[1].1), origin: None }).unwrap();
        }
        let plan = b.finish().unwrap();
        let run = interp::run(&text(&plan, &profile())).unwrap();
        // Each vertex moves by at most 0.0005 mm per axis (3 decimals); a segment changes by
        // at most 2 * 0.0005 * sqrt(2) mm. Zero-length (deduplicated) segments only shrink.
        let tol = pts.len() as f64 * 0.0015;
        proptest::prop_assert!((run.laser_on_length() - expected).abs() <= tol, "{} vs {}", run.laser_on_length(), expected);
        let (x0, y0, x1, y1) = run.bounds().unwrap();
        proptest::prop_assert!(x0 >= 0.0 && y0 >= 0.0 && x1 <= 400.0 && y1 <= 300.0);
        proptest::prop_assert!(!run.laser_on_at_end && run.ended);
    }
}
