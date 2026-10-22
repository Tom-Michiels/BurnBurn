//! G8-cam-werk, W07, on Windows (headless): the nest form offers the orders and sends the same intent
//! as the Mac when the quantity of one is taken (one panel model).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, OrderDraft, SchikkenIntent, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{ControlId, ControlValue, Form, FormContext, FormLine, FormOutcome, PanelState, schikken};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::SchikkenControl as C;

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

fn nest_form(h: &HeadlessEditorHarness, doc: DocumentId) -> Form {
    let vm = h.view();
    schikken::form(&FormContext { vm: &vm, document: doc, locale: Locale::En }, &PanelState::default())
        .expect("the nest form")
}

#[test]
fn the_form_lists_an_order_and_its_button_sends_the_quantity_to_the_nest() {
    let (mut h, doc, name) = window();
    assert!(nest_form(&h, doc).find(&ControlId::Schikken(C::NestOrder(0))).is_none(), "no orders, no button");
    h.send(UiIntent::AddOrder { order: OrderDraft { name: "Klant 8".into(), part: name, quantity: 4, remnant: None } });
    let form = nest_form(&h, doc);
    assert!(form.lines.iter().any(|l| matches!(l, FormLine::Text(t) if t.contains("Klant 8: 4 still to make"))));
    assert!(form.find(&ControlId::Schikken(C::NestOrder(0))).is_some());

    let vm = h.view();
    let outcome = schikken::react(
        &ControlId::Schikken(C::NestOrder(0)),
        &ControlValue::Press,
        &form.values(),
        &FormContext { vm: &vm, document: doc, locale: Locale::En },
    );
    let FormOutcome::Intent(UiIntent::Schikken(SchikkenIntent::TakeOrderQuantities { document, order })) = outcome
    else {
        panic!("the button takes the quantity")
    };
    assert_eq!(document, doc);
    h.send(UiIntent::Schikken(SchikkenIntent::TakeOrderQuantities { document, order }));
    let nest = h.document(doc).unwrap().schikken.nest.unwrap();
    assert_eq!(nest.part_rows[0].copies, "4");
}
