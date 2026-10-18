//! Sketch evaluation: driving vs driven dimensions, diagnosis per constraint, last valid result
//! with production blocked, and reproducible outputs.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

mod common;

use burnburn_document::sketch::{Along, DimensionKind, DimensionRole, Relation, SketchCommand, SketchConstraint};
use burnburn_document::{DerivedOwner, EvalDiagnosis, EvalStatus, Evaluator};
use burnburn_sketch::param::{Dim, ExprErrorKind, Span, Value};
use burnburn_sketch::solver::{Coordinate, SketchSolverAdapter};
use burnburn_sketch::{Diagnosis, DimensionError, Health, SketchEvaluator, evaluate_sketch};
use burnburn_types::{CancellationSource, CancellationToken};
use common::{plate, session, set_param, sketch_cmd};
use proptest::prelude::*;

fn eval(s: &burnburn_document::sketch::Sketch) -> burnburn_sketch::Evaluation {
    evaluate_sketch(s, &SketchSolverAdapter, &CancellationToken::never()).unwrap()
}

#[test]
fn plate_is_fully_constrained_and_every_constraint_satisfied() {
    let p = plate();
    let e = eval(&p.sketch);
    assert_eq!(e.health, Health::FullyConstrained);
    assert_eq!(e.dof, 0);
    assert!(e.diagnostics.values().all(|d| *d == Diagnosis::Satisfied), "{:?}", e.diagnostics);
    let solved = e.solved.as_ref().unwrap();
    let c2 = solved.point(p.centres[1]).unwrap();
    assert!((c2.x - 110.0).abs() < 1e-9 && (c2.y - 10.0).abs() < 1e-9, "{c2:?}");
    assert!((solved.radius(p.holes[3]).unwrap() - 2.0).abs() < 1e-12);
}

#[test]
fn driven_dimension_measures_without_constraining() {
    let mut p = plate();
    let driven = p
        .sketch
        .dimension(
            DimensionKind::Distance { a: p.corners[0], b: p.centres[2], along: Along::Direct },
            "",
            DimensionRole::Driven,
        )
        .unwrap();
    let e = eval(&p.sketch);
    assert_eq!(e.health, Health::FullyConstrained, "a driven dimension adds no equation");
    match e.diagnostics[&driven] {
        Diagnosis::Measured(Value::Length(l)) => {
            assert!((l.mm() - (110f64.powi(2) + 70f64.powi(2)).sqrt()).abs() < 1e-9)
        }
        ref other => panic!("{other:?}"),
    }
}

#[test]
fn removing_a_dimension_leaves_free_dof_and_stays_producible() {
    let mut p = plate();
    p.sketch.remove_constraint(p.width).unwrap();
    let e = eval(&p.sketch);
    assert_eq!(e.health, Health::UnderConstrained { dof: 1 });
    assert!(e.free.contains(&(p.corners[1], Coordinate::X)), "{:?}", e.free);

    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut s = session();
    s.apply_with(sketch_cmd(&s, SketchCommand::Create { sketch: Box::new(p.sketch) }), &[&ev]).unwrap();
    let snap = s.snapshot();
    assert!(snap.objects().all(|(id, _)| !snap.production_blocked(id)), "underconstrained is not an error");
}

#[test]
fn driving_an_overconstraining_dimension_conflicts_and_keeps_last_valid_geometry() {
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut s = session();
    let mut p = plate();
    let extra =
        p.sketch.dimension(DimensionKind::Length { line: p.edges[0] }, "100 mm", DimensionRole::Driven).unwrap();
    let out = s.apply_with(sketch_cmd(&s, SketchCommand::Create { sketch: Box::new(p.sketch) }), &[&ev]).unwrap();
    let id = out.changes.sketches[0];
    let before: Vec<_> = s.snapshot().objects().map(|(_, o)| o.clone()).collect();

    let c = SketchCommand::SetDimensionRole { sketch: id, constraint: extra, role: DimensionRole::Driving };
    s.apply_with(sketch_cmd(&s, c), &[&ev]).unwrap();
    let snap = s.snapshot();
    match &snap.derived_record(DerivedOwner::Sketch(id)).unwrap().status {
        EvalStatus::Failed { diagnosis: EvalDiagnosis::Conflict { constraints } } => {
            assert!(constraints.contains(&extra) && constraints.contains(&p.width), "{constraints:?}");
        }
        other => panic!("{other:?}"),
    }
    let after: Vec<_> = snap.objects().map(|(_, o)| o.clone()).collect();
    assert_eq!(after, before, "last valid geometry stays, bit for bit");
    assert!(snap.objects().all(|(gid, _)| snap.production_blocked(gid)));
    let e = eval(snap.sketch(id).unwrap());
    assert!(matches!(e.diagnostics[&extra], Diagnosis::Conflicting { .. }), "{:?}", e.diagnostics[&extra]);
    assert_eq!(e.conflict_set().first(), Some(&p.width));

    let c = SketchCommand::SetDimensionRole { sketch: id, constraint: extra, role: DimensionRole::Driven };
    s.apply_with(sketch_cmd(&s, c), &[&ev]).unwrap();
    let snap = s.snapshot();
    assert_eq!(snap.derived_record(DerivedOwner::Sketch(id)).unwrap().status, EvalStatus::Valid);
    assert!(snap.objects().all(|(gid, _)| !snap.production_blocked(gid)));
}

#[test]
fn dimension_with_an_angle_where_a_length_is_needed_is_reported_with_position() {
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut s = session();
    let p = plate();
    let out = s.apply_with(sketch_cmd(&s, SketchCommand::Create { sketch: Box::new(p.sketch) }), &[&ev]).unwrap();
    let id = out.changes.sketches[0];
    let c = SketchCommand::SetDimensionValue { sketch: id, constraint: p.hole_diameter, text: "45 deg".into() };
    s.apply_with(sketch_cmd(&s, c), &[&ev]).unwrap();
    let snap = s.snapshot();
    assert_eq!(
        snap.derived_record(DerivedOwner::Sketch(id)).unwrap().status,
        EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidDimensions { constraints: vec![p.hole_diameter] } }
    );
    let e = eval(snap.sketch(id).unwrap());
    assert_eq!(
        e.diagnostics[&p.hole_diameter],
        Diagnosis::Invalid(DimensionError::Expression(burnburn_sketch::param::ExprError {
            kind: ExprErrorKind::WrongDimension { expected: Dim::LENGTH, found: Dim::ANGLE },
            span: Span::new(0, 6),
        }))
    );
}

#[test]
fn parameter_cycle_fails_without_moving_geometry() {
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut s = session();
    let p = plate();
    let out = s.apply_with(sketch_cmd(&s, SketchCommand::Create { sketch: Box::new(p.sketch) }), &[&ev]).unwrap();
    let id = out.changes.sketches[0];
    let before: Vec<_> = s.snapshot().objects().map(|(_, o)| o.clone()).collect();
    s.apply_with(sketch_cmd(&s, set_param("offset", "width / 2 - offset", id)), &[&ev]).unwrap();
    let snap = s.snapshot();
    assert_eq!(
        snap.derived_record(DerivedOwner::Sketch(id)).unwrap().status,
        EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidParameters { parameter: Some("offset".into()) } }
    );
    assert_eq!(snap.objects().map(|(_, o)| o.clone()).collect::<Vec<_>>(), before);
    assert_eq!(snap.sketch(id).unwrap().parameters().get("offset"), Some("width / 2 - offset"), "text kept to fix");
}

#[test]
fn outline_is_one_closed_contour_and_construction_is_not_output() {
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut s = session();
    let p = plate();
    s.apply_with(sketch_cmd(&s, SketchCommand::Create { sketch: Box::new(p.sketch) }), &[&ev]).unwrap();
    let snap = s.snapshot();
    let mut closed = 0;
    for (_, o) in snap.objects() {
        assert_eq!(o.path().subpaths().len(), 1);
        assert!(o.path().subpaths()[0].is_closed());
        closed += 1;
    }
    assert_eq!(closed, 5);
    let outline = snap.objects().map(|(_, o)| o).find(|o| o.path().subpaths()[0].segments().len() == 3).unwrap();
    assert!((outline.path().signed_area() - 9600.0).abs() < 1e-9, "counter-clockwise 120 × 80");
}

#[test]
fn reevaluating_a_stored_sketch_reproduces_bit_identical_outputs() {
    // What reopening a saved project and evaluating again must give (io test covers the file).
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut s = session();
    let p = plate();
    let out = s.apply_with(sketch_cmd(&s, SketchCommand::Create { sketch: Box::new(p.sketch) }), &[&ev]).unwrap();
    let id = out.changes.sketches[0];
    s.apply_with(sketch_cmd(&s, set_param("width", "150 mm", id)), &[&ev]).unwrap();
    let snap = s.snapshot();
    let patch = ev.evaluate(&snap, DerivedOwner::Sketch(id));
    assert_eq!(
        snap.derived_record(DerivedOwner::Sketch(id)).unwrap().inputs,
        Some(patch.inputs),
        "inputs = definition"
    );
    for o in &patch.outputs {
        let stored = snap
            .objects()
            .find(|(_, x)| matches!(x.source(), burnburn_document::GeometrySource::Derived(k) if *k == o.key))
            .map(|(_, x)| x.path().clone())
            .unwrap();
        assert_eq!(*stored, o.path, "bit-identical after re-evaluation from the stored solution");
    }
    assert_eq!(patch.outputs.len(), 5);
}

#[test]
fn cancelled_evaluation_returns_cancelled() {
    let src = CancellationSource::new();
    src.cancel();
    assert!(evaluate_sketch(&plate().sketch, &SketchSolverAdapter, &src.token()).is_err());
}

#[test]
fn constraint_kinds_of_the_document_all_reach_the_solver() {
    // A tangent line–arc and a point-on-line from document data, solved through the evaluator.
    let mut sk = burnburn_document::sketch::Sketch::new("t");
    let a = sk.point(0.0, 0.0).unwrap();
    let b = sk.point(50.0, 0.0).unwrap();
    let l = sk.line(a, b).unwrap();
    let c = sk.point(25.0, 12.0).unwrap();
    let st = sk.point(35.0, 12.0).unwrap();
    let en = sk.point(15.0, 12.0).unwrap();
    let arc = sk.arc(c, st, en, false).unwrap();
    let q = sk.point(30.0, 1.0).unwrap();
    sk.relate(Relation::Fix { point: a, at: burnburn_geometry::Point::new(0.0, 0.0) }).unwrap();
    sk.relate(Relation::Fix { point: b, at: burnburn_geometry::Point::new(50.0, 0.0) }).unwrap();
    sk.dimension(DimensionKind::Radius { curve: arc }, "10 mm", DimensionRole::Driving).unwrap();
    sk.relate(Relation::Tangent(l, arc)).unwrap();
    sk.relate(Relation::PointOn { point: q, curve: l }).unwrap();
    let e = eval(&sk);
    assert!(e.health.is_valid(), "{:?}", e.health);
    let solved = e.solved.unwrap();
    assert!((solved.point(c).unwrap().y - 10.0).abs() < 1e-9);
    assert!(solved.point(q).unwrap().y.abs() < 1e-9);
    let _ = SketchConstraint::Relation(Relation::Horizontal(l));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn feasible_plates_solve_within_tolerance_and_keep_symmetry(
        w in 40.0f64..600.0, h in 40.0f64..400.0, off_frac in 0.05f64..0.4, d in 0.5f64..3.0,
    ) {
        let off = off_frac * w.min(h);
        let mut p = plate();
        p.sketch.set_parameter("width", &format!("{w} mm")).unwrap();
        p.sketch.set_parameter("height", &format!("{h} mm")).unwrap();
        p.sketch.set_parameter("offset", &format!("{off} mm")).unwrap();
        p.sketch.set_parameter("hole", &format!("{d} mm")).unwrap();
        let e = eval(&p.sketch);
        prop_assert_eq!(e.health, Health::FullyConstrained);
        prop_assert!(e.residual <= e.tolerance, "residual {} > {}", e.residual, e.tolerance);
        let s = e.solved.as_ref().unwrap();
        let tol = 1e-9 * w.max(h);
        let c = p.centres.map(|id| s.point(id).unwrap());
        prop_assert!((c[0].x - off).abs() <= tol && (c[1].x - (w - off)).abs() <= tol);
        prop_assert!((c[2].y - (h - off)).abs() <= tol && (c[3].x - off).abs() <= tol);
        prop_assert!((s.radius(p.holes[2]).unwrap() - d / 2.0).abs() <= tol);
    }

    #[test]
    fn evaluation_is_deterministic(w in 40.0f64..600.0) {
        let mut p = plate();
        p.sketch.set_parameter("width", &format!("{w} mm")).unwrap();
        let a = eval(&p.sketch);
        let b = eval(&p.sketch);
        prop_assert_eq!(format!("{:?}", a), format!("{:?}", b));
        let (pa, pb) = (a.solved.unwrap().outputs().unwrap(), b.solved.unwrap().outputs().unwrap());
        prop_assert_eq!(pa, pb);
    }
}
