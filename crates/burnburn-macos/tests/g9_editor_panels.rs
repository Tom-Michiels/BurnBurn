//! G9-editor panels, headless on the Mac: the "Distribute and nest" button of the structure panel
//! (P06). Field text goes to presentation unparsed (DR-05). The Windows host shows the same lines
//! (`burnburn-windows/tests/g9_editor_forms.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{EditorControl as E, G5Control as C, G5Line, G5Panel, g5_intent, g5_panel};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, StructureEdit, UiIntent};
use burnburn_types::DocumentId;

/// A window with a 30 x 20 rectangle made a part "Beugel" with one instance, and one plate.
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

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<G5Line> {
    g5_panel(&h.view(), doc, G5Panel::Structure, locale, &[]).expect("the structure panel").lines
}

fn find(all: &[G5Line], control: E) -> &G5Line {
    all.iter().find(|l| l.control() == Some(C::Editor(control))).unwrap_or_else(|| panic!("{control:?} missing"))
}

#[test]
fn the_distribute_block_has_a_nest_button_in_both_languages() {
    let (h, doc) = window();
    let en = lines(&h, doc, Locale::En);
    assert!(matches!(find(&en, E::PlatesDistributeNest), G5Line::Button { enabled: true, title, .. } if title.contains("nest")));
    let nl = lines(&h, doc, Locale::Nl);
    assert!(matches!(find(&nl, E::PlatesDistributeNest), G5Line::Button { title, .. } if title.contains("nest")));
}

#[test]
fn the_nest_button_sends_the_gap_unparsed_and_the_instance_ends_up_on_the_plate() {
    let (mut h, doc) = window();
    let values = vec![(C::Editor(E::PlatesGap), "2,5 mm".to_owned())];
    let go = g5_intent(&h.view(), doc, G5Panel::Structure, C::Editor(E::PlatesDistributeNest), &values).unwrap();
    assert_eq!(
        go,
        UiIntent::Structure {
            document: doc,
            edit: StructureEdit::DistributeAndNestOverPlates { gap: "2,5 mm".into() }
        }
    );
    h.send(go);
    h.run_tasks();
    let plate = h.document(doc).unwrap().structure.plates[0].id;
    let row = &h.document(doc).unwrap().structure.parts[0].instances[0];
    assert_eq!(row.plate_id, Some(plate), "the nest answered and the instance is on the plate");
}
