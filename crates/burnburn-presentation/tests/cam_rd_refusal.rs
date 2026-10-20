//! Cross-hatch, dot mode and ramp length plan and preview, but RD output is refused with the
//! reason in words (W3-UI-cam, W2-L3): the host gets `RdRefused(reason)`, no file is written, the
//! document is untouched and nothing is replaced by another form.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_job::{Feature, MachineCapabilities, Support};
use burnburn_presentation::{
    CommandKey, CutExtras, CutField, HeadlessEditorHarness, HostEffect, MessageKey, OperationKind, RdRefusal,
    SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, OperationId};

use common::{RUIDA, profile};

fn layer(h: &mut HeadlessEditorHarness) -> (DocumentId, OperationId) {
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id: GeometryId = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let caps = [Feature::DotMode, Feature::RampLength]
        .into_iter()
        .fold(MachineCapabilities::conservative(), |c, f| c.with(f, Support::Supported));
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1).with_capabilities(caps).unwrap()), ruida: RUIDA });
    (doc, h.document(doc).unwrap().operations[0].id)
}

fn apply(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId, mode: OperationKind, extras: CutExtras) {
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let mut input = h.document(doc).unwrap().cut_settings.unwrap().input();
    input.mode = mode;
    input.extras = extras;
    h.send(UiIntent::ApplyCutSettings { document: doc, operation: op, settings: input });
    assert!(h.document(doc).unwrap().cut_settings.is_none(), "the sheet accepted the options");
}

/// Compiles and tries to write RD; returns the refusal the host is told.
fn export_refusal(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Option<MessageKey> {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let plan = h.document(doc).unwrap().plan.expect("the plan and its preview exist");
    assert_eq!(plan.errors, 0, "refusing RD is not a preflight error of the plan");
    let revision = h.document(doc).unwrap().revision;
    let path = std::path::PathBuf::from("/out/job.rd");
    h.send(UiIntent::ExportRd { document: doc, path: path.clone() });
    let updates = h.run_tasks();
    assert_eq!(h.document(doc).unwrap().revision, revision, "the document is untouched");
    assert!(burnburn_io::fs::FileSystem::read(h.fs(), &path, 1 << 20).is_err(), "no file was written");
    assert!(h.document(doc).unwrap().export.is_none());
    updates.iter().flat_map(|u| u.effects.iter()).find_map(|e| match e {
        HostEffect::ShowError { message, .. } => Some(*message),
        _ => None,
    })
}

#[test]
fn cross_hatch_is_refused_for_rd_in_words() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    apply(&mut h, doc, op, OperationKind::Fill, CutExtras::new().with_flag(CutField::FillCrossHatch, true));
    assert_eq!(export_refusal(&mut h, doc), Some(MessageKey::RdRefused(RdRefusal::CrossHatch)));
}

#[test]
fn dot_mode_is_refused_for_rd_in_words() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    apply(&mut h, doc, op, OperationKind::Line, CutExtras::new().with_flag(CutField::DotEnabled, true));
    assert_eq!(export_refusal(&mut h, doc), Some(MessageKey::RdRefused(RdRefusal::DotMode)));
}

#[test]
fn ramp_length_is_refused_for_rd_in_words() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    apply(&mut h, doc, op, OperationKind::Fill, CutExtras::new().with_text(CutField::FillRamp, "2"));
    assert_eq!(export_refusal(&mut h, doc), Some(MessageKey::RdRefused(RdRefusal::RampLength)));
}

#[test]
fn a_plain_fill_is_not_refused() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    apply(&mut h, doc, op, OperationKind::Fill, CutExtras::new());
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    h.send(UiIntent::ExportRd { document: doc, path: "/out/job.rd".into() });
    let updates = h.run_tasks();
    assert!(updates.iter().flat_map(|u| u.effects.iter()).all(|e| !matches!(e, HostEffect::ShowError { .. })));
    assert!(h.document(doc).unwrap().export.is_some());
}
