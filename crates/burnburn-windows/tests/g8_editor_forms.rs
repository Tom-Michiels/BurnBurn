//! G8-editor forms, headless on the Windows side: adding a step to a modifier chain (E06) and the
//! stack form of the selection. The lines come from the same model as the Mac panel
//! (`burnburn-macos/tests/g8_editor_panels.rs`); here only the form mapping and the intents.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{
    BooleanKind, CommandKey, HeadlessEditorHarness, NewModifierStep, SelectMode, StructureEdit, UiIntent,
};
use burnburn_types::DocumentId;
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{EditorControl as E, G5Control as C, G5Panel as P};

fn window_with_chain() -> (HeadlessEditorHarness, DocumentId, u64) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (100.0, 50.0), (130.0, 70.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    h.send(UiIntent::Structure { document: doc, edit: StructureEdit::CreateOffsetModifier { distance: "2".into() } });
    let id = h.document(doc).unwrap().structure.modifiers[0].id;
    (h, doc, id)
}

fn form_of(h: &HeadlessEditorHarness, doc: DocumentId, panel: P, locale: Locale) -> Option<Form> {
    let vm = h.view();
    let state = PanelState { g5_shown: vec![P::Structure], ..PanelState::default() };
    g5::forms(&FormContext { vm: &vm, document: doc, locale }, &state, &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(panel))
}

fn react(
    h: &HeadlessEditorHarness,
    doc: DocumentId,
    panel: P,
    c: E,
    values: Vec<(ControlId, ControlValue)>,
) -> FormOutcome {
    let vm = h.view();
    g5::react(panel, &ControlId::G5(C::Editor(c)), &values, &FormContext { vm: &vm, document: doc, locale: Locale::En })
}

#[test]
fn the_structure_form_has_the_kind_choice_the_numbers_and_the_add_buttons() {
    let (h, doc, m) = window_with_chain();
    let f = form_of(&h, doc, P::Structure, Locale::En).expect("the structure form");
    assert!(matches!(
        f.find(&ControlId::G5(C::Editor(E::ModNewKind))),
        Some(FormLine::Choice { caption, items, selected: 0, .. }) if caption == "Kind of step" && items.len() == 7
    ));
    for c in [
        E::ModNewDistance,
        E::ModNewColumns,
        E::ModNewCount,
        E::ModNewSweep,
        E::ModAddStep { modifier: m },
        E::ModNewChain,
    ] {
        assert!(f.find(&ControlId::G5(C::Editor(c))).is_some(), "{c:?}");
    }
    let nl = form_of(&h, doc, P::Structure, Locale::Nl).unwrap();
    assert!(matches!(
        nl.find(&ControlId::G5(C::Editor(E::ModNewKind))),
        Some(FormLine::Choice { caption, .. }) if caption == "Soort stap"
    ));
}

#[test]
fn a_chosen_kind_and_typed_numbers_are_one_add_step_intent_like_on_the_mac() {
    let (h, doc, m) = window_with_chain();
    let values = vec![
        (ControlId::G5(C::Editor(E::ModNewKind)), ControlValue::Choice(5)),
        (ControlId::G5(C::Editor(E::ModNewColumns)), ControlValue::Text("3".into())),
        (ControlId::G5(C::Editor(E::ModNewRows)), ControlValue::Text("2".into())),
        (ControlId::G5(C::Editor(E::ModNewPitchX)), ControlValue::Text("12 mm".into())),
        (ControlId::G5(C::Editor(E::ModNewPitchY)), ControlValue::Text("8".into())),
    ];
    let outcome = react(&h, doc, P::Structure, E::ModAddStep { modifier: m }, values.clone());
    assert_eq!(
        outcome,
        FormOutcome::Intent(UiIntent::Structure {
            document: doc,
            edit: StructureEdit::AddModifierStep {
                modifier: m,
                step: NewModifierStep::Grid {
                    columns: "3".into(),
                    rows: "2".into(),
                    pitch_x: "12 mm".into(),
                    pitch_y: "8".into()
                }
            }
        })
    );
    let chain = react(
        &h,
        doc,
        P::Structure,
        E::ModNewChain,
        vec![(ControlId::G5(C::Editor(E::ModNewKind)), ControlValue::Choice(2))],
    );
    assert_eq!(
        chain,
        FormOutcome::Intent(UiIntent::Structure {
            document: doc,
            edit: StructureEdit::CreateModifierChain { step: NewModifierStep::Boolean { op: BooleanKind::Difference } }
        })
    );
}

#[test]
fn the_stack_form_opens_by_itself_for_the_selection_of_a_chain_and_sends_the_same_intents() {
    let (mut h, doc, m) = window_with_chain();
    // No menu item asked for it: the Stack form is there because the selection belongs to a chain.
    let f = form_of(&h, doc, P::Stack, Locale::En).expect("the stack form");
    assert_eq!(f.title, "Stack of the selection");
    assert!(f.find(&ControlId::G5(C::Editor(E::ModStepOn { modifier: m, step: 0 }))).is_some());
    let outcome = react(&h, doc, P::Stack, E::ModBake { modifier: m }, Vec::new());
    assert_eq!(
        outcome,
        FormOutcome::Intent(UiIntent::Structure { document: doc, edit: StructureEdit::BakeModifier { modifier: m } })
    );
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    assert!(form_of(&h, doc, P::Stack, Locale::En).is_none(), "nothing selected, no stack form");
}
