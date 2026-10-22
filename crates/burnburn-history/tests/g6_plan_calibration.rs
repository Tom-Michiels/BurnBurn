//! G6-machine, W09: a frozen job keeps the calibrated values its plan was made with. The plan blob
//! carries them, so the rebuilt plan has the id that was recorded; a blob whose measurement was
//! changed is refused.
#![allow(clippy::unwrap_used)]

mod common;

use burnburn_history::{HistoryError, decode_plan, encode_plan};
use burnburn_job::{
    AppliedCalibration, AppliedKind, JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings,
    OriginCorner, PlanEvent, PlanMode, PlanSource, PowerRatio, ProofRef,
};
use burnburn_types::{ContentHash, HashDomain, OperationId, RevisionId, StableId};
use common::*;

fn plan(value_mm: f64) -> JobPlan {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"plan-calibration"),
        machine_profile: profile(1).revision().clone(),
        compiler_version: "test".into(),
    };
    let mut b = JobPlanBuilder::new(
        source,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        MachinePoint::new(0.0, 0.0).unwrap(),
    );
    let s = b
        .add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s: 20.0,
            min_power: PowerRatio::new(0.5).unwrap(),
            max_power: PowerRatio::new(0.5).unwrap(),
            air_assist: false,
        })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Cut { to: MachinePoint::new(10.0, 0.0).unwrap(), origin: None }).unwrap();
    b.note_calibration(AppliedCalibration {
        operation: OperationId::from_raw(1),
        kind: AppliedKind::Kerf { material: "multiplex-3".into() },
        value_mm: value_mm,
        proofs: vec![ProofRef { id: 4, date: "2026-10-01".into() }],
    });
    b.note_calibration(AppliedCalibration {
        operation: OperationId::from_raw(1),
        kind: AppliedKind::ScanOffset { speed_mm_s: 100.0 },
        value_mm: 0.2,
        proofs: vec![ProofRef { id: 5, date: "2026-10-02".into() }, ProofRef { id: 6, date: "2026-10-03".into() }],
    });
    b.finish().unwrap()
}

#[test]
fn the_plan_blob_carries_the_calibrated_values_and_rebuilds_the_same_plan() {
    let original = plan(0.18);
    let bytes = encode_plan(&original).unwrap();
    let back = decode_plan(&bytes, &original.id().hash()).unwrap();
    assert_eq!(back.id(), original.id());
    assert_eq!(back.calibration(), original.calibration());
}

#[test]
fn a_measurement_changed_in_the_blob_does_not_pass_for_the_recorded_plan() {
    let original = plan(0.18);
    let bytes = encode_plan(&original).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("0.18"), "the measurement is in the blob");
    let tampered = text.replace("0.18", "0.19");
    let refused = decode_plan(tampered.as_bytes(), &original.id().hash());
    assert!(matches!(refused, Err(HistoryError::BlobMismatch)), "{refused:?}");
}

#[test]
fn a_job_made_with_measurements_is_frozen_and_comes_back_from_the_history() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let plain = compile_session(&plate(), &profile(1));
    let calibrated = plan(0.18);
    let job = h
        .record_compiled(
            &fs,
            burnburn_history::CompileRecord {
                plan: &calibrated,
                rd_file: &plain.rd,
                project: &plain.project,
                project_path: None,
                preflight: plain.preflight.clone(),
                profile: profile(1).revision(),
            },
            t(0),
        )
        .unwrap();
    drop(h);
    let (h, report) = open(&fs);
    assert_eq!(report.corrupt, None);
    let back = h.load_plan(&fs, job).unwrap();
    assert_eq!(back.calibration(), calibrated.calibration(), "what the job was made with is still on record");
}
