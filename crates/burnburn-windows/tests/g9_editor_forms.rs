//! G9-editor forms, headless on the Windows side: the "Distribute and nest" button of the structure
//! form (P06). The lines come from the same model as the Mac panel
//! (`burnburn-macos/tests/g9_editor_panels.rs`); here only the form mapping and the intents.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, StructureEdit, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{ControlId, ControlValue, Form, FormContext, FormKind, FormOutcome, PanelState};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{EditorControl as E, G5Control as C, G5Panel as P};

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (100.0, 50.0), (130.0, 70.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    h.send(UiIntent::Structure { document: doc, edit: StructureEdit::DefinePart { name: "Beugel".into() } });
    let part = h.document(doc).unwrap().structure.parts[0].id;
    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::PlaceInstance { definition: part, x: "0".into(), y: "0".into() },
    });
    h.send(UiIntent::Structure {
        document: doc,
        edit: StructureEdit::CreatePlate {
            name: "Plaat A".into(),
            width: "600".into(),
            height: "400".into(),
            material: String::new(),
            thickness: String::new(),
        },
    });
    (h, doc)
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Form {
    let vm = h.view();
    let state = PanelState { g5_shown: vec![P::Structure], ..PanelState::default() };
    g5::forms(&FormContext { vm: &vm, document: doc, locale }, &state, &Vec::new())
        .into_iter()
        .find(|f| f.kind == FormKind::G5(P::Structure))
        .expect("the structure form")
}

#[test]
fn the_structure_form_has_the_nest_button_and_sends_the_same_intent_as_the_mac_panel() {
    let (mut h, doc) = window();
    let f = form(&h, doc, Locale::En);
    assert!(f.find(&ControlId::G5(C::Editor(E::PlatesDistributeNest))).is_some());
    assert!(form(&h, doc, Locale::Nl).find(&ControlId::G5(C::Editor(E::PlatesDistributeNest))).is_some());

    let vm = h.view();
    let gap = ControlId::G5(C::Editor(E::PlatesGap));
    let out = g5::react(
        P::Structure,
        &ControlId::G5(C::Editor(E::PlatesDistributeNest)),
        &[(gap, ControlValue::Text("3".into()))],
        &FormContext { vm: &vm, document: doc, locale: Locale::En },
    );
    let FormOutcome::Intent(intent) = out else { panic!("an intent") };
    assert_eq!(
        intent,
        UiIntent::Structure { document: doc, edit: StructureEdit::DistributeAndNestOverPlates { gap: "3".into() } }
    );
    h.send(intent);
    h.run_tasks();
    let plate = h.document(doc).unwrap().structure.plates[0].id;
    assert_eq!(h.document(doc).unwrap().structure.parts[0].instances[0].plate_id, Some(plate));
}
