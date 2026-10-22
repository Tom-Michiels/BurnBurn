//! G8-cam-werk on Windows, headless: the nest form pages a long list of parts and has a grain
//! rule per part, with the words and the intents of the Mac (one panel model).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, NestPartGrain, SchikkenIntent, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormLine, FormOutcome, PanelState, schikken,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::SchikkenControl as C;

fn window(count: usize) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    for i in 0..count {
        let x = i as f64 * 30.0;
        h.drag(doc, (x, 0.0), (x + 20.0, 10.0));
    }
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    h.invoke(doc, CommandKey::QuickNest);
    (h, doc)
}

fn nest_form(h: &HeadlessEditorHarness, doc: DocumentId) -> Form {
    let vm = h.view();
    schikken::form(&FormContext { vm: &vm, document: doc, locale: Locale::En }, &PanelState::default())
        .expect("the nest form")
}

fn react(h: &HeadlessEditorHarness, doc: DocumentId, c: C, value: ControlValue) -> FormOutcome {
    let vm = h.view();
    let mut values = nest_form(h, doc).values();
    match values.iter_mut().find(|(id, _)| *id == ControlId::Schikken(c)) {
        Some(slot) => slot.1 = value.clone(),
        None => values.push((ControlId::Schikken(c), value.clone())),
    }
    schikken::react(
        &ControlId::Schikken(c),
        &value,
        &values,
        &FormContext { vm: &vm, document: doc, locale: Locale::En },
    )
}

fn send(h: &mut HeadlessEditorHarness, outcome: FormOutcome) {
    let FormOutcome::Intent(i) = outcome else { panic!("an intent") };
    h.send(i);
}

fn copy_rows(f: &Form) -> Vec<usize> {
    (0..40)
        .filter(|i| matches!(f.find(&ControlId::Schikken(C::NestPartCopies(*i))), Some(FormLine::Field { .. })))
        .collect()
}

#[test]
fn the_form_lists_eight_parts_a_page_and_the_page_buttons_reach_the_others() {
    let (mut h, doc) = window(12);
    let f = nest_form(&h, doc);
    assert_eq!(copy_rows(&f), (0..8).collect::<Vec<_>>());
    assert!(f.find(&ControlId::Schikken(C::NestPartsNext)).is_some());
    let outcome = react(&h, doc, C::NestPartsNext, ControlValue::Press);
    let FormOutcome::Intent(UiIntent::Schikken(SchikkenIntent::SetNestOptions { input, .. })) = &outcome else {
        panic!("the page button sends the options")
    };
    assert_eq!(input.part_page, 1);
    send(&mut h, outcome);
    assert_eq!(copy_rows(&nest_form(&h, doc)), vec![8, 9, 10, 11]);
}

#[test]
fn a_grain_on_the_sheet_gives_every_part_a_choice_that_sends_its_rule() {
    let (mut h, doc) = window(2);
    assert!(nest_form(&h, doc).find(&ControlId::Schikken(C::NestPartGrain(0))).is_none());
    let outcome = react(&h, doc, C::NestGrain, ControlValue::Text("0".into()));
    send(&mut h, outcome);
    let f = nest_form(&h, doc);
    for i in 0..2 {
        assert!(matches!(
            f.find(&ControlId::Schikken(C::NestPartGrain(i))),
            Some(FormLine::Choice { selected: 0, .. })
        ));
    }
    let outcome = react(&h, doc, C::NestPartGrain(1), ControlValue::Choice(2));
    let FormOutcome::Intent(UiIntent::Schikken(SchikkenIntent::SetNestOptions { input, .. })) = &outcome else {
        panic!("the choice sends the options")
    };
    assert_eq!(input.part_grain.len(), 1);
    assert_eq!(input.part_grain[0].1, NestPartGrain::Free);
}
