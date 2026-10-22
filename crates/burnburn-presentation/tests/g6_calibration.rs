//! G6-machine, W09: the calibration proofs of the machine hub are kept (they outlive the session),
//! the active machine profile carries what they measured, the compiler uses it, and the plan says
//! which proofs it took its values from. Frozen jobs keep their own snapshot.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use burnburn_document::process::{FillSettings, OperationDraft, OperationMode, ScanAxis, ScanDirection};
use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_job::AppliedKind;
use burnburn_presentation::{
    CommandKey, FilesIntent, HeadlessEditorHarness, ProofInput, ProofKind, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, Length, OperationId};
use common::{RUIDA, cut, profile};

fn settings_for_test(speed: f64, power: f64) -> burnburn_document::process::ProcessSettings {
    use burnburn_document::process::{Passes, PowerPercent, PowerRange, ProcessSettings, Speed};
    let p = PowerPercent::new(power).unwrap();
    ProcessSettings {
        speed: Speed::mm_per_s(speed).unwrap(),
        power: PowerRange::new(p, p).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: true,
    }
}

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn start(fs: Arc<MemoryFileSystem>) -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::with_fs(fs);
    h.fs().create_dir_all_for_test(FsPath::new("/lib"));
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h.run_tasks();
    h
}

fn proof(kind: ProofKind, material: &str, speed: &str, value: &str) -> ProofInput {
    ProofInput {
        kind,
        material: material.into(),
        speed: speed.into(),
        value: value.into(),
        value_y: "-0,5".into(),
        method: "schuifmaat".into(),
        date: "2026-10-22".into(),
    }
}

fn record(h: &mut HeadlessEditorHarness, input: ProofInput) {
    h.send(UiIntent::RecordCalibration { input });
    h.run_tasks();
}

fn rows(h: &mut HeadlessEditorHarness) -> Vec<(u64, ProofKind, f64)> {
    h.controller().machine_hub_view().calibration.rows.iter().map(|r| (r.id, r.kind, r.value_mm)).collect()
}

#[test]
fn a_proof_is_still_there_after_the_app_is_restarted() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut first = start(Arc::clone(&fs));
    record(&mut first, proof(ProofKind::Kerf, "multiplex-3", "", "0,18"));
    record(&mut first, proof(ProofKind::ScanOffset, "", "100", "0,2"));
    record(&mut first, proof(ProofKind::PointerOffset, "", "", "1,5"));
    let before = rows(&mut first);
    assert_eq!(before.len(), 3);
    drop(first);

    let mut second = start(Arc::clone(&fs));
    assert_eq!(rows(&mut second), before, "the same proofs with the same ids");
    let view = second.controller().machine_hub_view().calibration;
    assert_eq!(
        (view.rows[0].material.as_str(), view.rows[0].method.as_str(), view.rows[0].date.as_str()),
        ("multiplex-3", "schuifmaat", "2026-10-22"),
        "what was measured, how and when"
    );
}

#[test]
fn a_removed_proof_stays_removed_and_ids_are_not_reused() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut first = start(Arc::clone(&fs));
    record(&mut first, proof(ProofKind::Kerf, "a", "", "0,1"));
    record(&mut first, proof(ProofKind::Kerf, "b", "", "0,2"));
    // The newest proof goes: its id is the highest, so only a remembered next id keeps it spent.
    let id = rows(&mut first)[1].0;
    first.send(UiIntent::RemoveCalibration { id });
    first.run_tasks();
    drop(first);

    let mut second = start(Arc::clone(&fs));
    assert_eq!(rows(&mut second).len(), 1);
    record(&mut second, proof(ProofKind::Kerf, "c", "", "0,3"));
    let ids: Vec<u64> = rows(&mut second).iter().map(|r| r.0).collect();
    assert!(ids.iter().all(|i| *i != id), "the id of a removed proof is not handed out again: {ids:?}");
}

#[test]
fn a_damaged_proof_file_is_left_alone_while_the_session_still_works() {
    let fs = Arc::new(MemoryFileSystem::new());
    fs.create_dir_all_for_test(FsPath::new("/lib"));
    fs.insert(FsPath::new("/lib/calibration.json"), b"{ not json".to_vec());
    let mut h = start(Arc::clone(&fs));
    record(&mut h, proof(ProofKind::Kerf, "a", "", "0,1"));
    assert_eq!(rows(&mut h).len(), 1, "usable in this session");
    assert_eq!(fs.read(FsPath::new("/lib/calibration.json"), 1 << 20).unwrap(), b"{ not json".to_vec());
}

#[test]
fn the_machines_proofs_do_not_leak_to_another_machine() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = start(Arc::clone(&fs));
    record(&mut h, proof(ProofKind::Kerf, "multiplex-3", "", "0,18"));
    let other = burnburn_job::MachineProfile::new(
        "tweede-machine",
        1,
        burnburn_job::MachineFrame::new(600.0, 400.0, burnburn_job::OriginCorner::BottomLeft).unwrap(),
        500.0,
        300.0,
        burnburn_job::PowerRatio::new(0.0).unwrap(),
        burnburn_job::PowerRatio::new(1.0).unwrap(),
    )
    .unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(other), ruida: RUIDA });
    h.run_tasks();
    assert!(
        h.controller().machine_profile().unwrap().calibration().is_empty(),
        "a measurement of one machine is not a calibration of the other"
    );
    h.send(UiIntent::ChooseMachineProfile { id: "research-1500x900".into() });
    h.run_tasks();
    assert_eq!(h.controller().machine_profile().unwrap().calibration().kerf().len(), 1, "and it is back with its machine");
}

#[test]
fn the_active_profile_carries_what_the_proofs_measured_and_is_a_new_revision() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    let base = h.controller().machine_profile().unwrap().revision().clone();
    record(&mut h, proof(ProofKind::Kerf, "multiplex-3", "", "0,18"));
    record(&mut h, proof(ProofKind::ScanOffset, "", "100", "0,2"));
    let p = h.controller().machine_profile().unwrap().clone();
    assert_eq!(p.revision().id, base.id);
    assert_ne!(p.revision().content, base.content, "a new setup revision");
    assert_eq!(p.calibration().kerf_for("multiplex-3").map(|k| k.kerf_mm), Some(0.18));
    assert!((p.calibration().scan_offset_at(100.0).unwrap().offset_mm - 0.2).abs() < 1e-12);
    // The pointer offset moves where the head is placed, not what is burnt: it is not in the profile.
    let before = p.revision().content;
    record(&mut h, proof(ProofKind::PointerOffset, "", "", "1,5"));
    assert_eq!(h.controller().machine_profile().unwrap().revision().content, before);
    assert_eq!(h.controller().pointer_offset_mm(), Some((1.5, -0.5)));
}

#[test]
fn the_newest_proof_of_a_material_counts() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    record(&mut h, proof(ProofKind::Kerf, "multiplex-3", "", "0,18"));
    let mut newer = proof(ProofKind::Kerf, "multiplex-3", "", "0,21");
    newer.date = "2026-11-02".into();
    record(&mut h, newer);
    let mut older = proof(ProofKind::Kerf, "multiplex-3", "", "0,99");
    older.date = "2026-01-02".into();
    record(&mut h, older);
    let k = h.controller().machine_profile().unwrap().calibration().kerf_for("multiplex-3").unwrap().clone();
    assert_eq!(k.kerf_mm, 0.21, "the latest date wins, whatever order the proofs were entered in");
}

// ---- the compiler uses them ---------------------------------------------------------------------------------

/// A window with a 20 × 1 rectangle and a bidirectional fill at 100 mm/s.
fn fill_window(h: &mut HeadlessEditorHarness) -> (DocumentId, OperationId) {
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (30.0, 11.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: SelectMode::Replace });
    let fill = FillSettings::new(mm(0.5), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0)).unwrap();
    h.send(UiIntent::AddOperation {
        document: doc,
        draft: OperationDraft {
            name: "Vullen".into(),
            mode: OperationMode::Fill(fill),
            settings: settings_for_test(100.0, 20.0),
            output_enabled: true,
        },
    });
    let op = h.document(doc).unwrap().operations[0].id;
    (doc, op)
}

fn compile(h: &mut HeadlessEditorHarness, doc: DocumentId) -> burnburn_presentation::PlanViewModel {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    h.document(doc).unwrap().plan.expect("a plan")
}

#[test]
fn a_scan_offset_proof_changes_the_next_compile_and_the_plan_says_where_it_came_from() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    let (doc, op) = fill_window(&mut h);
    let plain = compile(&mut h, doc);
    assert!(plain.calibration.is_empty());

    record(&mut h, proof(ProofKind::ScanOffset, "", "100", "0,2"));
    assert!(h.document(doc).unwrap().plan.unwrap().stale, "the plan was made for the profile without it");
    let plan = compile(&mut h, doc);
    assert!(!plan.stale);
    assert_ne!(plan.id, plain.id);
    let proof_id = rows(&mut h)[0].0;
    assert_eq!(plan.calibration.len(), 1);
    assert_eq!(plan.calibration[0].operation, op);
    assert_eq!(plan.calibration[0].kind, AppliedKind::ScanOffset { speed_mm_s: 100.0 });
    assert!((plan.calibration[0].value_mm - 0.2).abs() < 1e-12);
    assert_eq!(plan.calibration[0].proofs.iter().map(|p| p.id).collect::<Vec<_>>(), vec![proof_id]);
    assert_eq!(plan.calibration[0].proofs[0].date, "2026-10-22");
}

#[test]
fn removing_the_proof_takes_the_value_out_of_the_next_plan() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    let (doc, _) = fill_window(&mut h);
    let plain = compile(&mut h, doc);
    record(&mut h, proof(ProofKind::ScanOffset, "", "100", "0,2"));
    let used = compile(&mut h, doc);
    assert_ne!(used.id, plain.id);
    let id = rows(&mut h)[0].0;
    h.send(UiIntent::RemoveCalibration { id });
    h.run_tasks();
    assert!(h.document(doc).unwrap().plan.unwrap().stale);
    let again = compile(&mut h, doc);
    assert!(again.calibration.is_empty());
    assert_eq!(again.id, plain.id, "back to the plan of the machine without measurements");
}

#[test]
fn a_kerf_proof_for_the_material_of_an_operation_offsets_its_contours() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (110.0, 110.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: SelectMode::Replace });
    h.send(UiIntent::AddOperation { document: doc, draft: cut() });
    let op = h.document(doc).unwrap().operations[0].id;
    // The operation takes its settings from a library entry for multiplex-3.
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    h.run_tasks();
    h.send(UiIntent::Files(FilesIntent::SaveMaterial {
        document: doc,
        operation: op,
        name: "multiplex snijden".into(),
        material: "multiplex-3".into(),
        thickness: "3".into(),
        notes: String::new(),
    }));
    h.run_tasks();
    let entry = h.document(doc).unwrap().files.material.unwrap().entries[0].id;
    h.send(UiIntent::Files(FilesIntent::ApplyMaterial { document: doc, entry, operations: vec![op] }));
    h.run_tasks();

    let nominal = compile(&mut h, doc);
    assert!(nominal.calibration.is_empty());
    record(&mut h, proof(ProofKind::Kerf, "multiplex-3", "", "0,2"));
    let plan = compile(&mut h, doc);
    assert_eq!(plan.calibration.len(), 1);
    assert_eq!(plan.calibration[0].kind, AppliedKind::Kerf { material: "multiplex-3".into() });
    assert_eq!(plan.calibration[0].value_mm, 0.2);
    // The preview shows the contour 0.1 mm outside the 10..110 square.
    let bounds = plan
        .preview
        .moves
        .iter()
        .filter(|m| m.kind == burnburn_job::MoveKind::Cut)
        .fold(f64::INFINITY, |m, mv| m.min(mv.from.x_mm().min(mv.to.x_mm())));
    assert!((bounds - 9.9).abs() < 1e-3, "the beam centre runs 0.1 mm into the waste: {bounds}");
}

// ---- frozen jobs keep their snapshot -----------------------------------------------------------------------

#[test]
fn a_prepared_job_keeps_the_setup_it_was_made_for_when_a_proof_is_added() {
    let mut r = common::rig();
    let job = common::prepare(&mut r.h, r.doc);
    r.h.send(UiIntent::RecordCalibration { input: proof(ProofKind::ScanOffset, "", "200", "0,15") });
    let view = r.h.document(r.doc).unwrap();
    assert_eq!(view.job.map(|j| (j.plan, j.rd_hash)), Some((job.plan, job.rd_hash)), "the frozen job is untouched");
    assert!(view.plan.unwrap().stale);
}
