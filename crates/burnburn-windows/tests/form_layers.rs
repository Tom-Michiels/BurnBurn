//! The Layers panel of the Windows window, headless (W4-L8a): the 30-colour palette, the layer list
//! with the editors of the selected layer, the add row and the way into Cut Settings. Same rows,
//! words and intents as the macOS layer table (`layer_panels.rs` there).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, LAYER_SLOTS, OperationField, OperationKind, SelectMode, UiIntent,
};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState, Placement, layers,
};
use burnburn_windows::l10n::{HostText, Locale, host_text};
use burnburn_windows::panels::LayerColumn;

/// One 40 x 20 rectangle, selected.
fn rectangle() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    (h, doc)
}

fn ctx<'a>(vm: &'a burnburn_presentation::AppViewModel, doc: DocumentId, locale: Locale) -> FormContext<'a> {
    FormContext { vm, document: doc, locale }
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId, state: &PanelState, locale: Locale) -> Form {
    let vm = h.view();
    layers::form(&ctx(&vm, doc, locale), state).expect("a layers form")
}

fn react(
    h: &HeadlessEditorHarness,
    doc: DocumentId,
    state: &PanelState,
    id: ControlId,
    value: ControlValue,
    values: &[(ControlId, ControlValue)],
) -> FormOutcome {
    let vm = h.view();
    layers::react(&id, &value, values, &ctx(&vm, doc, Locale::En), state)
}

/// Send what a control event led to, as the host does.
fn act(h: &mut HeadlessEditorHarness, outcome: FormOutcome) -> Option<UiIntent> {
    match outcome {
        FormOutcome::Intent(i) => {
            h.send(i.clone());
            Some(i)
        }
        _ => None,
    }
}

fn assign(h: &mut HeadlessEditorHarness, doc: DocumentId, slot: u8) {
    let outcome = react(h, doc, &PanelState::default(), ControlId::Palette(slot), ControlValue::Press, &[]);
    act(h, outcome).unwrap();
}

fn list_items(form: &Form) -> Vec<String> {
    match form.find(&ControlId::LayerList) {
        Some(FormLine::List { items, .. }) => items.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_panel_docks_left_and_offers_one_button_per_palette_colour_that_marks_used_layers() {
    let (mut h, doc) = rectangle();
    assign(&mut h, doc, 1);
    let f = form(&h, doc, &PanelState::default(), Locale::En);
    assert_eq!((f.kind, f.placement), (FormKind::Layers, Placement::DockLeft));
    let ids = f.ids();
    for slot in 0..LAYER_SLOTS {
        assert!(ids.contains(&ControlId::Palette(slot)), "colour {slot}");
    }
    let Some(FormLine::Grid { buttons, columns }) = f.find(&ControlId::Palette(0)) else { panic!("a palette grid") };
    assert_eq!(buttons.len(), usize::from(LAYER_SLOTS));
    assert!(*columns >= 5);
    assert_eq!(buttons[1].title, "C01");
    assert!(buttons[1].pressed, "C01 has a layer");
    assert!(!buttons[0].pressed, "C00 is still empty");
}

#[test]
fn a_palette_click_assigns_the_layer_and_shift_adds_a_second_treatment() {
    let (h, doc) = rectangle();
    let st = PanelState::default();
    assert_eq!(
        react(&h, doc, &st, ControlId::Palette(3), ControlValue::Press, &[]),
        FormOutcome::Intent(UiIntent::AssignLayer { document: doc, slot: 3, add: false })
    );
    assert_eq!(
        react(&h, doc, &st, ControlId::Palette(3), ControlValue::ShiftPress, &[]),
        FormOutcome::Intent(UiIntent::AssignLayer { document: doc, slot: 3, add: true })
    );
    assert_eq!(
        react(&h, doc, &st, ControlId::Palette(LAYER_SLOTS), ControlValue::Press, &[]),
        FormOutcome::Nothing,
        "no button beyond the palette"
    );
}

#[test]
fn the_list_has_one_line_per_layer_and_an_empty_note_before_the_first() {
    let (mut h, doc) = rectangle();
    let st = PanelState::default();
    let empty = form(&h, doc, &st, Locale::Nl);
    assert!(list_items(&empty).is_empty());
    assert!(empty.lines.iter().any(|l| matches!(l, FormLine::Text(t) if *t == host_text(HostText::NoOperations, Locale::Nl))), "an empty note");
    assign(&mut h, doc, 2);
    let f = form(&h, doc, &st, Locale::Nl);
    let items = list_items(&f);
    assert_eq!(items.len(), 1);
    for part in ["C02", "Lijn", "100", "1 object"] {
        assert!(items[0].contains(part), "{part} in {:?}", items[0]);
    }
    // The first layer is not selected until the user picks it.
    assert!(matches!(f.find(&ControlId::LayerList), Some(FormLine::List { selected, .. }) if selected.is_empty()));
    assert!(!f.ids().contains(&ControlId::LayerField(LayerColumn::Name)), "no editors without a selected layer");
}

#[test]
fn picking_a_row_selects_it_and_shows_its_editors_with_the_view_model_values() {
    let (mut h, doc) = rectangle();
    assign(&mut h, doc, 2);
    assert_eq!(
        react(&h, doc, &PanelState::default(), ControlId::LayerList, ControlValue::Rows(vec![0]), &[]),
        FormOutcome::SelectLayerRow(Some(0))
    );
    assert_eq!(
        react(&h, doc, &PanelState::default(), ControlId::LayerList, ControlValue::Rows(vec![]), &[]),
        FormOutcome::SelectLayerRow(None)
    );
    let st = PanelState { layer_row: Some(0), ..PanelState::default() };
    let f = form(&h, doc, &st, Locale::En);
    let text = |c| match f.find(&ControlId::LayerField(c)) {
        Some(FormLine::Field { text, caption, .. }) => (text.clone(), caption.clone()),
        other => panic!("{c:?}: {other:?}"),
    };
    assert_eq!(text(LayerColumn::Speed).0, "100");
    assert_eq!(text(LayerColumn::Passes).0, "1");
    assert_eq!(text(LayerColumn::Speed).1, LayerColumn::Speed.caption(Locale::En));
    assert!(matches!(f.find(&ControlId::LayerField(LayerColumn::Output)), Some(FormLine::Check { on: true, .. })));
    assert!(matches!(f.find(&ControlId::LayerField(LayerColumn::Visible)), Some(FormLine::Check { on: true, .. })));
    // A row that no longer exists shows no editors.
    let gone = PanelState { layer_row: Some(7), ..PanelState::default() };
    assert!(!form(&h, doc, &gone, Locale::En).ids().contains(&ControlId::LayerField(LayerColumn::Speed)));
}

#[test]
fn an_editor_commit_sets_one_operation_field_as_typed_and_the_same_text_is_none() {
    let (mut h, doc) = rectangle();
    assign(&mut h, doc, 2);
    let op = h.document(doc).unwrap().operations[0].id;
    let st = PanelState { layer_row: Some(0), ..PanelState::default() };
    let commit = |h: &HeadlessEditorHarness, c, t: &str| {
        react(h, doc, &st, ControlId::LayerField(c), ControlValue::Text(t.into()), &[])
    };
    assert_eq!(
        commit(&h, LayerColumn::Speed, "250 mm/s"),
        FormOutcome::Intent(UiIntent::SetOperationField {
            document: doc,
            operation: op,
            field: OperationField::Speed,
            text: "250 mm/s".into()
        })
    );
    assert_eq!(
        commit(&h, LayerColumn::Power, "40"),
        FormOutcome::Intent(UiIntent::SetOperationField {
            document: doc,
            operation: op,
            field: OperationField::PowerMax,
            text: "40".into()
        }),
        "the power column is the maximum"
    );
    assert_eq!(commit(&h, LayerColumn::Speed, "100"), FormOutcome::Nothing, "Return plus focus loss is one commit");
    // A refused text stays on the panel, and typing it again is a retry.
    let outcome = commit(&h, LayerColumn::Speed, "snel");
    act(&mut h, outcome).unwrap();
    assert!(form(&h, doc, &st, Locale::En).lines.iter().any(|l| matches!(l, FormLine::Problem(p) if !p.is_empty())));
    assert!(matches!(commit(&h, LayerColumn::Speed, "snel"), FormOutcome::Intent(_)));
    // A column without an editor sends nothing.
    assert_eq!(commit(&h, LayerColumn::Targets, "9"), FormOutcome::Nothing);
}

#[test]
fn the_output_and_visible_checks_become_their_own_intents_and_only_when_they_change() {
    let (mut h, doc) = rectangle();
    assign(&mut h, doc, 2);
    let op = h.document(doc).unwrap().operations[0].id;
    let st = PanelState { layer_row: Some(0), ..PanelState::default() };
    let toggle = |h: &HeadlessEditorHarness, c, on| {
        react(h, doc, &st, ControlId::LayerField(c), ControlValue::Toggle(on), &[])
    };
    assert_eq!(
        toggle(&h, LayerColumn::Visible, false),
        FormOutcome::Intent(UiIntent::SetOperationVisible { document: doc, operation: op, visible: false })
    );
    assert_eq!(
        toggle(&h, LayerColumn::Output, false),
        FormOutcome::Intent(UiIntent::SetOperationOutput { document: doc, operation: op, enabled: false })
    );
    assert_eq!(toggle(&h, LayerColumn::Output, true), FormOutcome::Nothing, "already on");
    let outcome = toggle(&h, LayerColumn::Visible, false);
    act(&mut h, outcome).unwrap();
    let row = &h.document(doc).unwrap().operations[0];
    assert_eq!((row.output_enabled, row.visible), (true, false));
    assert!(matches!(
        form(&h, doc, &st, Locale::En).find(&ControlId::LayerField(LayerColumn::Visible)),
        Some(FormLine::Check { on: false, .. })
    ));
}

#[test]
fn double_click_and_the_button_open_cut_settings_of_the_layer() {
    let (mut h, doc) = rectangle();
    assign(&mut h, doc, 1);
    let op = h.document(doc).unwrap().operations[0].id;
    let open = UiIntent::OpenCutSettings { document: doc, operation: op };
    assert_eq!(
        react(&h, doc, &PanelState::default(), ControlId::LayerList, ControlValue::Activate(0), &[]),
        FormOutcome::Intent(open.clone())
    );
    assert_eq!(
        react(&h, doc, &PanelState::default(), ControlId::LayerList, ControlValue::Activate(4), &[]),
        FormOutcome::Nothing
    );
    let st = PanelState { layer_row: Some(0), ..PanelState::default() };
    assert_eq!(
        react(&h, doc, &st, ControlId::LayerCutSettings, ControlValue::Press, &[]),
        FormOutcome::Intent(open)
    );
    assert_eq!(
        react(&h, doc, &PanelState::default(), ControlId::LayerCutSettings, ControlValue::Press, &[]),
        FormOutcome::Nothing,
        "no layer selected"
    );
}

#[test]
fn the_add_row_sends_name_kind_speed_and_power_as_typed() {
    let (mut h, doc) = rectangle();
    let values = vec![
        (ControlId::LayerAddName, ControlValue::Text("Gravure".into())),
        (ControlId::LayerAddKind, ControlValue::Choice(1)),
        (ControlId::LayerAddSpeed, ControlValue::Text("300 mm/s".into())),
        (ControlId::LayerAddPower, ControlValue::Text("45 %".into())),
    ];
    let outcome = react(&h, doc, &PanelState::default(), ControlId::LayerAdd, ControlValue::Press, &values);
    assert_eq!(
        outcome,
        FormOutcome::Intent(UiIntent::AddOperationText {
            document: doc,
            name: "Gravure".into(),
            kind: OperationKind::Fill,
            speed: "300 mm/s".into(),
            power: "45 %".into(),
        })
    );
    act(&mut h, outcome).unwrap();
    assert_eq!(h.document(doc).unwrap().operations.len(), 1);
    // A kind that is not on offer sends nothing.
    let bad = vec![(ControlId::LayerAddKind, ControlValue::Choice(9))];
    assert_eq!(react(&h, doc, &PanelState::default(), ControlId::LayerAdd, ControlValue::Press, &bad), FormOutcome::Nothing);
}
