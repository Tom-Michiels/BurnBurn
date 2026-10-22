//! Tab Cut Power (C04, G7-cam): a tab with a cut power is cut with that share of the operation's
//! power instead of being skipped. Oracles are analytic: perimeters, tab centres, power products.

#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::{CompileError, Finding, preflight};
use burnburn_document::process::{AdvancedOptions, LineOptions, ProcessCommand, TabSpec};
use burnburn_document::{CommandPayload, DocumentSession};
use burnburn_job::{Feature, JobPlan, MachinePoint, PlanEvent, Support};
use burnburn_types::{GeometryId, OperationId};
use common::*;
use proptest::prelude::*;

fn tabs_on(s: &mut DocumentSession, g: GeometryId, tabs: TabSpec, power: f64) -> OperationId {
    let op = add_op(s, line_op("Snijden", 10.0, power), vec![g]);
    let line = LineOptions { tabs: Some(tabs), ..Default::default() };
    run(
        s,
        CommandPayload::Process(ProcessCommand::SetAdvanced {
            operation: op,
            advanced: AdvancedOptions { line, ..Default::default() },
        }),
    );
    op
}

fn lowered_profile() -> burnburn_job::MachineProfile {
    supporting(&[Feature::TabCutPower])
}

/// `(from, to, power)` of every `CutPower` event; the head position before one is the end of the
/// previous event.
fn power_cuts(plan: &JobPlan) -> Vec<(MachinePoint, MachinePoint, f64)> {
    let mut at = plan.start();
    let mut out = Vec::new();
    for e in plan.events() {
        match *e {
            PlanEvent::Travel { to } | PlanEvent::Cut { to, .. } => at = to,
            PlanEvent::CutPower { to, power, .. } => {
                out.push((at, to, power.get()));
                at = to;
            }
            _ => {}
        }
    }
    out
}

fn travels_after_entry(plan: &JobPlan) -> usize {
    plan.events().iter().filter(|e| matches!(e, PlanEvent::Travel { .. })).count().saturating_sub(1)
}

#[test]
fn tabs_with_a_power_are_cut_at_that_share_of_the_operation_power() {
    // 10 x 10 square at 50 % power, 4 tabs of 2 mm at 25 %: tabs centred at arc 5, 15, 25, 35.
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    tabs_on(&mut s, g, TabSpec::new(4, mm(2.0)).unwrap().with_cut_power(25.0).unwrap(), 50.0);
    let plan = plan_from_outside(&s, &lowered_profile()).unwrap();

    let tabs = power_cuts(&plan);
    assert_eq!(tabs.len(), 4, "{tabs:?}");
    let expected = [((4.0, 0.0), (6.0, 0.0)), ((10.0, 4.0), (10.0, 6.0)), ((6.0, 10.0), (4.0, 10.0)), ((0.0, 6.0), (0.0, 4.0))];
    for ((from, to, power), (want_from, want_to)) in tabs.iter().zip(expected) {
        assert!(near((from.x_mm(), from.y_mm()), want_from), "{from:?} vs {want_from:?}");
        assert!(near((to.x_mm(), to.y_mm()), want_to), "{to:?} vs {want_to:?}");
        assert!((power - 0.125).abs() < 1e-12, "25 % of 50 % is 12.5 %, got {power}");
    }
    assert_eq!(travels_after_entry(&plan), 0, "the laser stays on all around the contour");
    assert!((cut_length(&plan) - 32.0).abs() < 1e-9, "the full-power part is the perimeter minus the tabs");
}

#[test]
fn tabs_without_a_power_are_still_skipped_and_need_no_new_capability() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    tabs_on(&mut s, g, TabSpec::new(4, mm(2.0)).unwrap(), 50.0);
    // `profile()` leaves TabCutPower unverified: skipped tabs do not ask for it.
    let plan = plan_from_outside(&s, &profile()).unwrap();
    assert!(power_cuts(&plan).is_empty());
    assert_eq!(travels_after_entry(&plan), 4);
}

#[test]
fn a_machine_without_the_capability_refuses_the_power_by_name() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let op = tabs_on(&mut s, g, TabSpec::new(4, mm(2.0)).unwrap().with_cut_power(25.0).unwrap(), 50.0);
    // Tabs themselves are supported, the power per movement is unverified on the default matrix.
    match plan_from_outside(&s, &profile()) {
        Err(CompileError::UnsupportedFeature { operation, feature: Feature::TabCutPower, support }) => {
            assert_eq!(operation, op);
            assert!(matches!(support, Support::Unverified { .. }), "{support:?}");
        }
        other => panic!("expected the named refusal, got {other:?}"),
    }
}

#[test]
fn the_tab_segments_count_as_burning_for_the_preflight() {
    // A 10 x 10 square from x = 1495 on a 1500 mm wide bed. One tab of 39 of the 40 mm is cut at a
    // lowered power: everything outside the bed is tab, the two full-power stubs at the entry
    // corner (1495, 0) are inside.
    let mut s = session();
    let g = rectangle(&mut s, 1495.0, 0.0, 10.0, 10.0);
    tabs_on(&mut s, g, TabSpec::new(1, mm(39.0)).unwrap().with_cut_power(25.0).unwrap(), 50.0);
    let p = lowered_profile();
    let plan = plan_from_outside(&s, &p).unwrap();
    assert!(power_cuts(&plan).iter().any(|(_, to, _)| to.x_mm() > 1500.0), "the tab piece leaves the bed");
    let report = preflight(&s.snapshot(), &plan, &p);
    assert!(report.findings.iter().any(|f| matches!(f, Finding::OutsideBed { .. })), "{:?}", report.findings);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Whatever the rectangle, count, width and power: the tab pieces add up to count x width, the
    /// full-power rest to the remaining perimeter, no laser-off gap appears and every tab
    /// piece carries exactly `share x operation power`.
    #[test]
    fn tab_pieces_and_the_rest_always_add_up_to_the_perimeter(
        w in 5.0f64..60.0,
        h in 5.0f64..60.0,
        count in 1u32..8,
        width_frac in 0.05f64..0.9,
        share in 1.0f64..99.0,
        power in 5.0f64..100.0,
    ) {
        let perimeter = 2.0 * (w + h);
        let tab_width = width_frac * perimeter / f64::from(count);
        let mut s = session();
        let g = rectangle(&mut s, 0.0, 0.0, w, h);
        tabs_on(&mut s, g, TabSpec::new(count, mm(tab_width)).unwrap().with_cut_power(share).unwrap(), power);
        let plan = plan_from_outside(&s, &lowered_profile()).unwrap();
        let tab_length: f64 = power_cuts(&plan).iter().map(|(a, b, _)| a.distance(*b)).sum();
        prop_assert!((tab_length - f64::from(count) * tab_width).abs() < 1e-6, "{tab_length}");
        prop_assert!((cut_length(&plan) - (perimeter - f64::from(count) * tab_width)).abs() < 1e-6);
        prop_assert_eq!(travels_after_entry(&plan), 0);
        for (_, _, p) in power_cuts(&plan) {
            prop_assert!((p - power / 100.0 * share / 100.0).abs() < 1e-12);
        }
    }
}
