//! G8-editor panels, headless on the Mac: adding a step to a modifier chain from the panel (E06)
//! and the stack of the selection in the inspector panel. Field text goes to presentation
//! unparsed (DR-05). The Windows host shows the same lines (`burnburn-windows/tests/g8_editor_forms.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::l10n::{Locale, message_or_key};
use burnburn_macos::panels::{
    EditorControl as E, G5Control as C, G5Line, G5Panel, g5_intent, g5_opens_by_itself, g5_panel,
};
use burnburn_presentation::{
    BooleanKind, CommandKey, HeadlessEditorHarness, MessageKey, ModifierKind, NewModifierStep, SelectMode,
    StructureEdit, StructureProblem, UiIntent,
};
use burnburn_types::DocumentId;

/// A window with a rectangle that has an offset chain, and the rectangle selected.
fn window_with_chain() -> (HeadlessEditorHarness, DocumentId, u64) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (100.0, 50.0), (130.0, 70.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    let values = [(C::Editor(E::ModOffsetDistance), "2 mm".to_owned())];
    let make = g5_intent(&h.view(), doc, G5Panel::Structure, C::Editor(E::ModOffsetMake), &values).unwrap();
    h.send(make);
    let id = h.document(doc).unwrap().structure.modifiers[0].id;
    (h, doc, id)
}

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, panel: G5Panel, locale: Locale) -> Vec<G5Line> {
    g5_panel(&h.view(), doc, panel, locale, &[]).expect("the panel").lines
}

fn intent(
    h: &HeadlessEditorHarness,
    doc: DocumentId,
    panel: G5Panel,
    control: E,
    values: &[(E, &str)],
) -> Option<UiIntent> {
    let values: Vec<(C, String)> = values.iter().map(|(c, v)| (C::Editor(*c), (*v).to_owned())).collect();
    g5_intent(&h.view(), doc, panel, C::Editor(control), &values)
}

fn edit_of(intent: Option<UiIntent>) -> StructureEdit {
    match intent {
        Some(UiIntent::Structure { edit, .. }) => edit,
        other => panic!("a structure edit, got {other:?}"),
    }
}

fn find(all: &[G5Line], control: E) -> &G5Line {
    all.iter().find(|l| l.control() == Some(C::Editor(control))).unwrap_or_else(|| panic!("{control:?} missing"))
}

#[test]
fn the_structure_panel_has_the_add_block_with_a_kind_choice_numbers_and_buttons() {
    let (h, doc, m) = window_with_chain();
    let all = lines(&h, doc, G5Panel::Structure, Locale::En);
    let G5Line::Choice { options, selected, caption, .. } = find(&all, E::ModNewKind) else { panic!("a choice") };
    assert_eq!(caption, "Kind of step");
    assert_eq!(options.len(), 7);
    assert_eq!((options[0].as_str(), options[5].as_str(), options[6].as_str()), ("Offset", "Grid", "Circle"));
    assert_eq!(*selected, 0);
    for control in [
        E::ModNewDistance,
        E::ModNewColumns,
        E::ModNewRows,
        E::ModNewPitchX,
        E::ModNewPitchY,
        E::ModNewCount,
        E::ModNewCenterX,
        E::ModNewCenterY,
        E::ModNewSweep,
    ] {
        assert!(matches!(find(&all, control), G5Line::Field { .. }), "{control:?}");
    }
    let G5Line::Button { title, enabled, .. } = find(&all, E::ModAddStep { modifier: m }) else { panic!("a button") };
    assert!(*enabled);
    assert!(title.starts_with("Add the step to"), "{title}");
    assert!(matches!(find(&all, E::ModNewChain), G5Line::Button { enabled: true, .. }), "the rectangle is selected");
    let nl = lines(&h, doc, G5Panel::Structure, Locale::Nl);
    assert!(matches!(find(&nl, E::ModNewKind), G5Line::Choice { caption, .. } if caption == "Soort stap"));
}

#[test]
fn the_new_chain_button_waits_for_a_selection() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let all = lines(&h, doc, G5Panel::Structure, Locale::En);
    assert!(matches!(find(&all, E::ModNewChain), G5Line::Button { enabled: false, .. }));
}

#[test]
fn a_kind_and_its_numbers_become_one_add_step_edit_with_the_text_unparsed() {
    let (h, doc, m) = window_with_chain();
    let go = |kind: &str, extra: &[(E, &str)]| {
        let mut values = vec![(E::ModNewKind, kind)];
        values.extend_from_slice(extra);
        edit_of(intent(&h, doc, G5Panel::Structure, E::ModAddStep { modifier: m }, &values))
    };
    assert_eq!(
        go("0", &[(E::ModNewDistance, "1,5 mm")]),
        StructureEdit::AddModifierStep { modifier: m, step: NewModifierStep::Offset { distance: "1,5 mm".into() } }
    );
    for (index, op) in
        [(1, BooleanKind::Union), (2, BooleanKind::Difference), (3, BooleanKind::Intersection), (4, BooleanKind::Xor)]
    {
        assert_eq!(
            go(&index.to_string(), &[]),
            StructureEdit::AddModifierStep { modifier: m, step: NewModifierStep::Boolean { op } }
        );
    }
    assert_eq!(
        go("5", &[(E::ModNewColumns, "3"), (E::ModNewRows, "2"), (E::ModNewPitchX, "12 mm"), (E::ModNewPitchY, "-8"),],),
        StructureEdit::AddModifierStep {
            modifier: m,
            step: NewModifierStep::Grid {
                columns: "3".into(),
                rows: "2".into(),
                pitch_x: "12 mm".into(),
                pitch_y: "-8".into()
            }
        }
    );
    assert_eq!(
        go("6", &[(E::ModNewCount, "6"), (E::ModNewCenterX, "10"), (E::ModNewCenterY, "-2"), (E::ModNewSweep, "180")],),
        StructureEdit::AddModifierStep {
            modifier: m,
            step: NewModifierStep::Circle {
                count: "6".into(),
                center_x: "10".into(),
                center_y: "-2".into(),
                sweep: "180".into()
            }
        }
    );
    // The same kind as a new chain on the selection.
    assert_eq!(
        edit_of(intent(&h, doc, G5Panel::Structure, E::ModNewChain, &[(E::ModNewKind, "1")])),
        StructureEdit::CreateModifierChain { step: NewModifierStep::Boolean { op: BooleanKind::Union } }
    );
}

#[test]
fn an_add_button_of_a_chain_that_is_gone_sends_nothing() {
    let (h, doc, _) = window_with_chain();
    assert_eq!(intent(&h, doc, G5Panel::Structure, E::ModAddStep { modifier: 999 }, &[]), None);
}

#[test]
fn the_buttons_add_a_step_and_a_circle_step_reaches_the_document() {
    let (mut h, doc, m) = window_with_chain();
    let go = intent(
        &h,
        doc,
        G5Panel::Structure,
        E::ModAddStep { modifier: m },
        &[
            (E::ModNewKind, "6"),
            (E::ModNewCount, "3"),
            (E::ModNewCenterX, "0"),
            (E::ModNewCenterY, "0"),
            (E::ModNewSweep, "360"),
        ],
    )
    .unwrap();
    h.send(go);
    let steps = &h.document(doc).unwrap().structure.modifiers[0].steps;
    assert_eq!(steps.iter().map(|s| s.kind).collect::<Vec<_>>(), [ModifierKind::Offset, ModifierKind::CircularArray]);
    // A refusal is said in the window language.
    let bad = intent(
        &h,
        doc,
        G5Panel::Structure,
        E::ModAddStep { modifier: m },
        &[(E::ModNewKind, "6"), (E::ModNewCount, "3"), (E::ModNewSweep, "0")],
    )
    .unwrap();
    h.send(bad);
    let problem = h.document(doc).unwrap().problem.map(|p| p.message);
    assert_eq!(problem, Some(MessageKey::Structure(StructureProblem::InvalidSweep)));
    assert_eq!(
        message_or_key(problem.unwrap(), Locale::En),
        "A sweep is an angle between 0 and 360 degrees, not zero."
    );
    let operands = {
        h.invoke(doc, CommandKey::SelectAll);
        let go = intent(&h, doc, G5Panel::Structure, E::ModAddStep { modifier: m }, &[(E::ModNewKind, "1")]).unwrap();
        h.send(go);
        h.document(doc).unwrap().problem.map(|p| p.message)
    };
    assert_eq!(operands, Some(MessageKey::Structure(StructureProblem::NeedsOperands)));
    assert_eq!(
        message_or_key(operands.unwrap(), Locale::Nl),
        "Een booleanstap heeft andere geselecteerde objecten nodig om mee te combineren."
    );
}

// --- The stack of the selection ("inspector" panel) -----------------------------------------------

#[test]
fn the_stack_panel_opens_by_itself_for_a_selection_that_belongs_to_a_chain_only() {
    assert!(g5_opens_by_itself(G5Panel::Stack));
    let (mut h, doc, m) = window_with_chain();
    // The rectangle (the chain's source) is selected.
    let model = g5_panel(&h.view(), doc, G5Panel::Stack, Locale::En, &[]).expect("a chain belongs to the selection");
    assert_eq!(model.title, "Stack of the selection");
    assert!(model.lines.iter().any(|l| l.control() == Some(C::Editor(E::ModStepOn { modifier: m, step: 0 }))));
    assert!(model.lines.iter().any(|l| l.control() == Some(C::Editor(E::ModAddStep { modifier: m }))));
    assert!(model.lines.iter().any(|l| l.control() == Some(C::Editor(E::ModNewKind))));
    // Nothing selected: no panel.
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    assert!(g5_panel(&h.view(), doc, G5Panel::Stack, Locale::En, &[]).is_none());
    // Its controls do what the structure panel's do.
    h.invoke(doc, CommandKey::SelectAll);
    let toggle = intent(&h, doc, G5Panel::Stack, E::ModStepOn { modifier: m, step: 0 }, &[]);
    assert_eq!(toggle, None, "no value typed: an unchanged check is no edit");
    let nl = g5_panel(&h.view(), doc, G5Panel::Stack, Locale::Nl, &[]).unwrap();
    assert_eq!(nl.title, "Stapel van de selectie");
}

#[test]
fn a_plain_object_without_a_chain_has_no_stack_panel() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (100.0, 50.0), (130.0, 70.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    assert!(g5_panel(&h.view(), doc, G5Panel::Stack, Locale::En, &[]).is_none());
}

#[test]
fn every_panel_is_listed_once() {
    let mut all = G5Panel::ALL.to_vec();
    all.dedup();
    assert_eq!(all.len(), 13);
    assert!(all.contains(&G5Panel::Stack));
}
