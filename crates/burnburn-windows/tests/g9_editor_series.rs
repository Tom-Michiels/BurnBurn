//! G9-editor E06 form, headless on the Windows side: the add-a-step block also offers a linear
//! series and a series along a path. The lines come from the same model as the Mac panel
//! (`burnburn-macos/tests/g9_editor_series.rs`); here only the form mapping and the intents.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, NewModifierStep, StructureEdit, UiIntent};
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

fn structure_form(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Form {
    let vm = h.view();
    let state = PanelState { g5_shown: vec![P::Structure], ..PanelState::default() };
    g5::forms(&FormContext { vm: &vm, document: doc, locale }, &state, &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(P::Structure))
        .expect("the structure form")
}

fn react(h: &HeadlessEditorHarness, doc: DocumentId, c: E, values: Vec<(ControlId, ControlValue)>) -> FormOutcome {
    let vm = h.view();
    g5::react(
        P::Structure,
        &ControlId::G5(C::Editor(c)),
        &values,
        &FormContext { vm: &vm, document: doc, locale: Locale::En },
    )
}

#[test]
fn the_structure_form_offers_the_series_kinds_and_their_fields() {
    let (h, doc, _) = window_with_chain();
    let f = structure_form(&h, doc, Locale::En);
    let Some(FormLine::Choice { items, .. }) = f.find(&ControlId::G5(C::Editor(E::ModNewKind))) else {
        panic!("a choice")
    };
    assert_eq!(items.len(), 9);
    for c in [E::ModNewLinearCount, E::ModNewStepX, E::ModNewStepY, E::ModNewPathCount, E::ModNewPathOrient] {
        assert!(f.find(&ControlId::G5(C::Editor(c))).is_some(), "{c:?}");
    }
    assert!(structure_form(&h, doc, Locale::Nl).find(&ControlId::G5(C::Editor(E::ModNewPathOrient))).is_some());
}

#[test]
fn a_series_kind_and_its_numbers_are_one_intent_like_on_the_mac() {
    let (h, doc, m) = window_with_chain();
    let id = |c: E| ControlId::G5(C::Editor(c));
    let linear = react(
        &h,
        doc,
        E::ModAddStep { modifier: m },
        vec![
            (id(E::ModNewKind), ControlValue::Choice(7)),
            (id(E::ModNewLinearCount), ControlValue::Text("4".into())),
            (id(E::ModNewStepX), ControlValue::Text("12,5 mm".into())),
            (id(E::ModNewStepY), ControlValue::Text("-3".into())),
        ],
    );
    assert_eq!(
        linear,
        FormOutcome::Intent(UiIntent::Structure {
            document: doc,
            edit: StructureEdit::AddModifierStep {
                modifier: m,
                step: NewModifierStep::Linear { count: "4".into(), step_x: "12,5 mm".into(), step_y: "-3".into() }
            }
        })
    );
    let path = react(
        &h,
        doc,
        E::ModNewChain,
        vec![
            (id(E::ModNewKind), ControlValue::Choice(8)),
            (id(E::ModNewPathCount), ControlValue::Text("6".into())),
            (id(E::ModNewPathOrient), ControlValue::Toggle(false)),
        ],
    );
    assert_eq!(
        path,
        FormOutcome::Intent(UiIntent::Structure {
            document: doc,
            edit: StructureEdit::CreateModifierChain {
                step: NewModifierStep::Path { count: "6".into(), orient: false }
            }
        })
    );
}
