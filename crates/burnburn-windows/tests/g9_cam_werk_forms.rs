//! G9-cam-werk on Windows, headless: the nest form has the grain of a part as a free angle, with the
//! words and the intents of the Mac (one panel model).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, NestPartGrain, SchikkenIntent, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormLine, FormOutcome, PanelState, schikken,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::SchikkenControl as C;

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
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

#[test]
fn the_angle_choice_adds_a_field_to_the_form_and_a_typed_angle_reaches_the_options() {
    let (mut h, doc) = window();
    let outcome = react(&h, doc, C::NestGrain, ControlValue::Text("0".into()));
    let FormOutcome::Intent(i) = outcome else { panic!("an intent") };
    h.send(i);
    assert!(nest_form(&h, doc).find(&ControlId::Schikken(C::NestPartGrainAngle(0))).is_none());
    let form = nest_form(&h, doc);
    let Some(FormLine::Choice { items, .. }) = form.find(&ControlId::Schikken(C::NestPartGrain(0))) else {
        panic!("a grain choice")
    };
    assert_eq!(items.len(), 4);

    let outcome = react(&h, doc, C::NestPartGrain(0), ControlValue::Choice(3));
    let FormOutcome::Intent(i) = outcome else { panic!("an intent") };
    h.send(i);
    assert!(matches!(
        nest_form(&h, doc).find(&ControlId::Schikken(C::NestPartGrainAngle(0))),
        Some(FormLine::Field { .. })
    ));

    let outcome = react(&h, doc, C::NestPartGrainAngle(0), ControlValue::Text("30".into()));
    let FormOutcome::Intent(UiIntent::Schikken(SchikkenIntent::SetNestOptions { input, .. })) = outcome else {
        panic!("the field sends the options")
    };
    assert_eq!(input.part_grain.len(), 1);
    assert_eq!(input.part_grain[0].1, NestPartGrain::Angle(300));
}
