//! W1-L3a line-mode extras: tabs by spacing and their invalidation, lead angle and arc leads,
//! overcut on the last pass only, perforation phase and dot (dwell) mode. Oracles are analytic:
//! contour lengths, tab centres, circle geometry.

#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::{CompileError, Finding, compile, preflight};
use burnburn_document::process::{
    AdvancedOptions, DotMode, Lead, LeadStyle, LineOptions, Overcut, Passes, Perforation, ProcessCommand, TabSpec,
};
use burnburn_document::{Anchor, Axis, CommandPayload, DocumentSession, GeometryCommand};
use burnburn_geometry::{PathBuilder, Point};
use burnburn_job::{Feature, JobPlan, PlanEvent, PlanMode, Support};
use burnburn_types::{CancellationToken, GeometryId, OperationId};
use common::*;

fn line_with(s: &mut DocumentSession, g: GeometryId, line: LineOptions) -> OperationId {
    let op = add_op(s, line_op("Snijden", 10.0, 50.0), vec![g]);
    run(
        s,
        CommandPayload::Process(ProcessCommand::SetAdvanced {
            operation: op,
            advanced: AdvancedOptions { line, ..Default::default() },
        }),
    );
    op
}

fn tabs(spacing: f64, width: f64) -> LineOptions {
    LineOptions { tabs: Some(TabSpec::spaced(mm(spacing), mm(width)).unwrap()), ..Default::default() }
}

/// `(x, y)` of every laser-off travel after the entry travel, i.e. the end of each tab gap.
fn gap_ends(plan: &JobPlan) -> Vec<(f64, f64)> {
    plan.events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::Travel { to } => Some((to.x_mm(), to.y_mm())),
            _ => None,
        })
        .skip(1)
        .collect()
}

#[test]
fn tabs_by_spacing_get_their_count_from_each_contour_length() {
    // 10 × 10 square: perimeter 40, spacing 10 → 4 tabs of width 2 centred at arc 5, 15, 25, 35.
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    line_with(&mut s, g, tabs(10.0, 2.0));
    let plan = plan_from_outside(&s, &profile()).unwrap();
    assert!((cut_length(&plan) - (40.0 - 4.0 * 2.0)).abs() < 1e-9, "{}", cut_length(&plan));
    // Each gap ends one half tab width past its centre: (6,0), (10,6), (4,10), (0,4).
    let ends = gap_ends(&plan);
    let expected = [(6.0, 0.0), (10.0, 6.0), (4.0, 10.0), (0.0, 4.0)];
    assert_eq!(ends.len(), 4, "{ends:?}");
    for (got, want) in ends.iter().zip(expected) {
        assert!(near(*got, want), "{got:?} vs {want:?}");
    }

    // A larger contour in the same operation gets more tabs: 20 × 20 (perimeter 80) → 4 at
    // spacing 20, the 10 × 10 one → 2 (round(40/20)).
    let mut t = session();
    let small = rectangle(&mut t, 0.0, 0.0, 10.0, 10.0);
    let large = rectangle(&mut t, 30.0, 0.0, 20.0, 20.0);
    let op = add_op(&mut t, line_op("Snijden", 10.0, 50.0), vec![small, large]);
    run(
        &mut t,
        CommandPayload::Process(ProcessCommand::SetAdvanced {
            operation: op,
            advanced: AdvancedOptions { line: tabs(20.0, 1.0), ..Default::default() },
        }),
    );
    let plan = plan_from_outside(&t, &profile()).unwrap();
    assert!((cut_length(&plan) - ((40.0 - 2.0) + (80.0 - 4.0))).abs() < 1e-9, "{}", cut_length(&plan));
}

#[test]
fn tabs_that_cannot_be_placed_are_refused_not_clamped() {
    // 80 tabs would be needed (perimeter 40 / spacing 0.5), the limit is 64.
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let op = line_with(&mut s, g, tabs(0.5, 0.1));
    assert_eq!(
        plan_from_outside(&s, &profile()).unwrap_err(),
        CompileError::InvalidTabs { operation: op, geometry: g }
    );
}

#[test]
fn tab_invalid_after_source_change_is_reported() {
    // C04. Two tabs of 1 mm on a 10 × 10 square are fine …
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let two = LineOptions { tabs: Some(TabSpec::new(2, mm(1.0)).unwrap()), ..Default::default() };
    let op = line_with(&mut s, g, two);
    let p = profile();
    let plan = plan_from_outside(&s, &p).unwrap();
    assert!(!preflight(&s.snapshot(), &plan, &p).findings.iter().any(|f| matches!(f, Finding::InvalidTabs { .. })));

    // … until the source shrinks to 0.4 × 0.4 mm (perimeter 1.6 < 2 mm of tabs).
    for axis in [Axis::Width, Axis::Height] {
        run(&mut s, CommandPayload::SetDimension { target: g, axis, value: mm(0.4), anchor: Anchor::Min });
    }
    let report = preflight(&s.snapshot(), &plan, &p);
    let finding = Finding::InvalidTabs { operation: op, geometry: g };
    assert!(report.findings.contains(&finding), "{:?}", report.findings);
    assert!(report.has_errors(), "an invalid tab blocks production");
    assert!(report.findings.contains(&Finding::StalePlan), "the old plan is stale as well");
    // Compiling refuses with the same operation and geometry.
    assert_eq!(
        compile(&s.snapshot(), &p, &options(), &CancellationToken::never()),
        Err(CompileError::InvalidTabs { operation: op, geometry: g })
    );
    // Removing the tabs makes the operation valid again.
    run(
        &mut s,
        CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced: AdvancedOptions::default() }),
    );
    let plan = plan_from_outside(&s, &p).unwrap();
    assert!(!preflight(&s.snapshot(), &plan, &p).findings.iter().any(|f| matches!(f, Finding::InvalidTabs { .. })));
}

#[test]
fn lead_angle_tilts_the_lead_towards_the_contour_tangent() {
    // Counter-clockwise square entered at (0, 0) heading +X; the waste is below the bottom edge.
    // A 45° lead of 2 mm starts at (−√2, −√2) (perpendicular would be (0, −2)); the lead-out
    // leaves (0, 0) heading −Y and ends at (−√2, −√2) as well.
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let lead = Lead::new(mm(2.0)).unwrap().with_angle(45.0).unwrap();
    line_with(&mut s, g, LineOptions { lead_in: Some(lead), lead_out: Some(lead), ..Default::default() });
    let plan = plan_from_outside(&s, &profile()).unwrap();
    let r = std::f64::consts::SQRT_2;
    let moves = motion(plan.events());
    assert!(near((moves[0].1, moves[0].2), (-r, -r)), "{:?}", moves[0]);
    assert_eq!(moves[1].0, "cut");
    assert!(near((moves[1].1, moves[1].2), (0.0, 0.0)), "the lead ends on the contour");
    let last = moves.last().unwrap();
    assert_eq!(last.0, "cut");
    assert!(near((last.1, last.2), (-r, -r)), "{last:?}");
}

#[test]
fn arc_leads_are_tangent_quarter_circles_on_the_waste_side() {
    // Radius 2, entry (0, 0) heading +X, waste below: centre (0, −2); the lead-in starts at
    // (−2, −2) and arrives heading +X; the lead-out leaves heading −Y and ends at (−2, −2).
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let arc = Lead::new(mm(2.0)).unwrap().with_style(LeadStyle::Arc);
    line_with(&mut s, g, LineOptions { lead_in: Some(arc), lead_out: Some(arc), ..Default::default() });
    let plan = plan_from_outside(&s, &profile()).unwrap();
    let moves = motion(plan.events());
    assert!(near((moves[0].1, moves[0].2), (-2.0, -2.0)), "{:?}", moves[0]);
    // Lead-in: the cuts up to the first contour vertex (10, 0) are arc vertices; the contour
    // itself is one segment from (0,0) to (10,0).
    let arrive = moves.iter().position(|m| near((m.1, m.2), (0.0, 0.0))).unwrap();
    assert!(arrive >= 8, "a quarter circle at 0.01 mm tolerance needs many chords, got {arrive}");
    let centre = (0.0, -2.0);
    for m in &moves[..=arrive] {
        let d = ((m.1 - centre.0).powi(2) + (m.2 - centre.1).powi(2)).sqrt();
        assert!((d - 2.0).abs() < 1e-9, "arc vertex {m:?} is {d} from the centre");
    }
    // Chord sagitta stays within the 0.01 mm production tolerance.
    for w in moves[..=arrive].windows(2) {
        let mid = ((w[0].1 + w[1].1) / 2.0, (w[0].2 + w[1].2) / 2.0);
        let d = ((mid.0 - centre.0).powi(2) + (mid.1 - centre.1).powi(2)).sqrt();
        assert!(2.0 - d <= 0.01 + 1e-9, "sagitta {} too large", 2.0 - d);
    }
    // Tangent at the arrival: the last chord is within a few degrees of +X.
    let (a, b) = (moves[arrive - 1], moves[arrive]);
    let angle = (b.2 - a.2).atan2(b.1 - a.1);
    assert!(angle.abs() < 0.1, "arrives heading {angle} rad");
    // The lead-out ends at (−2, −2) on the circle around (−2, 0)... for a path leaving (0, 0)
    // heading −Y with the waste to its left (−X): centre (−2, 0).
    let last = moves.last().unwrap();
    assert!(near((last.1, last.2), (-2.0, -2.0)), "{last:?}");
}

#[test]
fn arc_lead_on_an_open_contour_is_refused_not_replaced() {
    let mut s = session();
    let open = PathBuilder::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)).build().unwrap();
    let g = run(&mut s, CommandPayload::Edit(GeometryCommand::CreatePath { path: open, name: String::new() }))
        .changes
        .created[0];
    let arc = Lead::new(mm(2.0)).unwrap().with_style(LeadStyle::Arc);
    let op = line_with(&mut s, g, LineOptions { lead_in: Some(arc), ..Default::default() });
    assert_eq!(
        plan_from_outside(&s, &profile()).unwrap_err(),
        CompileError::ArcLeadOnOpenContour { operation: op, geometry: g }
    );
}

#[test]
fn overcut_is_cut_on_the_last_pass_only() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let mut three = line_op("Snijden", 10.0, 50.0);
    three.settings.passes = Passes::new(3).unwrap();
    let op = add_op(&mut s, three, vec![g]);
    run(
        &mut s,
        CommandPayload::Process(ProcessCommand::SetAdvanced {
            operation: op,
            advanced: AdvancedOptions {
                line: LineOptions { overcut: Some(Overcut::new(mm(3.0)).unwrap()), ..Default::default() },
                ..Default::default()
            },
        }),
    );
    let plan = plan_from_outside(&s, &profile()).unwrap();
    // Cut length per pass: split the events at the operation boundaries.
    let mut per_pass: Vec<f64> = Vec::new();
    let mut at = plan.start();
    for e in plan.events() {
        match *e {
            PlanEvent::OperationBoundary { .. } => per_pass.push(0.0),
            PlanEvent::Travel { to } => at = to,
            PlanEvent::Cut { to, .. } => {
                *per_pass.last_mut().unwrap() += at.distance(to);
                at = to;
            }
            _ => {}
        }
    }
    assert_eq!(per_pass.len(), 3);
    assert!((per_pass[0] - 40.0).abs() < 1e-9 && (per_pass[1] - 40.0).abs() < 1e-9, "{per_pass:?}");
    assert!((per_pass[2] - 43.0).abs() < 1e-9, "the overcut belongs to the last pass: {per_pass:?}");
}

#[test]
fn perforation_phase_runs_through_the_corners() {
    // Characterisation: on 3 / off 2 along the 40 mm perimeter is exactly eight periods, so the
    // cut length is 8 · 3 = 24 and the phase is not reset at the three corners.
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    line_with(
        &mut s,
        g,
        LineOptions { perforation: Some(Perforation::new(mm(3.0), mm(2.0)).unwrap()), ..Default::default() },
    );
    let plan = plan_from_outside(&s, &profile()).unwrap();
    assert!((cut_length(&plan) - 24.0).abs() < 1e-9, "{}", cut_length(&plan));
}

#[test]
fn dot_mode_fires_one_pulse_per_spacing_and_is_gated() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let dot = DotMode::new(mm(5.0), 2.0).unwrap();
    let op = line_with(&mut s, g, LineOptions { dot: Some(dot), ..Default::default() });
    // The default matrix has no proof for dot pulses.
    let err = plan_from_outside(&s, &profile()).unwrap_err();
    assert!(
        matches!(err, CompileError::UnsupportedFeature { operation, feature: Feature::DotMode, support: Support::Unverified { .. } } if operation == op),
        "{err:?}"
    );
    let plan = plan_from_outside(&s, &supporting(&[Feature::DotMode])).unwrap();
    assert_eq!(plan.settings().len(), 1);
    assert_eq!(plan.settings()[0].mode, PlanMode::Dots { dwell_ms: 2.0 });
    assert!(!plan.events().iter().any(|e| matches!(e, PlanEvent::Cut { .. })), "dots are never joined by cuts");
    // Perimeter 40 at spacing 5: eight dots at arc 0, 5, …, 35 (the closing point is the first dot).
    let mut dots = Vec::new();
    let mut at = plan.start();
    for e in plan.events() {
        match *e {
            PlanEvent::Travel { to } => at = to,
            PlanEvent::Dot { .. } => dots.push((at.x_mm(), at.y_mm())),
            _ => {}
        }
    }
    let expected =
        [(0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (10.0, 5.0), (10.0, 10.0), (5.0, 10.0), (0.0, 10.0), (0.0, 5.0)];
    assert_eq!(dots.len(), 8, "{dots:?}");
    for (got, want) in dots.iter().zip(expected) {
        assert!(near(*got, want), "{got:?} vs {want:?}");
    }

    // An open 10 mm line gets dots at both ends: 0, 5, 10.
    let mut t = session();
    let open = PathBuilder::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)).build().unwrap();
    let g = run(&mut t, CommandPayload::Edit(GeometryCommand::CreatePath { path: open, name: String::new() }))
        .changes
        .created[0];
    line_with(&mut t, g, LineOptions { dot: Some(dot), ..Default::default() });
    let plan = plan_from_outside(&t, &supporting(&[Feature::DotMode])).unwrap();
    assert_eq!(plan.events().iter().filter(|e| matches!(e, PlanEvent::Dot { .. })).count(), 3);
}
