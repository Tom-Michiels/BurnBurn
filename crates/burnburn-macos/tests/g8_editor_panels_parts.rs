//! G8-editor panels, headless on the Mac: turning an instance, distributing over plates and the
//! formula hint in the structure panel (S07, P06). Field text goes to presentation unparsed (DR-05).
//! The Windows host shows the same lines (`burnburn-windows/tests/g8_editor_forms.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::l10n::{Locale, message_or_key};
use burnburn_macos::panels::{EditorControl as E, G5Control as C, G5Line, G5Panel, g5_intent, g5_panel};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, MessageKey, StructureEdit, StructureProblem, UiIntent};
use burnburn_types::DocumentId;

/// A window with a 30 x 20 rectangle made a part "Beugel" with one instance, and one plate.
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

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<G5Line> {
    g5_panel(&h.view(), doc, G5Panel::Structure, locale, &[]).expect("the structure panel").lines
}

fn find(all: &[G5Line], control: E) -> &G5Line {
    all.iter().find(|l| l.control() == Some(C::Editor(control))).unwrap_or_else(|| panic!("{control:?} missing"))
}

fn intent(h: &HeadlessEditorHarness, doc: DocumentId, control: E, values: &[(E, &str)]) -> Option<UiIntent> {
    let values: Vec<(C, String)> = values.iter().map(|(c, v)| (C::Editor(*c), (*v).to_owned())).collect();
    g5_intent(&h.view(), doc, G5Panel::Structure, C::Editor(control), &values)
}

fn edit_of(intent: Option<UiIntent>) -> StructureEdit {
    match intent {
        Some(UiIntent::Structure { edit, .. }) => edit,
        other => panic!("a structure edit, got {other:?}"),
    }
}

#[test]
fn each_instance_shows_its_angle_and_a_turn_button_and_the_plates_have_a_distribute_block() {
    let (h, doc, instance) = window();
    let all = lines(&h, doc, Locale::En);
    assert!(matches!(
        find(&all, E::InstanceRotation { instance }),
        G5Line::Field { caption, text, .. } if caption.contains("angle") && text == "0"
    ));
    assert!(matches!(find(&all, E::InstanceTurn { instance }), G5Line::Button { enabled: true, .. }));
    assert!(matches!(find(&all, E::PlatesGap), G5Line::Field { text, .. } if text == "5"));
    assert!(matches!(find(&all, E::PlatesDistribute), G5Line::Button { enabled: true, .. }));
    let nl = lines(&h, doc, Locale::Nl);
    assert!(matches!(find(&nl, E::PlatesDistribute), G5Line::Button { title, .. } if title.contains("Verdeel")));
}

#[test]
fn a_typed_angle_and_the_turn_button_send_one_rotate_edit_each() {
    let (h, doc, instance) = window();
    let rotation = E::InstanceRotation { instance };
    assert_eq!(intent(&h, doc, rotation, &[(rotation, "0")]), None, "the shown angle again is no edit");
    assert_eq!(
        edit_of(intent(&h, doc, rotation, &[(rotation, "45")])),
        StructureEdit::RotateInstance { instance, degrees: "45".into(), relative: false }
    );
    assert_eq!(
        edit_of(intent(&h, doc, E::InstanceTurn { instance }, &[])),
        StructureEdit::RotateInstance { instance, degrees: "90".into(), relative: true }
    );
    assert_eq!(intent(&h, doc, E::InstanceTurn { instance: 999 }, &[]), None, "an instance that is gone");
}

#[test]
fn the_distribute_button_sends_the_gap_unparsed_and_the_result_shows_in_the_window() {
    let (mut h, doc, instance) = window();
    let go = intent(&h, doc, E::PlatesDistribute, &[(E::PlatesGap, "2,5 mm")]).unwrap();
    assert_eq!(
        go,
        UiIntent::Structure { document: doc, edit: StructureEdit::DistributeOverPlates { gap: "2,5 mm".into() } }
    );
    h.send(go);
    let plate = h.document(doc).unwrap().structure.plates[0].id;
    let row = &h.document(doc).unwrap().structure.parts[0].instances[0];
    assert_eq!((row.id, row.plate_id), (instance, Some(plate)));
    // Turning works end to end: the angle shows after the edit, and the field follows it.
    let turn = intent(&h, doc, E::InstanceTurn { instance }, &[]).unwrap();
    h.send(turn);
    assert_eq!(h.document(doc).unwrap().structure.parts[0].instances[0].rotation, "90");
    let all = lines(&h, doc, Locale::En);
    assert!(matches!(find(&all, E::InstanceRotation { instance }), G5Line::Field { text, .. } if text == "90"));
}

#[test]
fn the_new_refusals_are_said_in_both_languages() {
    for p in [
        StructureProblem::BadFormula,
        StructureProblem::NoPlates,
        StructureProblem::NothingToDistribute,
        StructureProblem::NoRoomOnPlates,
    ] {
        let (nl, en) = (
            message_or_key(MessageKey::Structure(p), Locale::Nl),
            message_or_key(MessageKey::Structure(p), Locale::En),
        );
        assert_ne!(nl, en, "{p:?}");
        assert!(!nl.contains("Structure"), "{p:?}: {nl}");
        // The older table ends in a catch-all: a problem without words of its own would show that.
        let fallback = message_or_key(MessageKey::Structure(StructureProblem::NoCandidate), Locale::Nl);
        assert_ne!(nl, fallback, "{p:?} has its own words");
    }
}

#[test]
fn the_shape_hint_tells_about_formulas() {
    let (h, doc, _) = window();
    // A part with shapes shows the hint; this one was made from a drawn rectangle (fixed shape).
    let all = lines(&h, doc, Locale::En);
    let hint = all
        .iter()
        .find_map(|l| match l {
            G5Line::Text(t) if t.contains("formula") => Some(t.clone()),
            _ => None,
        })
        .expect("the hint mentions formulas");
    assert!(hint.contains("parameter"), "{hint}");
    let nl = lines(&h, doc, Locale::Nl);
    assert!(nl.iter().any(|l| matches!(l, G5Line::Text(t) if t.contains("formule"))));
}
