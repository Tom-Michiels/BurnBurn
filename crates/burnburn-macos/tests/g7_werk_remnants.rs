//! The remnant library as a panel, headless (G7-werk, W07): the menu item, the kept remnants with
//! their size and usable area, the form that keeps the selection, the problems in words, and the
//! remnant of each order. Raw text goes to presentation unparsed (DR-05).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_test_support::host::rig as common;

use burnburn_macos::adapter::{
    MenuItem, NativeEvent, Reaction, menu_enabled, menu_item, menu_tag, menu_title, translate,
};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{G5Control as C, G5Line, G5Panel, g5_intent, g5_opens_by_itself, g5_panel};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, OrderDraft, RemnantIntent, SelectMode, UiIntent};
use burnburn_types::DocumentId;
use common::*;

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale, draft: &[(C, String)]) -> Vec<G5Line> {
    g5_panel(&h.view(), doc, G5Panel::Remnants, locale, draft).expect("the remnant panel").lines
}

fn texts(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<String> {
    lines(h, doc, locale, &[])
        .into_iter()
        .filter_map(|l| match l {
            G5Line::Text(t) | G5Line::Problem(t) | G5Line::Unavailable(t) | G5Line::Heading(t) => Some(t),
            // LF17: an empty library is an empty state (symbol and sentence).
            G5Line::EmptyState { text, .. } => Some(text),
            G5Line::Button { title, .. } => Some(format!("[{title}]")),
            G5Line::Field { caption, .. } => Some(format!("<{caption}>")),
            _ => None,
        })
        .collect()
}

/// A 200 x 100 sheet with a 20 x 20 zone, selected.
fn select_offcut(r: &mut Rig) {
    r.h.invoke(r.doc, CommandKey::ToolRectangle);
    r.h.drag(r.doc, (10.0, 150.0), (210.0, 250.0));
    let sheet = r.h.document(r.doc).unwrap().objects.last().unwrap().id;
    r.h.drag(r.doc, (30.0, 170.0), (50.0, 190.0));
    let zone = r.h.document(r.doc).unwrap().objects.last().unwrap().id;
    r.h.invoke(r.doc, CommandKey::ToolSelect);
    r.h.send(UiIntent::Select { document: r.doc, ids: vec![sheet, zone], mode: SelectMode::Replace });
}

fn keep(r: &mut Rig, name: &str) {
    r.h.send(UiIntent::Remnant(RemnantIntent::SaveFromSelection {
        document: r.doc,
        name: name.into(),
        material: "multiplex".into(),
        thickness: "3".into(),
        note: String::new(),
        date: "2026-10-22".into(),
    }));
    r.h.run_tasks();
}

#[test]
fn the_menu_item_has_its_tag_its_titles_and_toggles_the_panel() {
    let r = rig();
    let vm = r.h.view();
    assert_eq!(menu_tag(MenuItem::RemnantsPanel), 1070);
    assert_eq!(menu_item(1070), Some(MenuItem::RemnantsPanel));
    assert!(menu_enabled(MenuItem::RemnantsPanel, &vm));
    assert_eq!(menu_title(MenuItem::RemnantsPanel, &vm, Locale::Nl), "Restplaten");
    assert_eq!(menu_title(MenuItem::RemnantsPanel, &vm, Locale::En), "Remnants");
    assert_eq!(translate(NativeEvent::Menu(MenuItem::RemnantsPanel), &vm), Reaction::ToggleG5Panel(G5Panel::Remnants));
    assert!(!g5_opens_by_itself(G5Panel::Remnants), "it waits for its menu item");
}

#[test]
fn an_empty_library_says_so_and_offers_the_form_to_keep_the_selection() {
    let r = rig();
    let nl = texts(&r.h, r.doc, Locale::Nl).join("\n");
    assert!(
        nl.contains("Nog geen restplaten bewaard. Selecteer de omtrek van een restplaat om hem te bewaren."),
        "{nl}"
    );
    assert!(
        nl.contains("Selectie bewaren als restplaat") && nl.contains("De grootste gesloten lijn is de plaat"),
        "{nl}"
    );
    for caption in ["<Naam>", "<Materiaal>", "<Dikte (mm)>", "<Notitie>", "[Bewaar selectie]"] {
        assert!(nl.contains(caption), "{caption} in {nl}");
    }
    let en = texts(&r.h, r.doc, Locale::En).join("\n");
    assert!(
        en.contains("No remnants kept yet. Select the outline of a remnant to keep it.")
            && en.contains("[Keep selection]"),
        "{en}"
    );
}

#[test]
fn the_form_sends_the_typed_text_unparsed_for_the_window_in_front() {
    let r = rig();
    let draft = vec![
        (C::RemnantName, "Rest 1".to_owned()),
        (C::RemnantMaterial, "multiplex".to_owned()),
        (C::RemnantThickness, "3,5".to_owned()),
        (C::RemnantNote, "rand verbrand".to_owned()),
        (C::RemnantDate, "2026-10-22".to_owned()),
    ];
    let intent = g5_intent(&r.h.view(), r.doc, G5Panel::Remnants, C::RemnantSave, &draft);
    assert_eq!(
        intent,
        Some(UiIntent::Remnant(RemnantIntent::SaveFromSelection {
            document: r.doc,
            name: "Rest 1".into(),
            material: "multiplex".into(),
            thickness: "3,5".into(),
            note: "rand verbrand".into(),
            date: "2026-10-22".into(),
        }))
    );
}

#[test]
fn a_kept_remnant_is_listed_with_its_size_and_zones_and_its_buttons_draw_and_remove_it() {
    let mut r = rig();
    select_offcut(&mut r);
    keep(&mut r, "Restplaat 1");
    let id = r.h.document(r.doc).unwrap().remnants.rows[0].id;
    let nl = texts(&r.h, r.doc, Locale::Nl).join("\n");
    assert!(
        nl.contains("Restplaat 1 — multiplex, 3 mm dik — 200 × 100 mm, 196 cm² bruikbaar, 1 onbruikbare zone(s), bewaard op 2026-10-22"),
        "{nl}"
    );
    assert!(nl.contains("[Teken Restplaat 1 in het venster]") && nl.contains("[Verwijder Restplaat 1]"), "{nl}");
    let en = texts(&r.h, r.doc, Locale::En).join("\n");
    assert!(en.contains("200 × 100 mm, 196 cm² usable, 1 unusable zone(s), kept on 2026-10-22"), "{en}");

    let vm = r.h.view();
    assert_eq!(
        g5_intent(&vm, r.doc, G5Panel::Remnants, C::RemnantPlace { id }, &[]),
        Some(UiIntent::Remnant(RemnantIntent::Place { document: r.doc, id }))
    );
    assert_eq!(
        g5_intent(&vm, r.doc, G5Panel::Remnants, C::RemnantRemove { id }, &[]),
        Some(UiIntent::Remnant(RemnantIntent::Remove { id }))
    );
}

#[test]
fn a_refused_save_is_said_in_words() {
    let mut r = rig();
    r.h.send(UiIntent::Select { document: r.doc, ids: Vec::new(), mode: SelectMode::Replace });
    keep(&mut r, "Rest");
    let nl = texts(&r.h, r.doc, Locale::Nl).join("\n");
    assert!(nl.contains("Selecteer eerst de restplaat in het venster."), "{nl}");
    select_offcut(&mut r);
    keep(&mut r, "Rest");
    keep(&mut r, "rest");
    assert!(texts(&r.h, r.doc, Locale::En).join("\n").contains("A remnant with this name exists already."));
}

#[test]
fn every_order_chooses_its_remnant_and_a_missing_one_is_said() {
    let mut r = rig();
    let part = r.h.document(r.doc).unwrap().title;
    r.h.send(UiIntent::AddOrder { order: OrderDraft { name: "Klant 7".into(), part, quantity: 2, remnant: None } });
    select_offcut(&mut r);
    keep(&mut r, "Restplaat 1");
    let order = r.h.operator_view().orders[0].clone();

    let choice = |h: &HeadlessEditorHarness| {
        lines(h, r.doc, Locale::Nl, &[]).into_iter().find_map(|l| match l {
            G5Line::Choice { control: C::RemnantOrder { index: 0 }, caption, options, selected } => {
                Some((caption, options, selected))
            }
            _ => None,
        })
    };
    let (caption, options, selected) = choice(&r.h).expect("a choice for the order");
    assert_eq!(caption, "Order Klant 7");
    assert_eq!(options, vec!["(geen restplaat)".to_owned(), "Restplaat 1".to_owned()]);
    assert_eq!(selected, 0);

    let vm = r.h.view();
    let pick = vec![(C::RemnantOrder { index: 0 }, "1".to_owned())];
    assert_eq!(
        g5_intent(&vm, r.doc, G5Panel::Remnants, C::RemnantOrder { index: 0 }, &pick),
        Some(UiIntent::SetOrderRemnant { order: order.id, remnant: Some("Restplaat 1".into()) })
    );
    let none = vec![(C::RemnantOrder { index: 0 }, "0".to_owned())];
    assert_eq!(
        g5_intent(&vm, r.doc, G5Panel::Remnants, C::RemnantOrder { index: 0 }, &none),
        Some(UiIntent::SetOrderRemnant { order: order.id, remnant: None })
    );

    r.h.send(UiIntent::SetOrderRemnant { order: order.id, remnant: Some("Restplaat 1".into()) });
    assert_eq!(choice(&r.h).unwrap().2, 1, "the order's remnant is the selected one");
    let note = texts(&r.h, r.doc, Locale::Nl).join("\n");
    assert!(note.contains("Een koppeling verandert geen aantallen"), "{note}");

    let id = r.h.document(r.doc).unwrap().remnants.rows[0].id;
    r.h.send(UiIntent::Remnant(RemnantIntent::Remove { id }));
    r.h.run_tasks();
    let gone = texts(&r.h, r.doc, Locale::Nl).join("\n");
    assert!(gone.contains("De restplaat Restplaat 1 van order Klant 7 staat niet meer in de bibliotheek."), "{gone}");
}
