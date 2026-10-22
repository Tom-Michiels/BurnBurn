//! G6-machine, W09: measured kerf and scan offsets are part of the machine profile (a new
//! measurement is a new revision) and a plan lists the calibrated values it used with their proofs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use burnburn_job::{
    AppliedCalibration, AppliedKind, Calibration, CalibrationError, JobPlanBuilder, KerfProof, MachineFrame,
    MachinePoint, MachineProfile, OriginCorner, PlanSource, PowerRatio, ProofRef, ScanOffsetProof,
};
use burnburn_types::{ContentHash, HashDomain, OperationId, RevisionId, StableId};

fn profile() -> MachineProfile {
    MachineProfile::new(
        "bench",
        1,
        MachineFrame::new(600.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        500.0,
        300.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn proof(id: u64, date: &str) -> ProofRef {
    ProofRef { id, date: date.into() }
}

fn kerf(material: &str, mm: f64, id: u64) -> KerfProof {
    KerfProof { material: material.into(), kerf_mm: mm, proof: proof(id, "2026-10-01") }
}

fn scan(speed: f64, offset: f64, id: u64) -> ScanOffsetProof {
    ScanOffsetProof { speed_mm_s: speed, offset_mm: offset, proof: proof(id, "2026-10-02") }
}

#[test]
fn an_empty_calibration_leaves_the_profile_revision_alone() {
    let plain = profile();
    let same = profile().with_calibration(Calibration::default()).unwrap();
    assert_eq!(same.revision(), plain.revision(), "no measurement, no new revision");
}

#[test]
fn a_measurement_is_a_new_profile_revision_and_the_same_measurement_the_same_revision() {
    let a = profile().with_calibration(Calibration::new(vec![kerf("multiplex-3", 0.18, 1)], vec![]).unwrap()).unwrap();
    let b = profile().with_calibration(Calibration::new(vec![kerf("multiplex-3", 0.18, 1)], vec![]).unwrap()).unwrap();
    let c = profile().with_calibration(Calibration::new(vec![kerf("multiplex-3", 0.20, 1)], vec![]).unwrap()).unwrap();
    assert_ne!(a.revision().content, profile().revision().content);
    assert_eq!(a.revision(), b.revision());
    assert_ne!(a.revision().content, c.revision().content);
    assert_eq!(a.revision().id, "bench");
}

#[test]
fn the_order_the_measurements_were_given_in_does_not_matter() {
    let one =
        Calibration::new(vec![kerf("b", 0.2, 2), kerf("a", 0.1, 1)], vec![scan(300.0, 0.3, 4), scan(100.0, 0.1, 3)])
            .unwrap();
    let two =
        Calibration::new(vec![kerf("a", 0.1, 1), kerf("b", 0.2, 2)], vec![scan(100.0, 0.1, 3), scan(300.0, 0.3, 4)])
            .unwrap();
    assert_eq!(one, two);
    assert_eq!(
        profile().with_calibration(one).unwrap().revision(),
        profile().with_calibration(two).unwrap().revision()
    );
}

#[test]
fn the_proof_a_value_came_from_is_part_of_the_revision() {
    let a = profile().with_calibration(Calibration::new(vec![kerf("a", 0.1, 1)], vec![]).unwrap()).unwrap();
    let b = profile().with_calibration(Calibration::new(vec![kerf("a", 0.1, 9)], vec![]).unwrap()).unwrap();
    assert_ne!(a.revision().content, b.revision().content, "another measurement of the same number is another setup");
}

#[test]
fn kerf_is_found_per_material() {
    let c = Calibration::new(vec![kerf("multiplex-3", 0.18, 1), kerf("acryl-5", 0.25, 2)], vec![]).unwrap();
    assert_eq!(c.kerf_for("acryl-5").map(|k| (k.kerf_mm, k.proof.id)), Some((0.25, 2)));
    assert_eq!(c.kerf_for("multiplex-3").map(|k| k.kerf_mm), Some(0.18));
    assert!(c.kerf_for("karton").is_none(), "no measurement, no kerf");
}

#[test]
fn the_scan_offset_is_measured_interpolated_between_measurements_and_absent_outside_them() {
    let c = Calibration::new(vec![], vec![scan(100.0, 0.10, 1), scan(300.0, 0.30, 2), scan(500.0, 0.34, 3)]).unwrap();
    let at = |v: f64| c.scan_offset_at(v);
    let exact = at(300.0).unwrap();
    assert_eq!((exact.offset_mm, exact.proofs.iter().map(|p| p.id).collect::<Vec<_>>()), (0.30, vec![2]));
    // Halfway between 100 and 300 mm/s: 0.20, and both measurements are named.
    let mid = at(200.0).unwrap();
    assert!((mid.offset_mm - 0.20).abs() < 1e-12, "{}", mid.offset_mm);
    assert_eq!(mid.proofs.iter().map(|p| p.id).collect::<Vec<_>>(), vec![1, 2]);
    // A quarter of the way from 300 to 500: 0.30 + 0.25 * 0.04.
    assert!((at(350.0).unwrap().offset_mm - 0.31).abs() < 1e-12);
    assert!(at(99.9).is_none() && at(500.1).is_none(), "outside the measured speeds there is no evidence");
    assert!(Calibration::default().scan_offset_at(100.0).is_none());
}

#[test]
fn unusable_measurements_are_refused_with_the_reason() {
    assert_eq!(Calibration::new(vec![kerf("a", -0.1, 1)], vec![]), Err(CalibrationError::OutOfRange("kerf")));
    assert_eq!(Calibration::new(vec![kerf("a", f64::NAN, 1)], vec![]), Err(CalibrationError::NotFinite));
    assert_eq!(
        Calibration::new(vec![kerf("a", 0.1, 1), kerf("a", 0.2, 2)], vec![]),
        Err(CalibrationError::DuplicateMaterial("a".into()))
    );
    assert_eq!(Calibration::new(vec![], vec![scan(0.0, 0.1, 1)]), Err(CalibrationError::OutOfRange("speed")));
    assert_eq!(Calibration::new(vec![], vec![scan(100.0, f64::INFINITY, 1)]), Err(CalibrationError::NotFinite));
    assert_eq!(
        Calibration::new(vec![], vec![scan(100.0, 0.1, 1), scan(100.0, 0.2, 2)]),
        Err(CalibrationError::DuplicateSpeed(100.0))
    );
}

fn plan_with(applied: &[AppliedCalibration]) -> burnburn_job::JobPlan {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"calibration-test"),
        machine_profile: profile().revision().clone(),
        compiler_version: "test".into(),
    };
    let mut b = JobPlanBuilder::new(
        source,
        MachineFrame::new(600.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        MachinePoint::new(0.0, 0.0).unwrap(),
    );
    for a in applied {
        b.note_calibration(a.clone());
    }
    b.finish().unwrap()
}

fn applied(value: f64, id: u64) -> AppliedCalibration {
    AppliedCalibration {
        operation: OperationId::from_raw(1),
        kind: AppliedKind::Kerf { material: "multiplex-3".into() },
        value_mm: value,
        proofs: vec![proof(id, "2026-10-01")],
    }
}

#[test]
fn a_plan_lists_the_calibrated_values_it_used_and_they_are_part_of_its_id() {
    let none = plan_with(&[]);
    let used = plan_with(&[applied(0.18, 1)]);
    assert!(none.calibration().is_empty());
    assert_eq!(used.calibration(), &[applied(0.18, 1)]);
    assert_ne!(none.id(), used.id(), "a plan made with a measurement is another plan");
    assert_ne!(used.id(), plan_with(&[applied(0.19, 1)]).id());
    assert_ne!(used.id(), plan_with(&[applied(0.18, 2)]).id(), "also the proof it came from");
    assert_eq!(used.id(), plan_with(&[applied(0.18, 1)]).id(), "deterministic");
}

#[test]
fn the_same_value_noted_twice_is_listed_once() {
    let plan = plan_with(&[applied(0.18, 1), applied(0.18, 1)]);
    assert_eq!(plan.calibration().len(), 1);
}
