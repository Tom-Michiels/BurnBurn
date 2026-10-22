//! FE01 (G5-cam): the Feeder dialog (Laser ▸ Feeder…): repeat, feed before, feed after and a
//! pause between the repeats. The settings belong to the window (not to the project); the plan
//! and its preview follow them, RD output of such a plan is refused in words.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_test_support::host::rig as common;

use burnburn_presentation::{
    CamDialog, CommandKey, DialogAction, DialogActionKind, DialogError, DialogField, DialogNote, DialogValue,
    DialogViewModel, HeadlessEditorHarness, HostEffect, MessageKey, RdRefusal, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId};

use common::{RUIDA, profile};

const FEEDER: CamDialog = CamDialog::Feeder;

fn rig() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (50.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id: GeometryId = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    (h, doc)
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: FEEDER });
    h.controller().cam_dialog(doc, FEEDER).expect("the dialog is open")
}

fn vm(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Option<DialogViewModel> {
    h.controller().cam_dialog(doc, FEEDER)
}

fn text(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, value: &str) {
    h.send(UiIntent::SetCamField { document: doc, dialog: FEEDER, field, value: DialogValue::Text(value.into()) });
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, action: DialogAction) {
    h.send(UiIntent::CamDialogAction { document: doc, dialog: FEEDER, action });
}

fn set_all(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    text(h, doc, DialogField::FeederRepeat, "3");
    text(h, doc, DialogField::FeederFeedBefore, "20");
    text(h, doc, DialogField::FeederFeedAfter, "30");
    text(h, doc, DialogField::FeederPause, "2");
    text(h, doc, DialogField::FeederSpeed, "50");
    press(h, doc, DialogAction::Apply);
}

fn compile(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
}

#[test]
fn the_dialog_starts_plain_and_applying_closes_it() {
    let (mut h, doc) = rig();
    let shown = open(&mut h, doc);
    assert_eq!(shown.text(DialogField::FeederRepeat), "1");
    assert_eq!(shown.text(DialogField::FeederFeedBefore), "0");
    assert_eq!(shown.text(DialogField::FeederFeedAfter), "0");
    assert_eq!(shown.text(DialogField::FeederPause), "0");
    assert!(!shown.notes.contains(&DialogNote::FeederHasNoRdForm), "a plain feeder asks nothing of RD");
    let revision = h.document(doc).unwrap().revision;
    set_all(&mut h, doc);
    assert!(vm(&mut h, doc).is_none(), "Apply closes the dialog");
    assert_eq!(h.document(doc).unwrap().revision, revision, "the feeder is not project data");
    // Reopened, it shows what is in force.
    let shown = open(&mut h, doc);
    assert_eq!(shown.text(DialogField::FeederRepeat), "3");
    assert_eq!(shown.text(DialogField::FeederFeedAfter), "30");
    assert!(shown.notes.contains(&DialogNote::FeederHasNoRdForm));
}

#[test]
fn the_plan_and_its_estimate_follow_the_feeder() {
    let (mut h, doc) = rig();
    compile(&mut h, doc);
    let plain = h.document(doc).unwrap().plan.unwrap();
    assert_eq!((plain.preview.feed_length_mm, plain.preview.pause_seconds), (0.0, 0.0));

    open(&mut h, doc);
    set_all(&mut h, doc);
    let stale = h.document(doc).unwrap().plan.unwrap();
    assert!(stale.stale, "a plan made without the feeder is out of date once the feeder changes");
    compile(&mut h, doc);
    let fed = h.document(doc).unwrap().plan.unwrap();
    assert!(!fed.stale);
    assert_eq!((fed.preview.feed_length_mm, fed.preview.pause_seconds), (150.0, 4.0));
    assert_ne!(fed.id, plain.id);
    assert_eq!(fed.errors, 0, "the feeder adds no preflight problem");
    let cut = plain.preview.cut_length_mm;
    assert!((fed.preview.cut_length_mm - 3.0 * cut).abs() < 1e-9, "three runs of the same job");
}

#[test]
fn a_feeder_plan_is_refused_for_rd_in_words_and_no_file_is_written() {
    let (mut h, doc) = rig();
    open(&mut h, doc);
    set_all(&mut h, doc);
    compile(&mut h, doc);
    let path = std::path::PathBuf::from("/out/job.rd");
    h.send(UiIntent::ExportRd { document: doc, path: path.clone() });
    let updates = h.run_tasks();
    let refusal = updates.iter().flat_map(|u| u.effects.iter()).find_map(|e| match e {
        HostEffect::ShowError { message, .. } => Some(*message),
        _ => None,
    });
    assert_eq!(refusal, Some(MessageKey::RdRefused(RdRefusal::Feeder)));
    assert!(burnburn_io::fs::FileSystem::read(h.fs(), &path, 1 << 20).is_err(), "no file was written");
}

#[test]
fn back_to_plain_makes_the_job_writable_again() {
    let (mut h, doc) = rig();
    open(&mut h, doc);
    set_all(&mut h, doc);
    open(&mut h, doc);
    text(&mut h, doc, DialogField::FeederRepeat, "1");
    text(&mut h, doc, DialogField::FeederFeedBefore, "0");
    text(&mut h, doc, DialogField::FeederFeedAfter, "0");
    press(&mut h, doc, DialogAction::Apply);
    compile(&mut h, doc);
    let path = std::path::PathBuf::from("/out/job.rd");
    h.send(UiIntent::ExportRd { document: doc, path: path.clone() });
    h.run_tasks();
    assert!(h.document(doc).unwrap().export.is_some(), "a plain job is written");
}

#[test]
fn bad_values_are_said_on_their_row_and_change_nothing() {
    let (mut h, doc) = rig();
    open(&mut h, doc);
    text(&mut h, doc, DialogField::FeederRepeat, "0");
    text(&mut h, doc, DialogField::FeederFeedBefore, "-1");
    text(&mut h, doc, DialogField::FeederFeedAfter, "abc");
    text(&mut h, doc, DialogField::FeederPause, "4000");
    text(&mut h, doc, DialogField::FeederSpeed, "0");
    let shown = vm(&mut h, doc).unwrap();
    assert_eq!(shown.row(DialogField::FeederRepeat).unwrap().error, Some(DialogError::OutOfRange));
    assert_eq!(shown.row(DialogField::FeederFeedBefore).unwrap().error, Some(DialogError::OutOfRange));
    assert_eq!(shown.row(DialogField::FeederFeedAfter).unwrap().error, Some(DialogError::Invalid));
    assert_eq!(shown.row(DialogField::FeederPause).unwrap().error, Some(DialogError::OutOfRange));
    assert_eq!(shown.row(DialogField::FeederSpeed).unwrap().error, Some(DialogError::OutOfRange));
    assert!(!shown.action(DialogActionKind::Apply).unwrap().enabled);
    text(&mut h, doc, DialogField::FeederRepeat, "2,5");
    assert_eq!(vm(&mut h, doc).unwrap().row(DialogField::FeederRepeat).unwrap().error, Some(DialogError::NotWhole));
    press(&mut h, doc, DialogAction::Apply);
    assert!(vm(&mut h, doc).is_some(), "the dialog stays open");
    // The settings in force did not change.
    press(&mut h, doc, DialogAction::Close);
    assert_eq!(open(&mut h, doc).text(DialogField::FeederRepeat), "1");
}
