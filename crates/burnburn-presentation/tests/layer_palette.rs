//! LightBurn-like layers in presentation (B10/B12, C01–C05): the colour palette assigns the
//! selection to the operation of that colour, the Cut Settings sheet edits one operation with raw
//! text (DR-05), visibility and output are separate toggles, and the canvas draws each object in
//! its layer colour. Headless, through the real AppController.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::UndoLabel;
use burnburn_document::process::ProcessLabel;
use burnburn_editor::StyleRole;
use burnburn_job::{Feature, MachineCapabilities, Support};
use burnburn_presentation::{
    CutSettingsInput, CutSettingsViewModel, DocumentViewModel, FieldKey, HeadlessEditorHarness, MessageKey,
    OperationField, OperationKind, OperationRow, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, OperationId};

use common::{RUIDA, profile};

/// A document with two rectangles `a` (40 × 20) and `b`, nothing selected.
fn two_rectangles(h: &mut HeadlessEditorHarness) -> (DocumentId, GeometryId, GeometryId) {
    let doc = h.new_document().expect("window");
    h.invoke(doc, burnburn_presentation::CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.drag(doc, (60.0, 0.0), (100.0, 20.0));
    h.invoke(doc, burnburn_presentation::CommandKey::ToolSelect);
    let ids: Vec<_> = vm(h, doc).objects.iter().map(|o| o.id).collect();
    assert_eq!(ids.len(), 2);
    select(h, doc, &[]);
    (doc, ids[0], ids[1])
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

fn click_colour(h: &mut HeadlessEditorHarness, doc: DocumentId, slot: u8, add: bool) {
    h.send(UiIntent::AssignLayer { document: doc, slot, add });
}

fn row(h: &HeadlessEditorHarness, doc: DocumentId, slot: u8) -> Option<OperationRow> {
    vm(h, doc).operations.into_iter().find(|o| o.slot == slot)
}

fn sheet(h: &HeadlessEditorHarness, doc: DocumentId) -> Option<CutSettingsViewModel> {
    vm(h, doc).cut_settings
}

fn input(s: &CutSettingsViewModel) -> CutSettingsInput {
    s.input()
}

fn apply(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId, settings: CutSettingsInput) {
    h.send(UiIntent::ApplyCutSettings { document: doc, operation: op, settings });
}

fn roles(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<(GeometryId, StyleRole)> {
    h.scene(doc).unwrap().items.iter().map(|i| (i.source, i.role)).collect()
}

#[test]
fn colour_click_with_selection_creates_a_layer_and_moves_objects_between_colours() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, a, b) = two_rectangles(&mut h);
    let rev = vm(&h, doc).revision;
    click_colour(&mut h, doc, 1, false);
    assert_eq!(vm(&h, doc).revision, rev, "no selection: no command");
    assert_eq!(vm(&h, doc).operation_error, Some(MessageKey::SelectObjectsFirst));

    select(&mut h, doc, &[a]);
    click_colour(&mut h, doc, 1, false);
    let v = vm(&h, doc);
    assert_eq!(v.revision.raw(), rev.raw() + 1, "a new layer is one command");
    assert_eq!(v.operation_error, None);
    let blue = row(&h, doc, 1).expect("layer of colour 1");
    assert_eq!((blue.name.as_str(), blue.kind, blue.targets), ("C01", OperationKind::Line, 1));
    assert!(blue.output_enabled && blue.visible);

    select(&mut h, doc, &[b]);
    click_colour(&mut h, doc, 1, false);
    assert_eq!(vm(&h, doc).operations.len(), 1, "the same colour is the same layer");
    assert_eq!(row(&h, doc, 1).unwrap().targets, 2);

    // `a` to colour 2: it leaves colour 1 (an object has one layer unless Shift is held).
    select(&mut h, doc, &[a]);
    click_colour(&mut h, doc, 2, false);
    let v = vm(&h, doc);
    assert_eq!(v.operations.len(), 2);
    assert_eq!(row(&h, doc, 1).unwrap().targets, 1, "only b stays in colour 1");
    assert_eq!(row(&h, doc, 2).unwrap().targets, 1);
    assert_eq!(roles(&h, doc).iter().find(|(id, _)| *id == b).map(|(_, r)| *r), Some(StyleRole::ProcessLayer(1)));
    let rev = v.revision;
    click_colour(&mut h, doc, 2, false);
    assert_eq!(vm(&h, doc).revision, rev, "already in that colour: nothing");
}

#[test]
fn shift_colour_click_adds_a_second_treatment_to_the_same_geometry() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, a, _) = two_rectangles(&mut h);
    select(&mut h, doc, &[a]);
    click_colour(&mut h, doc, 0, false);
    let rev = vm(&h, doc).revision;
    click_colour(&mut h, doc, 3, true);
    let v = vm(&h, doc);
    assert_eq!(v.revision.raw(), rev.raw() + 1, "one command for the second treatment");
    assert_eq!(v.objects.len(), 2, "no geometry copied");
    assert_eq!(row(&h, doc, 0).unwrap().targets, 1, "the first treatment keeps the object");
    assert_eq!(row(&h, doc, 3).unwrap().targets, 1);

    // Both treatments follow an edit of the one source (C02).
    select(&mut h, doc, &[a]);
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "50".into() });
    let v = vm(&h, doc);
    assert_eq!(v.inspector.unwrap().field(FieldKey::Width).unwrap().text, "50");
    assert_eq!((row(&h, doc, 0).unwrap().targets, row(&h, doc, 3).unwrap().targets), (1, 1));
    select(&mut h, doc, &[]);
    // Drawn in the colour of its first operation in execution order.
    assert_eq!(roles(&h, doc).iter().find(|(id, _)| *id == a).map(|(_, r)| *r), Some(StyleRole::ProcessLayer(0)));
}

#[test]
fn visibility_and_output_toggle_independently() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, a, b) = two_rectangles(&mut h);
    select(&mut h, doc, &[a]);
    click_colour(&mut h, doc, 1, false);
    select(&mut h, doc, &[]);
    let op = row(&h, doc, 1).unwrap().id;
    let before = vm(&h, doc);

    h.send(UiIntent::SetOperationVisible { document: doc, operation: op, visible: false });
    let v = vm(&h, doc);
    assert_eq!((v.revision, v.undo), (before.revision, before.undo), "visibility is no document command");
    let r = row(&h, doc, 1).unwrap();
    assert_eq!((r.visible, r.output_enabled), (false, true));
    let drawn: Vec<_> = roles(&h, doc).into_iter().map(|(id, _)| id).collect();
    assert_eq!(drawn, vec![b], "objects of a hidden layer are not drawn");

    h.send(UiIntent::SetOperationOutput { document: doc, operation: op, enabled: false });
    let v = vm(&h, doc);
    assert_eq!(v.revision.raw(), before.revision.raw() + 1, "output is one UpdateOperation");
    assert_eq!(v.undo, Some(UndoLabel::Process(ProcessLabel::UpdateOperation)));
    let r = row(&h, doc, 1).unwrap();
    assert_eq!((r.visible, r.output_enabled), (false, false), "output does not touch visibility");

    h.send(UiIntent::SetOperationVisible { document: doc, operation: op, visible: true });
    let r = row(&h, doc, 1).unwrap();
    assert_eq!((r.visible, r.output_enabled), (true, false), "visibility does not touch output");
    assert_eq!(roles(&h, doc).len(), 2);

    h.invoke(doc, burnburn_presentation::CommandKey::Undo);
    let r = row(&h, doc, 1).unwrap();
    assert_eq!((r.visible, r.output_enabled), (true, true), "undo restores output only");
    let rev = vm(&h, doc).revision;
    h.send(UiIntent::SetOperationOutput { document: doc, operation: op, enabled: true });
    assert_eq!(vm(&h, doc).revision, rev, "unchanged output: no command");
}

#[test]
fn cut_settings_sheet_applies_raw_text_in_one_command_and_keeps_errors_open() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, a, _) = two_rectangles(&mut h);
    select(&mut h, doc, &[a]);
    click_colour(&mut h, doc, 1, false);
    let op = row(&h, doc, 1).unwrap().id;
    assert_eq!(sheet(&h, doc), None);
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let s = sheet(&h, doc).expect("sheet open");
    assert_eq!((s.operation, s.slot, s.name.as_str(), s.mode), (op, 1, "C01", OperationKind::Line));
    assert_eq!(
        (s.speed.as_str(), s.power_min.as_str(), s.power_max.as_str(), s.passes.as_str(), s.kerf.as_str()),
        ("100", "20", "20", "1", "0")
    );
    let modes: Vec<_> = s.modes.iter().map(|m| (m.kind, m.blocked)).collect();
    assert_eq!(
        modes,
        vec![(OperationKind::Line, None), (OperationKind::Fill, None), (OperationKind::OffsetFill, None)],
        "without a machine profile the conservative matrix allows these modes"
    );
    assert!(s.output_enabled && s.visible && s.error.is_none());
    let rev = vm(&h, doc).revision;

    let refused = |h: &mut HeadlessEditorHarness, change: &dyn Fn(&mut CutSettingsInput), field: OperationField| {
        let mut i = input(&sheet(h, doc).unwrap());
        change(&mut i);
        apply(h, doc, op, i);
        let s = sheet(h, doc).expect("an error keeps the sheet open");
        assert_eq!(s.error, Some(MessageKey::InvalidNumber(field)));
        assert_eq!(vm(h, doc).revision, rev, "{field:?}: no command");
    };
    refused(&mut h, &|i| i.speed = "snel".into(), OperationField::Speed);
    refused(&mut h, &|i| i.power_max = "120 %".into(), OperationField::PowerMax);
    refused(
        &mut h,
        &|i| {
            i.power_min = "60".into();
            i.power_max = "40".into();
        },
        OperationField::PowerMin,
    );
    refused(&mut h, &|i| i.passes = "0".into(), OperationField::Passes);
    refused(&mut h, &|i| i.passes = "2,5".into(), OperationField::Passes);

    let mut i = input(&sheet(&h, doc).unwrap());
    i.mode = OperationKind::Fill;
    i.speed = "300 mm/s".into();
    i.power_min = "10 %".into();
    i.power_max = "35,5".into();
    i.passes = "2".into();
    apply(&mut h, doc, op, i);
    let v = vm(&h, doc);
    assert_eq!(v.cut_settings, None, "valid input closes the sheet");
    assert_eq!(v.revision.raw(), rev.raw() + 1, "all fields in one command");
    assert_eq!(v.undo, Some(UndoLabel::Process(ProcessLabel::UpdateOperation)));
    let r = row(&h, doc, 1).unwrap();
    assert_eq!(
        (r.kind, r.speed.as_str(), r.power_min.as_str(), r.power.as_str(), r.passes.as_str()),
        (OperationKind::Fill, "300", "10", "35.5", "2")
    );

    // Unchanged values: no command; Cancel: no command either.
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let same = input(&sheet(&h, doc).unwrap());
    apply(&mut h, doc, op, same);
    assert_eq!((vm(&h, doc).revision.raw(), sheet(&h, doc)), (rev.raw() + 1, None));
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    h.send(UiIntent::CancelCutSettings { document: doc });
    assert_eq!((vm(&h, doc).revision.raw(), sheet(&h, doc)), (rev.raw() + 1, None));

    h.invoke(doc, burnburn_presentation::CommandKey::Undo);
    let r = row(&h, doc, 1).unwrap();
    assert_eq!((r.kind, r.speed.as_str(), r.passes.as_str()), (OperationKind::Line, "100", "1"), "one undo");
}

#[test]
fn kerf_field_sets_line_kerf_in_one_command() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, a, _) = two_rectangles(&mut h);
    select(&mut h, doc, &[a]);
    click_colour(&mut h, doc, 2, false);
    let op = row(&h, doc, 2).unwrap().id;
    let rev = vm(&h, doc).revision;
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let mut i = input(&sheet(&h, doc).unwrap());
    i.kerf = "-0,1 mm".into();
    apply(&mut h, doc, op, i.clone());
    assert_eq!(sheet(&h, doc).unwrap().error, Some(MessageKey::InvalidNumber(OperationField::Kerf)));
    assert_eq!(vm(&h, doc).revision, rev);

    i.kerf = "0,15 mm".into();
    apply(&mut h, doc, op, i);
    assert_eq!(vm(&h, doc).revision.raw(), rev.raw() + 1);
    assert_eq!(row(&h, doc, 2).unwrap().kerf, "0.15");
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    assert_eq!(sheet(&h, doc).unwrap().kerf, "0.15", "the sheet shows the stored kerf");

    // Kerf belongs to Line: switching to Fill drops it.
    let mut i = input(&sheet(&h, doc).unwrap());
    i.mode = OperationKind::Fill;
    apply(&mut h, doc, op, i);
    let r = row(&h, doc, 2).unwrap();
    assert_eq!((r.kind, r.kerf.as_str()), (OperationKind::Fill, "0"));
}

#[test]
fn unvalidated_mode_is_blocked_with_missing_capability() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, a, _) = two_rectangles(&mut h);
    let caps = MachineCapabilities::conservative()
        .with(Feature::OffsetFill, Support::Unsupported)
        .with(Feature::KerfCompensation, Support::Unverified { reason: "kerf not measured".into() });
    let limited = profile(1).with_capabilities(caps).unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(limited), ruida: RUIDA });
    select(&mut h, doc, &[a]);
    click_colour(&mut h, doc, 1, false);
    let op = row(&h, doc, 1).unwrap().id;
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let s = sheet(&h, doc).unwrap();
    let modes: Vec<_> = s.modes.iter().map(|m| (m.kind, m.blocked)).collect();
    assert_eq!(
        modes,
        vec![
            (OperationKind::Line, None),
            (OperationKind::Fill, None),
            (OperationKind::OffsetFill, Some(MessageKey::MissingCapability(Feature::OffsetFill))),
        ],
        "the blocked mode stays visible with its reason"
    );
    let rev = vm(&h, doc).revision;
    let mut i = input(&s);
    i.mode = OperationKind::OffsetFill;
    apply(&mut h, doc, op, i.clone());
    assert_eq!(sheet(&h, doc).unwrap().error, Some(MessageKey::MissingCapability(Feature::OffsetFill)));
    assert_eq!(vm(&h, doc).revision, rev, "blocked mode: no command");

    i.mode = OperationKind::Line;
    i.kerf = "0,1".into();
    apply(&mut h, doc, op, i.clone());
    assert_eq!(sheet(&h, doc).unwrap().error, Some(MessageKey::MissingCapability(Feature::KerfCompensation)));
    assert_eq!(vm(&h, doc).revision, rev, "unverified kerf: no command");

    i.mode = OperationKind::Fill;
    i.kerf = String::new();
    apply(&mut h, doc, op, i);
    assert_eq!(row(&h, doc, 1).unwrap().kind, OperationKind::Fill, "a supported mode is applied");
}

#[test]
fn objects_are_drawn_in_their_layer_colour_and_hidden_layers_are_not() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, a, b) = two_rectangles(&mut h);
    select(&mut h, doc, &[a]);
    click_colour(&mut h, doc, 4, false);
    select(&mut h, doc, &[]);
    assert_eq!(roles(&h, doc), vec![(a, StyleRole::ProcessLayer(4)), (b, StyleRole::Design)]);
    select(&mut h, doc, &[a]);
    assert_eq!(roles(&h, doc)[0], (a, StyleRole::Selected), "selection outranks the layer colour");

    // An operation from the operations panel gets the lowest free colour, and that colour's
    // palette button then assigns to it.
    select(&mut h, doc, &[b]);
    h.send(UiIntent::AddOperationText {
        document: doc,
        name: "Graveren".into(),
        kind: OperationKind::Fill,
        speed: "200".into(),
        power: "15".into(),
    });
    let fill = vm(&h, doc).operations.into_iter().find(|o| o.name == "Graveren").unwrap();
    assert_eq!(fill.slot, 0);
    select(&mut h, doc, &[a]);
    click_colour(&mut h, doc, 0, true);
    assert_eq!(vm(&h, doc).operations.len(), 2, "colour 0 is the panel's operation");
    assert_eq!(row(&h, doc, 0).unwrap().targets, 2);
    select(&mut h, doc, &[]);
    assert_eq!(roles(&h, doc), vec![(a, StyleRole::ProcessLayer(4)), (b, StyleRole::ProcessLayer(0))]);
    h.send(UiIntent::SetOperationVisible { document: doc, operation: fill.id, visible: false });
    assert_eq!(roles(&h, doc), vec![(a, StyleRole::ProcessLayer(4))], "a stays visible through colour 4");
}

#[test]
fn layer_table_edits_power_min_passes_and_kerf_as_raw_text() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, a, _) = two_rectangles(&mut h);
    select(&mut h, doc, &[a]);
    click_colour(&mut h, doc, 1, false);
    let op = row(&h, doc, 1).unwrap().id;
    let rev = vm(&h, doc).revision;
    let set = |h: &mut HeadlessEditorHarness, field, text: &str| {
        h.send(UiIntent::SetOperationField { document: doc, operation: op, field, text: text.into() });
    };
    set(&mut h, OperationField::PowerMin, "30");
    assert_eq!(vm(&h, doc).operation_error, Some(MessageKey::InvalidNumber(OperationField::PowerMin)), "min > max");
    set(&mut h, OperationField::PowerMin, "5 %");
    set(&mut h, OperationField::Passes, "3");
    set(&mut h, OperationField::Kerf, "0,2 mm");
    let r = row(&h, doc, 1).unwrap();
    assert_eq!((r.power_min.as_str(), r.passes.as_str(), r.kerf.as_str()), ("5", "3", "0.2"));
    assert_eq!(vm(&h, doc).revision.raw(), rev.raw() + 3, "one command per field");
    set(&mut h, OperationField::Passes, "3");
    assert_eq!(vm(&h, doc).revision.raw(), rev.raw() + 3, "unchanged: no command");
}
