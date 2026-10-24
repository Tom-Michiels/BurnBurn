//! Parts and references in the structure panel, headless (G6-werk, S06, S07): defining a part from
//! the selection, placing instances, changing defaults, variants, and making references to an edge
//! of the selected object. What the panel shows comes from presentation's view model in the window
//! language; a button sends the typed text of its fields as one intent (DR-05).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::l10n::{Locale, message_or_key};
use burnburn_macos::l10n_g6_werk::structure_problem_text;
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, g5_intent, g5_panel};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, MessageKey, StructureEdit, StructureProblem, UiIntent};
use burnburn_types::DocumentId;

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (100.0, 50.0), (130.0, 70.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    (h, doc)
}

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<G5Line> {
    g5_panel(&h.view(), doc, G5Panel::Structure, locale, &[]).expect("the structure panel").lines
}

fn has(h: &HeadlessEditorHarness, doc: DocumentId, c: C) -> bool {
    lines(h, doc, Locale::En).iter().any(|l| l.control() == Some(c))
}

fn texts(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<String> {
    lines(h, doc, locale)
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) | G5Line::Problem(t) | G5Line::Unavailable(t) | G5Line::Heading(t) => Some(t),
            _ => None,
        })
        .collect()
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C, values: &[(C, &str)]) -> Option<UiIntent> {
    let values: Vec<(C, String)> = values.iter().map(|(c, v)| (*c, (*v).to_owned())).collect();
    let intent = g5_intent(&h.view(), doc, G5Panel::Structure, c, &values)?;
    h.send(intent.clone());
    Some(intent)
}

#[test]
fn a_part_is_made_from_the_selection_placed_and_changed_in_the_panel() {
    let (mut h, doc) = window();
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t.starts_with("1 object(s) selected")));
    let sent = press(&mut h, doc, C::StructDefine, &[(C::StructDefineName, "Beugel")]).unwrap();
    assert_eq!(sent, UiIntent::Structure { document: doc, edit: StructureEdit::DefinePart { name: "Beugel".into() } });
    let part = h.document(doc).unwrap().structure.parts[0].clone();
    assert_eq!(part.name, "Beugel");
    assert!(has(&h, doc, C::PartPlace { part: part.id }));
    assert!(has(&h, doc, C::PartParam { part: part.id, parameter: 0 }));
    assert!(texts(&h, doc, Locale::Nl).iter().any(|t| t.starts_with("1 object(en) geselecteerd")));

    let placed = press(
        &mut h,
        doc,
        C::PartPlace { part: part.id },
        &[(C::PartPlaceX { part: part.id }, "10"), (C::PartPlaceY { part: part.id }, "5,5")],
    )
    .unwrap();
    assert_eq!(
        placed,
        UiIntent::Structure {
            document: doc,
            edit: StructureEdit::PlaceInstance { definition: part.id, x: "10".into(), y: "5,5".into() }
        }
    );
    let instance = h.document(doc).unwrap().structure.parts[0].instances[0].id;
    assert!(has(&h, doc, C::InstanceRemove { instance }));
    assert!(
        !matches!(
            lines(&h, doc, Locale::En).into_iter().find(|l| l.control() == Some(C::PartRemove { part: part.id })),
            Some(G5Line::Button { enabled: true, .. })
        ),
        "a part in use cannot be removed"
    );

    // A default changes only when the typed text differs from the shown one.
    let same = g5_intent(
        &h.view(),
        doc,
        G5Panel::Structure,
        C::PartParam { part: part.id, parameter: 1 },
        &[(C::PartParam { part: part.id, parameter: 1 }, part.parameters[1].value.clone())],
    );
    assert_eq!(same, None);
    let width_index = part.parameters.iter().position(|p| p.name == "breedte").unwrap();
    let c = C::PartParam { part: part.id, parameter: width_index };
    let changed = press(&mut h, doc, c, &[(c, "45 mm")]).unwrap();
    assert!(
        matches!(changed, UiIntent::Structure { edit: StructureEdit::SetParameter { ref text, .. }, .. } if text == "45 mm")
    );
    let shown = h.document(doc).unwrap().structure.parts[0].parameters[width_index].value.clone();
    assert_eq!(shown, "45");

    // Variants: a name, then a value per parameter.
    press(&mut h, doc, C::PartAddVariant { part: part.id }, &[(C::PartVariantName { part: part.id }, "groot")])
        .unwrap();
    let v = C::PartVariantValue { part: part.id, variant: 0, parameter: width_index };
    assert!(has(&h, doc, v));
    press(&mut h, doc, v, &[(v, "60")]).unwrap();
    assert_eq!(h.document(doc).unwrap().structure.parts[0].variant_values[0].values[0].value, "60");

    press(&mut h, doc, C::InstanceRemove { instance }, &[]).unwrap();
    press(&mut h, doc, C::PartRemove { part: part.id }, &[]).unwrap();
    assert!(h.document(doc).unwrap().structure.parts.is_empty());
}

#[test]
fn without_a_selection_a_part_is_refused_in_words() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert!(!matches!(
        lines(&h, doc, Locale::En).into_iter().find(|l| l.control() == Some(C::StructDefine)),
        Some(G5Line::Button { enabled: true, .. })
    ));
    h.send(UiIntent::Structure { document: doc, edit: StructureEdit::DefinePart { name: "X".into() } });
    let problem = h.document(doc).unwrap().problem.expect("a problem").message;
    assert_eq!(message_or_key(problem, Locale::En), "Select the objects the part is made of first.");
    assert_eq!(
        message_or_key(problem, Locale::Nl),
        "Selecteer eerst de objecten waar het onderdeel van gemaakt wordt."
    );
}

#[test]
fn the_edges_of_the_selected_object_are_offered_and_a_reference_is_made_and_removed() {
    let (mut h, doc) = window();
    assert!(has(&h, doc, C::RefAdd) && has(&h, doc, C::RefEdge), "one object selected");
    let edge = lines(&h, doc, Locale::En).into_iter().find(|l| l.control() == Some(C::RefEdge)).unwrap();
    let G5Line::Choice { options, .. } = edge else { panic!() };
    assert_eq!(options.len(), 4);
    assert!(
        options.iter().any(|o| o == "Edge 1: 30 mm") && options.iter().any(|o| o == "Edge 2: 20 mm"),
        "{options:?}"
    );
    let sent = press(&mut h, doc, C::RefAdd, &[(C::RefName, "rechts"), (C::RefEdge, "1")]).unwrap();
    let UiIntent::Structure { edit: StructureEdit::AddReference { name, subpath, segment, .. }, .. } = sent else {
        panic!("a reference")
    };
    assert_eq!((name.as_str(), subpath, segment), ("rechts", 0, 1));
    let reference = h.document(doc).unwrap().structure.references[0].id;
    assert!(has(&h, doc, C::RefRemove { reference }));
    assert!(!has(&h, doc, C::RefRebind { reference }), "only an ambiguous reference offers candidates");
    press(&mut h, doc, C::RefRemove { reference }, &[]).unwrap();
    assert!(h.document(doc).unwrap().structure.references.is_empty());

    // With nothing selected there is no edge to choose from, and the panel says why.
    h.send(UiIntent::Select { document: doc, ids: Vec::new(), mode: burnburn_presentation::SelectMode::Replace });
    assert!(!has(&h, doc, C::RefAdd));
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t.starts_with("Select exactly one object")));
}

#[test]
fn every_refusal_of_a_structure_edit_has_words_in_both_languages() {
    use StructureProblem as P;
    for p in [
        P::NeedsSelection,
        P::NeedsName,
        P::UnknownPart,
        P::UnknownInstance,
        P::PartInUse,
        P::UnknownVariant,
        P::VariantExists,
        P::UnknownParameter,
        P::UnknownObject,
        P::NotAnEdge,
        P::UnknownReference,
        P::NotAmbiguous,
        P::NoCandidate,
    ] {
        let (nl, en) = (structure_problem_text(p, Locale::Nl), structure_problem_text(p, Locale::En));
        assert!(!nl.is_empty() && !en.is_empty() && nl != en, "{p:?}");
        assert_eq!(message_or_key(MessageKey::Structure(p), Locale::En), en);
    }
}
