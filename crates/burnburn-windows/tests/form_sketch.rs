//! The sketch panel of the Windows window, headless (W4-L8a): the constraint list of the open
//! sketch with its value editor, freedom or conflict in words, Measure/Drive, Remove and the
//! parameters. Same rows and intents as the macOS panel (`sketch_panel.rs` there): edits go on as
//! typed text (DR-05).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::sketch::EntityId;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, SketchDimensionKind, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, Placement, sketch,
};
use burnburn_windows::l10n::Locale;

fn rectangle_sketch() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::SketchNew);
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    (h, doc)
}

fn select_side(h: &mut HeadlessEditorHarness, doc: DocumentId, side: u32) {
    h.send(UiIntent::SketchSelect {
        document: doc,
        entities: vec![EntityId::from_raw(side)],
        constraints: vec![],
        mode: SelectMode::Replace,
    });
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Option<Form> {
    let vm = h.view();
    sketch::form(&FormContext { vm: &vm, document: doc, locale })
}

fn react(
    h: &HeadlessEditorHarness,
    doc: DocumentId,
    id: ControlId,
    value: ControlValue,
    values: &[(ControlId, ControlValue)],
) -> FormOutcome {
    let vm = h.view();
    sketch::react(&id, &value, values, &FormContext { vm: &vm, document: doc, locale: Locale::En })
}

fn items(f: &Form) -> Vec<String> {
    match f.find(&ControlId::SketchList) {
        Some(FormLine::List { items, .. }) => items.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn there_is_no_panel_outside_sketch_mode() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert!(form(&h, doc, Locale::En).is_none());
}

#[test]
fn the_panel_lists_each_constraint_with_kind_and_status_in_words() {
    let (h, doc) = rectangle_sketch();
    let f = form(&h, doc, Locale::Nl).expect("panel");
    assert_eq!((f.kind, f.placement), (FormKind::Sketch, Placement::Floating));
    let rows = items(&f);
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|r| r.contains("voldaan")), "{rows:?}");
    assert!(rows[0].contains("Horizontaal") && rows[1].contains("Verticaal"));
    assert!(f.lines.iter().any(|l| matches!(l, FormLine::Text(t) if t.contains("Vrijheidsgraden"))));
    assert!(items(&form(&h, doc, Locale::En).unwrap())[0].contains("Horizontal"));
}

#[test]
fn selecting_list_rows_selects_constraints_and_the_same_rows_again_send_nothing() {
    let (mut h, doc) = rectangle_sketch();
    select_side(&mut h, doc, 5);
    let before = h.document(doc).unwrap().sketch_mode.unwrap();
    let out = react(&h, doc, ControlId::SketchList, ControlValue::Rows(vec![0, 2]), &[]);
    let FormOutcome::Intent(i) = out else { panic!("an intent") };
    assert!(matches!(i, UiIntent::SketchSelect { mode: SelectMode::Replace, .. }));
    h.send(i);
    let now = h.document(doc).unwrap().sketch_mode.unwrap();
    assert_eq!(now.selected_constraints, vec![before.rows[0].id, before.rows[2].id]);
    assert_eq!(now.selected_entities, vec![EntityId::from_raw(5)], "the entities stay selected");
    let f = form(&h, doc, Locale::En).unwrap();
    assert!(
        matches!(f.find(&ControlId::SketchList), Some(FormLine::List { selected, multi: true, .. }) if *selected == vec![0, 2])
    );
    assert_eq!(react(&h, doc, ControlId::SketchList, ControlValue::Rows(vec![0, 2]), &[]), FormOutcome::Nothing);
    // Remove applies to the selected constraints.
    assert!(f.ids().contains(&ControlId::SketchRemove));
    assert_eq!(
        react(&h, doc, ControlId::SketchRemove, ControlValue::Press, &[]),
        FormOutcome::Intent(UiIntent::Invoke { document: Some(doc), command: CommandKey::SketchRemoveConstraint })
    );
}

#[test]
fn a_dimension_has_a_value_editor_that_sends_the_raw_text_once() {
    let (mut h, doc) = rectangle_sketch();
    select_side(&mut h, doc, 4);
    h.send(UiIntent::AddSketchDimension { document: doc, kind: SketchDimensionKind::Auto, text: "20 mm".into() });
    let vm = h.document(doc).unwrap().sketch_mode.unwrap();
    let row = vm.rows.len() - 1;
    let dimension = vm.rows[row].clone();
    // The new dimension is selected: its value becomes editable.
    if let FormOutcome::Intent(i) = react(&h, doc, ControlId::SketchList, ControlValue::Rows(vec![row]), &[]) {
        h.send(i);
    }
    let f = form(&h, doc, Locale::En).unwrap();
    assert!(
        matches!(f.find(&ControlId::SketchValue), Some(FormLine::Field { text, enabled: true, .. }) if text == "20 mm")
    );
    assert_eq!(
        react(&h, doc, ControlId::SketchValue, ControlValue::Text("breedte + 5 mm".into()), &[]),
        FormOutcome::Intent(UiIntent::SetSketchDimension {
            document: doc,
            sketch: vm.sketch,
            constraint: dimension.id,
            text: "breedte + 5 mm".into()
        }),
        "raw text, not parsed by the host"
    );
    assert_eq!(react(&h, doc, ControlId::SketchValue, ControlValue::Text("20 mm".into()), &[]), FormOutcome::Nothing);
    // Measure / Drive toggles the dimension's role.
    assert!(matches!(
        react(&h, doc, ControlId::SketchRole, ControlValue::Press, &[]),
        FormOutcome::Intent(UiIntent::SetSketchDimensionRole { .. })
    ));
}

#[test]
fn a_relation_has_no_value_editor_and_parameters_are_set_as_typed() {
    let (mut h, doc) = rectangle_sketch();
    let out = react(&h, doc, ControlId::SketchList, ControlValue::Rows(vec![0]), &[]);
    let FormOutcome::Intent(i) = out else { panic!() };
    h.send(i);
    let f = form(&h, doc, Locale::En).unwrap();
    assert!(matches!(f.find(&ControlId::SketchValue), Some(FormLine::Field { enabled: false, .. })));
    assert_eq!(react(&h, doc, ControlId::SketchRole, ControlValue::Press, &[]), FormOutcome::Nothing);
    let sketch_id = h.document(doc).unwrap().sketch_mode.unwrap().sketch;
    let values = vec![
        (ControlId::SketchParamName, ControlValue::Text(" breedte ".into())),
        (ControlId::SketchParamText, ControlValue::Text("150 mm".into())),
    ];
    assert_eq!(
        react(&h, doc, ControlId::SketchParamSet, ControlValue::Press, &values),
        FormOutcome::Intent(UiIntent::SetSketchParameter {
            document: doc,
            sketch: sketch_id,
            name: "breedte".into(),
            text: "150 mm".into()
        })
    );
    assert_eq!(
        react(
            &h,
            doc,
            ControlId::SketchParamSet,
            ControlValue::Press,
            &[(ControlId::SketchParamName, ControlValue::Text("  ".into()))]
        ),
        FormOutcome::Nothing,
        "a parameter needs a name"
    );
    h.send(UiIntent::SetSketchParameter {
        document: doc,
        sketch: sketch_id,
        name: "breedte".into(),
        text: "150 mm".into(),
    });
    let f = form(&h, doc, Locale::En).unwrap();
    let ids = f.ids();
    let n = ids.iter().position(|i| matches!(i, ControlId::SketchParam(_))).expect("an editor for the parameter");
    let ControlId::SketchParam(k) = ids[n].clone() else { panic!("a parameter id") };
    assert!(
        matches!(f.find(&ids[n]), Some(FormLine::Field { caption, text, .. }) if caption == "breedte" && text.contains("150"))
    );
    assert_eq!(
        react(&h, doc, ControlId::SketchParam(k), ControlValue::Text("200 mm".into()), &[]),
        FormOutcome::Intent(UiIntent::SetSketchParameter {
            document: doc,
            sketch: sketch_id,
            name: "breedte".into(),
            text: "200 mm".into()
        })
    );
}
