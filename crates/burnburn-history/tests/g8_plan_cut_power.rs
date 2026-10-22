//! G8-cam-werk, Tab Cut Power (C04): the plan blob of the history keeps a plan with `CutPower`
//! events (a tab cut at its own power). The rebuilt plan has the recorded id, the event keeps its
//! power and origin, and a blob whose tab power was changed is refused.
#![allow(clippy::unwrap_used)]

mod common;

use burnburn_history::{HistoryError, decode_plan, encode_plan};
use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings,
    OriginCorner, PlanEvent, PlanMode, PlanSource, PowerRatio, SourceRef,
};
use burnburn_types::{ContentHash, GeometryId, HashDomain, OperationId, RevisionId, StableId};
use common::*;

fn plan(tab_power: f64) -> JobPlan {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"plan-cut-power"),
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
    let origin = Some(SourceRef { geometry: GeometryId::from_raw(3), operation: OperationId::from_raw(1) });
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Cut { to: MachinePoint::new(10.0, 0.0).unwrap(), origin }).unwrap();
    b.push(PlanEvent::CutPower {
        to: MachinePoint::new(12.0, 0.0).unwrap(),
        power: PowerRatio::new(tab_power).unwrap(),
        origin,
    })
    .unwrap();
    b.push(PlanEvent::Cut { to: MachinePoint::new(30.0, 0.0).unwrap(), origin }).unwrap();
    b.finish().unwrap()
}

fn cut_powers(plan: &JobPlan) -> Vec<(f64, f64, Option<SourceRef>)> {
    plan.events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::CutPower { to, power, origin } => Some((to.x_mm(), power.get(), origin)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_plan_blob_keeps_a_tab_cut_at_its_own_power_and_rebuilds_the_same_plan() {
    let original = plan(0.25);
    let bytes = encode_plan(&original).unwrap();
    let back = decode_plan(&bytes, &original.id().hash()).unwrap();
    assert_eq!(back.id(), original.id());
    assert_eq!(back.events(), original.events());
    let powers = cut_powers(&back);
    assert_eq!(powers.len(), 1);
    assert_eq!((powers[0].0, powers[0].1), (12.0, 0.25));
    assert_eq!(powers[0].2, Some(SourceRef { geometry: GeometryId::from_raw(3), operation: OperationId::from_raw(1) }));
}

#[test]
fn a_tab_power_changed_in_the_blob_does_not_pass_for_the_recorded_plan() {
    let original = plan(0.25);
    let text = String::from_utf8(encode_plan(&original).unwrap()).unwrap();
    assert!(text.contains("0.25"), "the tab power is in the blob");
    let refused = decode_plan(text.replace("0.25", "0.9").as_bytes(), &original.id().hash());
    assert!(matches!(refused, Err(HistoryError::BlobMismatch)), "{refused:?}");
}

#[test]
fn a_tab_power_outside_zero_to_one_is_refused_by_name_and_not_rebuilt() {
    let original = plan(0.25);
    let text = String::from_utf8(encode_plan(&original).unwrap()).unwrap();
    let refused = decode_plan(text.replace("0.25", "1.5").as_bytes(), &original.id().hash());
    assert!(matches!(refused, Err(HistoryError::PlanBlob("power"))), "{refused:?}");
}

#[test]
fn a_job_with_a_tab_cut_power_is_frozen_and_comes_back_from_the_history() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let plain = compile_session(&plate(), &profile(1));
    let tabbed = plan(0.25);
    let job = h
        .record_compiled(
            &fs,
            burnburn_history::CompileRecord {
                plan: &tabbed,
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
    assert_eq!(back.id(), tabbed.id());
    assert_eq!(cut_powers(&back).len(), 1, "the tab keeps its own power on record");
}
