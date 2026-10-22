//! G8-cam-werk, W07, headless: the nest panel offers the orders with what is left of each and a
//! button that takes the quantity of an order as the copies of its parts.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{SchikkenControl as C, SchikkenLine, schikken_panel};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, OrderDraft, UiIntent};
use burnburn_types::DocumentId;

fn window() -> (HeadlessEditorHarness, DocumentId, String) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.drag(doc, (40.0, 0.0), (60.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    h.invoke(doc, CommandKey::QuickNest);
    let name = h.document(doc).unwrap().objects[0].label.clone();
    (h, doc, name)
}

fn text(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> String {
    schikken_panel(&h.view(), doc, locale)
        .unwrap()
        .lines
        .into_iter()
        .filter_map(|l| match l {
            SchikkenLine::Text(t) | SchikkenLine::Heading(t) | SchikkenLine::Problem(t) => Some(t),
            SchikkenLine::Button { title, enabled, .. } => {
                Some(format!("[{title}]{}", if enabled { "" } else { " (off)" }))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, control: C) {
    let event = NativeEvent::SchikkenControl { document: doc, control, values: vec![] };
    if let Reaction::Intent(intent) = translate(event, &h.view()) {
        h.send(intent);
    }
}

#[test]
fn an_order_is_offered_with_what_is_left_and_taking_it_sets_the_copies_of_its_part() {
    let (mut h, doc, name) = window();
    assert!(!text(&h, doc, Locale::En).contains("Take the quantity"), "no orders, no section");
    h.send(UiIntent::AddOrder {
        order: OrderDraft { name: "Klant 8".into(), part: name.clone(), quantity: 4, remnant: None },
    });
    h.send(UiIntent::AddOrder {
        order: OrderDraft { name: "Klant 9".into(), part: "Onbekend".into(), quantity: 2, remnant: None },
    });
    let en = text(&h, doc, Locale::En);
    assert!(en.contains("Take the quantity of an order"), "{en}");
    assert!(en.contains("Klant 8: 4 still to make, 1 part of the nest has this name"), "{en}");
    assert!(en.contains("[Use the quantity of Klant 8]\n"), "{en}");
    assert!(en.contains("[Use the quantity of Klant 9] (off)"), "{en}");
    let nl = text(&h, doc, Locale::Nl);
    assert!(nl.contains("Klant 8: nog 4 te maken, 1 onderdeel van de nest heeft deze naam"), "{nl}");

    press(&mut h, doc, C::NestOrder(0));
    let nest = h.document(doc).unwrap().schikken.nest.unwrap();
    assert_eq!(nest.input.part_copies.len(), 1);
    assert_eq!(nest.input.part_copies[0].1, "4");
    assert_eq!(nest.part_rows[0].copies, "4");

    // The button of an order that matches nothing is off; the model says why when it is pressed anyway.
    press(&mut h, doc, C::NestOrder(1));
    let nl = text(&h, doc, Locale::Nl);
    assert!(nl.contains("Geen enkel onderdeel van de nest heeft de naam van het onderdeel van deze order."), "{nl}");
}
