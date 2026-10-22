//! G6-machine, W09: the compiler uses the measured values of the machine profile. A kerf measured
//! for the material of an operation offsets its contours like the same kerf typed in; the return
//! rows of a bidirectional scan move back by the scan offset at the operation's speed. Every
//! value used is listed in the plan with the proof it came from.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use burnburn_cam::{CompileError, compile};
use burnburn_document::process::{Kerf, OperationMode, ProcessCommand, RecipeDraft};
use burnburn_document::{CommandPayload, DocumentSession};
use burnburn_job::{
    AppliedCalibration, AppliedKind, Calibration, Feature, JobPlan, KerfProof, MachineProfile, OriginCorner,
    PlanEvent, ProofRef, ScanOffsetProof, Support,
};
use burnburn_types::{CancellationToken, OperationId};
use common::*;

fn proof(id: u64, date: &str) -> ProofRef {
    ProofRef { id, date: date.into() }
}

fn with(profile: MachineProfile, calibration: Calibration) -> MachineProfile {
    profile.with_calibration(calibration).unwrap()
}

fn kerf_profile(material: &str, kerf_mm: f64) -> MachineProfile {
    let calibration = Calibration::new(
        vec![KerfProof { material: material.into(), kerf_mm, proof: proof(7, "2026-10-01") }],
        vec![],
    )
    .unwrap();
    with(supporting(&[Feature::KerfCompensation]), calibration)
}

fn scan_profile(origin: OriginCorner, points: &[(f64, f64)]) -> MachineProfile {
    let calibration = Calibration::new(
        vec![],
        points
            .iter()
            .enumerate()
            .map(|(i, &(speed, offset))| ScanOffsetProof {
                speed_mm_s: speed,
                offset_mm: offset,
                proof: proof(i as u64 + 1, "2026-10-02"),
            })
            .collect(),
    )
    .unwrap();
    with(profile_with(origin), calibration)
}

/// A recipe for `material` with the operation's own mode and settings, applied to `op`: the
/// operation then knows its material and keeps what it had.
fn apply_recipe(s: &mut DocumentSession, op: OperationId, material: &str) {
    let operation = s.snapshot().process().operation(op).unwrap().clone();
    let draft = RecipeDraft {
        name: format!("{material} snijden"),
        material: material.into(),
        thickness: mm(3.0),
        machine: "research-1500x900".into(),
        notes: String::new(),
        mode: operation.mode(),
        settings: operation.settings(),
    };
    let out = run(s, CommandPayload::Process(ProcessCommand::AddRecipe { draft }));
    let recipe = out.changes.recipes[0];
    run(s, CommandPayload::Process(ProcessCommand::ApplyRecipe { recipe, operation: op }));
}

/// A 100 mm square cut (10..110), optionally with a typed kerf and a recipe of `material`.
fn square_cut(material: Option<&str>, typed_kerf: Option<f64>) -> (DocumentSession, OperationId) {
    let mut s = session();
    let g = rectangle(&mut s, 10.0, 10.0, 100.0, 100.0);
    let mut draft = line_op("Snijden", 20.0, 60.0);
    if let Some(k) = typed_kerf {
        draft.mode = OperationMode::Line { kerf: Kerf::width(mm(k)).unwrap() };
    }
    let op = add_op(&mut s, draft, vec![g]);
    if let Some(m) = material {
        apply_recipe(&mut s, op, m);
    }
    (s, op)
}

fn compile_plan(s: &DocumentSession, profile: &MachineProfile) -> Result<JobPlan, CompileError> {
    compile(&s.snapshot(), profile, &options(), &CancellationToken::never())
}

fn bounds_of_cuts(plan: &JobPlan) -> (f64, f64, f64, f64) {
    let mut b = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for e in plan.events() {
        if let PlanEvent::Cut { to, .. } = e {
            b = (b.0.min(to.x_mm()), b.1.min(to.y_mm()), b.2.max(to.x_mm()), b.3.max(to.y_mm()));
        }
    }
    b
}

#[test]
fn a_measured_kerf_for_the_material_offsets_the_contour_by_half_of_it() {
    let (s, op) = square_cut(Some("multiplex-3"), None);
    let plan = compile_plan(&s, &kerf_profile("multiplex-3", 0.2)).unwrap();
    // The square is 10..110; the beam centre runs 0.1 mm into the waste: 9.9..110.1.
    let (x0, y0, x1, y1) = bounds_of_cuts(&plan);
    for (got, want) in [(x0, 9.9), (y0, 9.9), (x1, 110.1), (y1, 110.1)] {
        assert!((got - want).abs() < 1e-4, "{:?}", bounds_of_cuts(&plan));
    }
    assert_eq!(
        plan.calibration(),
        &[AppliedCalibration {
            operation: op,
            kind: AppliedKind::Kerf { material: "multiplex-3".into() },
            value_mm: 0.2,
            proofs: vec![proof(7, "2026-10-01")],
        }],
        "the plan says which measurement offset the contour"
    );
}

#[test]
fn it_is_the_same_plan_geometry_as_typing_the_kerf_in() {
    let (typed, _) = square_cut(None, Some(0.2));
    let reference = compile_plan(&typed, &supporting(&[Feature::KerfCompensation])).unwrap();
    let (s, _) = square_cut(Some("multiplex-3"), None);
    let plan = compile_plan(&s, &kerf_profile("multiplex-3", 0.2)).unwrap();
    assert_eq!(plan.events(), reference.events(), "the events are those of an explicit 0.2 mm kerf");
}

#[test]
fn another_material_an_operation_without_material_and_a_zero_kerf_leave_the_contour_alone() {
    let nominal = compile_plan(&square_cut(None, None).0, &supporting(&[Feature::KerfCompensation])).unwrap();
    let cases = [
        (square_cut(Some("acryl-5"), None).0, kerf_profile("multiplex-3", 0.2)),
        (square_cut(None, None).0, kerf_profile("multiplex-3", 0.2)),
        (square_cut(Some("multiplex-3"), None).0, kerf_profile("multiplex-3", 0.0)),
    ];
    for (s, profile) in cases {
        let plan = compile_plan(&s, &profile).unwrap();
        assert_eq!(plan.events(), nominal.events());
        assert!(plan.calibration().is_empty());
    }
}

#[test]
fn a_kerf_typed_into_the_operation_is_never_replaced_by_a_measurement() {
    let (s, _) = square_cut(Some("multiplex-3"), Some(0.3));
    let plan = compile_plan(&s, &kerf_profile("multiplex-3", 0.2)).unwrap();
    let (x0, _, x1, _) = bounds_of_cuts(&plan);
    assert!((x0 - 9.85).abs() < 1e-4 && (x1 - 110.15).abs() < 1e-4, "0.3 mm kerf, not 0.2: {x0} {x1}");
    assert!(plan.calibration().is_empty(), "nothing was taken from the measurements");
}

#[test]
fn a_measured_kerf_needs_the_kerf_capability_like_a_typed_one() {
    let (s, op) = square_cut(Some("multiplex-3"), None);
    let calibration = Calibration::new(
        vec![KerfProof { material: "multiplex-3".into(), kerf_mm: 0.2, proof: proof(7, "2026-10-01") }],
        vec![],
    )
    .unwrap();
    // A machine whose matrix does not report kerf compensation as supported.
    let no_kerf = burnburn_job::MachineCapabilities::conservative()
        .with(Feature::KerfCompensation, Support::Unverified { reason: "nog niet gemeten".into() });
    let err = compile_plan(&s, &with(profile().with_capabilities(no_kerf).unwrap(), calibration)).unwrap_err();
    match &err {
        CompileError::UnsupportedFeature { operation, feature: Feature::KerfCompensation, support } => {
            assert_eq!(*operation, op);
            assert_eq!(*support, Support::Unverified { reason: "nog niet gemeten".into() });
        }
        other => panic!("{other:?}"),
    }
}

// ---- scan offset ----------------------------------------------------------------------------

fn fill_square() -> (DocumentSession, OperationId) {
    let mut s = session();
    let g = rectangle(&mut s, 10.0, 10.0, 20.0, 1.0);
    // 0.5 mm interval over 1 mm height: two rows, forward then return.
    let op = add_op(&mut s, fill_op("Vullen", 0.5, 0.0), vec![g]);
    (s, op)
}

/// `(from_x, to_x, y)` of every scan run, as the plan has them.
fn rows(plan: &JobPlan) -> Vec<(f64, f64, f64)> {
    plan.events()
        .iter()
        .filter_map(|e| match e {
            PlanEvent::ScanRun { line, .. } => Some((line.from.x_mm(), line.to.x_mm(), line.from.y_mm())),
            _ => None,
        })
        .collect()
}

#[test]
fn the_return_rows_move_back_by_the_scan_offset_and_the_forward_rows_stay() {
    let (s, op) = fill_square();
    let plain = rows(&compile_plan(&s, &profile()).unwrap());
    assert!(plain.len() >= 2, "{plain:?}");
    let plan = compile_plan(&s, &scan_profile(OriginCorner::BottomLeft, &[(100.0, 0.2)])).unwrap();
    let shifted = rows(&plan);
    assert_eq!(shifted.len(), plain.len());
    for (a, b) in plain.iter().zip(&shifted) {
        let forward = a.1 > a.0;
        let (dx0, dx1) = (b.0 - a.0, b.1 - a.1);
        if forward {
            assert_eq!((dx0, dx1), (0.0, 0.0), "forward rows are left alone: {a:?} {b:?}");
        } else {
            assert!((dx0 + 0.2).abs() < 1e-9 && (dx1 + 0.2).abs() < 1e-9, "return rows 0.2 mm back: {a:?} {b:?}");
        }
        assert_eq!(a.2, b.2, "rows keep their line");
    }
    assert!(plain.iter().any(|r| r.1 > r.0) && plain.iter().any(|r| r.1 < r.0), "both directions are present");
    assert_eq!(
        plan.calibration(),
        &[AppliedCalibration {
            operation: op,
            kind: AppliedKind::ScanOffset { speed_mm_s: 100.0 },
            value_mm: 0.2,
            proofs: vec![proof(1, "2026-10-02")],
        }]
    );
}

#[test]
fn between_two_measured_speeds_the_offset_is_interpolated_and_both_proofs_are_named() {
    let (s, _) = fill_square();
    // The fill runs at 100 mm/s: halfway between 50 (0.1 mm) and 150 (0.3 mm) is 0.2 mm.
    let plan = compile_plan(&s, &scan_profile(OriginCorner::BottomLeft, &[(50.0, 0.1), (150.0, 0.3)])).unwrap();
    let plain = rows(&compile_plan(&s, &profile()).unwrap());
    let back = plain.iter().zip(rows(&plan)).find(|(a, _)| a.1 < a.0).map(|(a, b)| b.0 - a.0).unwrap();
    assert!((back + 0.2).abs() < 1e-9, "{back}");
    let used = &plan.calibration()[0];
    assert!((used.value_mm - 0.2).abs() < 1e-9);
    assert_eq!(used.proofs.iter().map(|p| p.id).collect::<Vec<_>>(), vec![1, 2]);
}

#[test]
fn outside_the_measured_speeds_nothing_moves_and_nothing_is_claimed() {
    let (s, _) = fill_square();
    let plain = compile_plan(&s, &profile()).unwrap();
    let plan = compile_plan(&s, &scan_profile(OriginCorner::BottomLeft, &[(200.0, 0.2), (400.0, 0.4)])).unwrap();
    assert_eq!(rows(&plan), rows(&plain), "100 mm/s lies below every measurement");
    assert!(plan.calibration().is_empty());
}

#[test]
fn a_scan_that_does_not_turn_around_has_no_return_rows_to_move() {
    let mut s = session();
    let g = rectangle(&mut s, 10.0, 10.0, 20.0, 1.0);
    let mut draft = fill_op("Vullen", 0.5, 0.0);
    draft.mode = {
        let f = burnburn_document::process::FillSettings::new(
            mm(0.5),
            burnburn_document::process::ScanAxis::X,
            burnburn_document::process::ScanDirection::Unidirectional,
            mm(0.0),
        )
        .unwrap();
        OperationMode::Fill(f)
    };
    add_op(&mut s, draft, vec![g]);
    let plain = compile_plan(&s, &profile()).unwrap();
    let plan = compile_plan(&s, &scan_profile(OriginCorner::BottomLeft, &[(100.0, 0.2)])).unwrap();
    assert_eq!(rows(&plan), rows(&plain));
    assert!(plan.calibration().is_empty());
}

#[test]
fn the_offset_is_along_the_machine_axis_whatever_corner_the_origin_is() {
    // Origin top-right: the machine X axis runs the other way on the bed. The rows that run toward
    // smaller machine X are the return rows there, so they are the ones that move back.
    let (s, _) = fill_square();
    let plain = rows(&compile_plan(&s, &profile_with(OriginCorner::TopRight)).unwrap());
    let plan = compile_plan(&s, &scan_profile(OriginCorner::TopRight, &[(100.0, 0.2)])).unwrap();
    let shifted = rows(&plan);
    assert_eq!(plain.len(), shifted.len());
    for (a, b) in plain.iter().zip(&shifted) {
        let toward_larger_x = a.1 > a.0;
        let want = if toward_larger_x { 0.0 } else { -0.2 };
        assert!((b.0 - a.0 - want).abs() < 1e-9 && (b.1 - a.1 - want).abs() < 1e-9, "{a:?} {b:?}");
    }
}

#[test]
fn a_changed_measurement_is_another_plan() {
    let (s, _) = fill_square();
    let a = compile_plan(&s, &scan_profile(OriginCorner::BottomLeft, &[(100.0, 0.2)])).unwrap();
    let b = compile_plan(&s, &scan_profile(OriginCorner::BottomLeft, &[(100.0, 0.25)])).unwrap();
    assert_ne!(a.id(), b.id());
    assert_eq!(a.id(), compile_plan(&s, &scan_profile(OriginCorner::BottomLeft, &[(100.0, 0.2)])).unwrap().id());
}
