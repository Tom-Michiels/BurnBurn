//! G9-editor E06 panel, headless on the Mac: the add-a-step block also offers a linear series and a
//! series along a path. Field text goes to presentation unparsed (DR-05). The Windows host shows the
//! same lines (`burnburn-windows/tests/g9_editor_series.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{EditorControl as E, G5Control as C, G5Line, G5Panel, g5_intent, g5_panel};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, NewModifierStep, StructureEdit, UiIntent};
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

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<G5Line> {
    g5_panel(&h.view(), doc, G5Panel::Structure, locale, &[]).expect("the panel").lines
}

fn find(all: &[G5Line], control: E) -> Option<&G5Line> {
    all.iter().find(|l| l.control() == Some(C::Editor(control)))
}

fn edit_of(h: &HeadlessEditorHarness, doc: DocumentId, control: E, values: &[(E, &str)]) -> StructureEdit {
    let values: Vec<(C, String)> = values.iter().map(|(c, v)| (C::Editor(*c), (*v).to_owned())).collect();
    match g5_intent(&h.view(), doc, G5Panel::Structure, C::Editor(control), &values) {
        Some(UiIntent::Structure { edit, .. }) => edit,
        other => panic!("a structure edit, got {other:?}"),
    }
}

#[test]
fn the_kind_choice_lists_the_series_and_their_fields_are_there_in_both_languages() {
    let (h, doc, _) = window_with_chain();
    let all = lines(&h, doc, Locale::En);
    let Some(G5Line::Choice { options, .. }) = find(&all, E::ModNewKind) else { panic!("a choice") };
    assert_eq!(options.len(), 9);
    assert_eq!((options[7].as_str(), options[8].as_str()), ("Linear series", "Series along a path"));
    for control in [E::ModNewLinearCount, E::ModNewStepX, E::ModNewStepY, E::ModNewPathCount] {
        assert!(matches!(find(&all, control), Some(G5Line::Field { .. })), "{control:?}");
    }
    assert!(matches!(find(&all, E::ModNewPathOrient), Some(G5Line::Check { on: true, .. })));
    let nl = lines(&h, doc, Locale::Nl);
    let Some(G5Line::Choice { options, .. }) = find(&nl, E::ModNewKind) else { panic!("a choice") };
    assert_eq!((options[7].as_str(), options[8].as_str()), ("Lineaire reeks", "Reeks langs een pad"));
}

#[test]
fn a_series_kind_and_its_numbers_become_one_edit_with_the_text_unparsed() {
    let (h, doc, m) = window_with_chain();
    let add = E::ModAddStep { modifier: m };
    assert_eq!(
        edit_of(
            &h,
            doc,
            add,
            &[(E::ModNewKind, "7"), (E::ModNewLinearCount, "4"), (E::ModNewStepX, "12,5 mm"), (E::ModNewStepY, "-3")]
        ),
        StructureEdit::AddModifierStep {
            modifier: m,
            step: NewModifierStep::Linear { count: "4".into(), step_x: "12,5 mm".into(), step_y: "-3".into() }
        }
    );
    assert_eq!(
        edit_of(&h, doc, add, &[(E::ModNewKind, "8"), (E::ModNewPathCount, "6"), (E::ModNewPathOrient, "0")]),
        StructureEdit::AddModifierStep {
            modifier: m,
            step: NewModifierStep::Path { count: "6".into(), orient: false }
        }
    );
    // Without a reported check the copies turn with the path, as the check is shown.
    assert_eq!(
        edit_of(&h, doc, add, &[(E::ModNewKind, "8"), (E::ModNewPathCount, "4")]),
        StructureEdit::AddModifierStep { modifier: m, step: NewModifierStep::Path { count: "4".into(), orient: true } }
    );
    // The same kinds as a new chain on the selection.
    assert_eq!(
        edit_of(
            &h,
            doc,
            E::ModNewChain,
            &[(E::ModNewKind, "7"), (E::ModNewLinearCount, "2"), (E::ModNewStepX, "20"), (E::ModNewStepY, "0")]
        ),
        StructureEdit::CreateModifierChain {
            step: NewModifierStep::Linear { count: "2".into(), step_x: "20".into(), step_y: "0".into() }
        }
    );
}

#[test]
fn the_series_work_end_to_end_from_the_panel() {
    let (mut h, doc, m) = window_with_chain();
    let values = vec![
        (C::Editor(E::ModNewKind), "7".to_owned()),
        (C::Editor(E::ModNewLinearCount), "3".to_owned()),
        (C::Editor(E::ModNewStepX), "50".to_owned()),
        (C::Editor(E::ModNewStepY), "0".to_owned()),
    ];
    let go = g5_intent(&h.view(), doc, G5Panel::Structure, C::Editor(E::ModAddStep { modifier: m }), &values).unwrap();
    h.send(go);
    let row = &h.document(doc).unwrap().structure.modifiers[0];
    assert_eq!(row.steps.len(), 2);
    assert_eq!(row.steps[1].value.as_ref().map(|v| v.1.as_str()), Some("3"));
}
