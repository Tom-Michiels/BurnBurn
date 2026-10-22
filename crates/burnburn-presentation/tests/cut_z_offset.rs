//! Z offset per layer in Cut Settings (W3-UI-cam, W2-L3): a text field that is one `SetAdvanced`,
//! greyed with its reason when the machine profile has no Z axis, and refused outside ±50 mm.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_test_support::host::rig as common;

use burnburn_document::UndoLabel;
use burnburn_document::process::ProcessLabel;
use burnburn_job::{Feature, MachineCapabilities, Support};
use burnburn_presentation::{
    CommandKey, CutExtras, CutField, CutSettingsViewModel, FieldState, HeadlessEditorHarness, MessageKey,
    OperationKind, SelectMode, UiIntent,
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
    (doc, h.document(doc).unwrap().operations[0].id)
}

fn with_z(h: &mut HeadlessEditorHarness, z: Support) {
    let caps = MachineCapabilities::conservative().with(Feature::ZStep, z);
    let p = profile(1).with_capabilities(caps).unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(p), ruida: RUIDA });
}

fn rev(h: &HeadlessEditorHarness, doc: DocumentId) -> u64 {
    h.document(doc).unwrap().revision.raw()
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId) -> CutSettingsViewModel {
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    h.document(doc).unwrap().cut_settings.expect("sheet open")
}

fn ok(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId, text: &str) -> Option<MessageKey> {
    let mut input = open(h, doc, op).input();
    input.extras = CutExtras::new().with_text(CutField::ZOffset, text);
    h.send(UiIntent::ApplyCutSettings { document: doc, operation: op, settings: input });
    let still_open = h.document(doc).unwrap().cut_settings;
    if still_open.is_some() {
        let error = still_open.and_then(|s| s.error);
        h.send(UiIntent::CancelCutSettings { document: doc });
        return error;
    }
    None
}

#[test]
fn the_z_offset_field_is_greyed_with_its_reason_without_a_z_axis() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    with_z(&mut h, Support::Unsupported);
    let sheet = open(&mut h, doc, op);
    assert_eq!(sheet.extras.text(CutField::ZOffset), "0", "no offset shows 0");
    assert_eq!(
        sheet.extras.state(CutField::ZOffset, OperationKind::Line, &sheet.gates),
        FieldState::Disabled(Some(MessageKey::MissingCapability(Feature::ZStep)))
    );
    // Typing into the greyed field is refused with the same reason and sends nothing.
    h.send(UiIntent::CancelCutSettings { document: doc });
    let before = rev(&h, doc);
    assert_eq!(ok(&mut h, doc, op, "1,5"), Some(MessageKey::MissingCapability(Feature::ZStep)));
    assert_eq!(rev(&h, doc), before);
}

#[test]
fn a_z_offset_is_one_set_advanced_and_shows_again_after_reopening() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    with_z(&mut h, Support::Supported);
    let before = rev(&h, doc);
    assert_eq!(ok(&mut h, doc, op, "1,5"), None);
    assert_eq!(rev(&h, doc), before + 1, "one command");
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Process(ProcessLabel::SetAdvanced)));
    let sheet = open(&mut h, doc, op);
    assert_eq!(sheet.extras.text(CutField::ZOffset), "1.5");
    assert_eq!(sheet.extras.state(CutField::ZOffset, OperationKind::Line, &sheet.gates), FieldState::Editable);
    h.send(UiIntent::CancelCutSettings { document: doc });

    // The same text again is not a command; 0 or an empty field removes the offset.
    assert_eq!(ok(&mut h, doc, op, "1,5"), None);
    assert_eq!(rev(&h, doc), before + 1);
    assert_eq!(ok(&mut h, doc, op, "0"), None);
    assert_eq!(rev(&h, doc), before + 2);
    assert_eq!(open(&mut h, doc, op).extras.text(CutField::ZOffset), "0");
}

#[test]
fn negative_offsets_are_valid_and_the_bounds_are_fifty_millimetres() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    with_z(&mut h, Support::Supported);
    assert_eq!(ok(&mut h, doc, op, "-50"), None);
    assert_eq!(open(&mut h, doc, op).extras.text(CutField::ZOffset), "-50");
    h.send(UiIntent::CancelCutSettings { document: doc });
    let before = rev(&h, doc);
    for text in ["50,1", "-51", "abc"] {
        let error = ok(&mut h, doc, op, text);
        assert_eq!(error, Some(MessageKey::InvalidCutField(CutField::ZOffset)), "{text:?}");
        assert_eq!(rev(&h, doc), before, "{text:?} sends no command");
    }
}
