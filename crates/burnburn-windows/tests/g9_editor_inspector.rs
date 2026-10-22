//! G9-editor E06 forms, headless on the Windows side: the stack of the selection is part of the
//! inspector form (docked in the right column), not a floating form of its own. The lines come from
//! the same model as the Mac panel (`burnburn-macos/tests/g8_editor_panels.rs`); the real Mac window
//! is covered by `tests/native/macos/g9_editor_inspector.rs`. The Win32 control is not run here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, StructureEdit, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormAux, FormContext, FormKind, FormLine, FormOutcome, PanelState, forms, react,
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

fn all_forms(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<Form> {
    let vm = h.view();
    let state = PanelState::default();
    let aux = FormAux { state: &state, cam: &[] };
    forms(&FormContext { vm: &vm, document: doc, locale: Locale::En }, &aux, &Vec::new())
}

fn inspector(h: &HeadlessEditorHarness, doc: DocumentId) -> Form {
    all_forms(h, doc).into_iter().find(|f| f.kind == FormKind::Inspector).expect("the inspector form")
}

#[test]
fn the_stack_of_the_selection_is_part_of_the_inspector_form_and_not_a_form_of_its_own() {
    let (h, doc, m) = window_with_chain();
    let forms = all_forms(&h, doc);
    assert!(!forms.iter().any(|f| f.kind == FormKind::G5(P::Stack)), "no floating stack form");
    let inspector = inspector(&h, doc);
    assert!(inspector.find(&ControlId::G5(C::Editor(E::ModStepOn { modifier: m, step: 0 }))).is_some());
    assert!(inspector.find(&ControlId::G5(C::Editor(E::ModNewKind))).is_some(), "and the add-a-step block");
    assert!(
        inspector.lines.iter().any(|l| matches!(l, FormLine::Heading(t) if t == "Stack of the selection")),
        "under its own heading"
    );
    // The usual inspector content is still first: the fields come before the stack.
    assert!(matches!(&inspector.lines[0], FormLine::Row(_)));
}

#[test]
fn a_control_of_the_stack_in_the_inspector_sends_the_intent_of_the_stack_panel() {
    let (h, doc, m) = window_with_chain();
    let vm = h.view();
    let state = PanelState::default();
    let aux = FormAux { state: &state, cam: &[] };
    let ctx = FormContext { vm: &vm, document: doc, locale: Locale::En };
    let outcome = react(
        FormKind::Inspector,
        &ControlId::G5(C::Editor(E::ModBake { modifier: m })),
        &ControlValue::Press,
        &Vec::new(),
        &ctx,
        &aux,
    );
    assert_eq!(
        outcome,
        FormOutcome::Intent(UiIntent::Structure { document: doc, edit: StructureEdit::BakeModifier { modifier: m } })
    );
}

#[test]
fn without_a_chain_in_the_selection_the_inspector_has_no_stack() {
    let (mut h, doc, m) = window_with_chain();
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    let inspector = inspector(&h, doc);
    assert!(inspector.find(&ControlId::G5(C::Editor(E::ModStepOn { modifier: m, step: 0 }))).is_none());
    assert!(!inspector.lines.iter().any(|l| matches!(l, FormLine::Heading(t) if t == "Stack of the selection")));
}
