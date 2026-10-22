//! C04 (G5-cam): Cut Settings gets "Z per pass" (greyed with its reason without a Z axis, one
//! `SetAdvanced`) and says at the tab field when the tabs no longer fit the shape after the source
//! changed, before any compile fails on it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::UndoLabel;
use burnburn_document::process::ProcessLabel;
use burnburn_job::{Feature, MachineCapabilities, Support};
use burnburn_presentation::{
    CommandKey, CutExtras, CutField, CutSettingsViewModel, FieldKey, FieldState, HeadlessEditorHarness, MessageKey,
    OperationKind, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, OperationId};

use common::{RUIDA, profile};

fn layer(h: &mut HeadlessEditorHarness) -> (DocumentId, OperationId, GeometryId) {
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id: GeometryId = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    (doc, h.document(doc).unwrap().operations[0].id, id)
}

fn with_z(h: &mut HeadlessEditorHarness, z: Support) {
    let caps = MachineCapabilities::conservative().with(Feature::ZStep, z).with(Feature::Tabs, Support::Supported);
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile(1).with_capabilities(caps).unwrap()),
        ruida: RUIDA,
    });
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId) -> CutSettingsViewModel {
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    h.document(doc).unwrap().cut_settings.expect("sheet open")
}

fn ok(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId, extras: CutExtras) -> Option<MessageKey> {
    let mut input = open(h, doc, op).input();
    input.extras = extras;
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
fn z_per_pass_is_greyed_without_a_z_axis_and_one_command_with_one() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op, _) = layer(&mut h);
    with_z(&mut h, Support::Unsupported);
    let sheet = open(&mut h, doc, op);
    assert_eq!(sheet.extras.text(CutField::ZStep), "0");
    assert_eq!(
        sheet.extras.state(CutField::ZStep, OperationKind::Line, &sheet.gates),
        FieldState::Disabled(Some(MessageKey::MissingCapability(Feature::ZStep)))
    );
    h.send(UiIntent::CancelCutSettings { document: doc });
    let before = h.document(doc).unwrap().revision;
    let refused = ok(&mut h, doc, op, CutExtras::new().with_text(CutField::ZStep, "0,5"));
    assert_eq!(refused, Some(MessageKey::MissingCapability(Feature::ZStep)));
    assert_eq!(h.document(doc).unwrap().revision, before);

    with_z(&mut h, Support::Supported);
    assert_eq!(ok(&mut h, doc, op, CutExtras::new().with_text(CutField::ZStep, "0,5")), None);
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Process(ProcessLabel::SetAdvanced)));
    assert_eq!(open(&mut h, doc, op).extras.text(CutField::ZStep), "0.5");
    h.send(UiIntent::CancelCutSettings { document: doc });
    for bad in ["abc", "51", "-51"] {
        assert_eq!(
            ok(&mut h, doc, op, CutExtras::new().with_text(CutField::ZStep, bad)),
            Some(MessageKey::InvalidCutField(CutField::ZStep)),
            "{bad:?}"
        );
    }
    assert_eq!(ok(&mut h, doc, op, CutExtras::new().with_text(CutField::ZStep, "0")), None);
    assert_eq!(open(&mut h, doc, op).extras.text(CutField::ZStep), "0", "0 removes the step");
}

#[test]
fn tabs_that_no_longer_fit_are_said_at_the_tab_field() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op, shape) = layer(&mut h);
    with_z(&mut h, Support::Supported);
    // Two tabs of 8 mm on the 40 x 20 rectangle (perimeter 120) fit.
    let tabs = CutExtras::new()
        .with_flag(CutField::TabsEnabled, true)
        .with_text(CutField::TabsValue, "2")
        .with_text(CutField::TabsWidth, "8");
    assert_eq!(ok(&mut h, doc, op, tabs), None);
    let sheet = open(&mut h, doc, op);
    assert!(sheet.gates.advisory.get(&CutField::TabsValue).is_none(), "{:?}", sheet.gates.advisory);
    h.send(UiIntent::CancelCutSettings { document: doc });

    // The source shrinks to 3 x 3 mm (perimeter 12): 2 x 8 mm of tabs cover all of it.
    h.send(UiIntent::Select { document: doc, ids: vec![shape], mode: SelectMode::Replace });
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "3".into() });
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Height, text: "3".into() });
    let sheet = open(&mut h, doc, op);
    assert_eq!(sheet.gates.advisory.get(&CutField::TabsValue), Some(&MessageKey::TabsDoNotFit));
    h.send(UiIntent::CancelCutSettings { document: doc });
    // Compiling says no, as before; the sheet said it first.
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    assert!(h.document(doc).unwrap().messages.iter().any(|m| m.key == MessageKey::CompileFailed));
}
