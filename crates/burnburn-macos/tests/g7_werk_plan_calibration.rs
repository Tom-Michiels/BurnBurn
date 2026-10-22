//! G7-werk, W09: the plan panel shows the measured values the plan used and the proof each came
//! from (`PlanViewModel.calibration`), in both languages. A plan without measurements shows none.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

#[path = "../../burnburn-presentation/tests/common/mod.rs"]
mod common;

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use burnburn_io::memory::MemoryFileSystem;
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::plan_panel;
use burnburn_presentation::{
    CommandKey, FilesIntent, HeadlessEditorHarness, ProofInput, ProofKind, SelectMode, UiIntent,
};
use burnburn_types::DocumentId;
use common::*;

/// A window with a 100 x 100 square whose cut operation takes its settings from a multiplex entry.
fn window_with_material() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::with_fs(Arc::new(MemoryFileSystem::new()));
    h.fs().create_dir_all_for_test(FsPath::new("/lib"));
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h.run_tasks();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (110.0, 110.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: SelectMode::Replace });
    h.send(UiIntent::AddOperation { document: doc, draft: cut() });
    let op = h.document(doc).unwrap().operations[0].id;
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
    (h, doc)
}

fn compile(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
}

#[test]
fn a_plan_without_measurements_shows_no_calibration_lines() {
    let (mut h, doc) = window_with_material();
    compile(&mut h, doc);
    assert!(plan_panel(&h.view(), doc, Locale::Nl).calibration.is_empty());
}

#[test]
fn the_plan_panel_names_the_measured_kerf_and_its_proof_in_both_languages() {
    let (mut h, doc) = window_with_material();
    h.send(UiIntent::RecordCalibration {
        input: ProofInput {
            kind: ProofKind::Kerf,
            material: "multiplex-3".into(),
            speed: String::new(),
            value: "0,2".into(),
            value_y: String::new(),
            method: "schuifmaat".into(),
            date: "2026-10-22".into(),
        },
    });
    h.run_tasks();
    compile(&mut h, doc);
    let applied = h.document(doc).unwrap().plan.unwrap().calibration;
    assert_eq!(applied.len(), 1, "the compiler used the kerf");
    let proof = applied[0].proofs[0].id.to_string();

    let nl = plan_panel(&h.view(), doc, Locale::Nl).calibration;
    assert_eq!(nl, vec![format!("Kerf 0,2 mm voor multiplex-3 (proef {proof} van 2026-10-22)")]);
    let en = plan_panel(&h.view(), doc, Locale::En).calibration;
    assert_eq!(en, vec![format!("Kerf 0.2 mm for multiplex-3 (proof {proof} of 2026-10-22)")]);
}
