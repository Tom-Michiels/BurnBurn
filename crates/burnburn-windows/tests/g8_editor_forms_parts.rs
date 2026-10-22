//! G8-editor forms, headless on the Windows side: turning an instance and distributing over plates
//! in the structure form (S07, P06). The lines come from the same model as the Mac panel
//! (`burnburn-macos/tests/g8_editor_panels_parts.rs`); here only the form mapping and the intents.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, StructureEdit, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::g5;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{EditorControl as E, G5Control as C, G5Panel as P};

fn window() -> (HeadlessEditorHarness, DocumentId, u64) {
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
    let instance = h.document(doc).unwrap().structure.parts[0].instances[0].id;
    (h, doc, instance)
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Form {
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
fn the_structure_form_has_the_angle_the_turn_button_and_the_distribute_block() {
    let (h, doc, instance) = window();
    let f = form(&h, doc, Locale::En);
    assert!(matches!(
        f.find(&ControlId::G5(C::Editor(E::InstanceRotation { instance }))),
        Some(FormLine::Field { text, .. }) if text == "0"
    ));
    for c in [E::InstanceTurn { instance }, E::PlatesGap, E::PlatesDistribute] {
        assert!(f.find(&ControlId::G5(C::Editor(c))).is_some(), "{c:?}");
    }
    let nl = form(&h, doc, Locale::Nl);
    assert!(nl.find(&ControlId::G5(C::Editor(E::PlatesDistribute))).is_some());
}

#[test]
fn the_form_sends_the_same_intents_as_the_mac_panel() {
    let (mut h, doc, instance) = window();
    let angle = ControlId::G5(C::Editor(E::InstanceRotation { instance }));
    let out = react(&h, doc, E::InstanceRotation { instance }, vec![(angle, ControlValue::Text("30".into()))]);
    assert_eq!(
        out,
        FormOutcome::Intent(UiIntent::Structure {
            document: doc,
            edit: StructureEdit::RotateInstance { instance, degrees: "30".into(), relative: false }
        })
    );
    let gap = ControlId::G5(C::Editor(E::PlatesGap));
    let out = react(&h, doc, E::PlatesDistribute, vec![(gap, ControlValue::Text("3".into()))]);
    let FormOutcome::Intent(intent) = out else { panic!("an intent") };
    assert_eq!(
        intent,
        UiIntent::Structure { document: doc, edit: StructureEdit::DistributeOverPlates { gap: "3".into() } }
    );
    h.send(intent);
    let plate = h.document(doc).unwrap().structure.plates[0].id;
    assert_eq!(h.document(doc).unwrap().structure.parts[0].instances[0].plate_id, Some(plate));
}
