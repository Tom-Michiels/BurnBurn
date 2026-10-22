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

mod part_name {
    use burnburn_presentation::{CommandKey, HeadlessEditorHarness, UiIntent};
    use burnburn_types::DocumentId;
    use burnburn_windows::forms::g5;
    use burnburn_windows::forms::{
        ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState,
    };
    use burnburn_windows::l10n::Locale;
    use burnburn_windows::panels::{G5Control as C, G5Panel as P};

    fn form(h: &HeadlessEditorHarness, doc: DocumentId) -> Form {
        let vm = h.view();
        let context = FormContext { vm: &vm, document: doc, locale: Locale::En };
        let shown = PanelState { g5_shown: vec![P::Notes], ..PanelState::default() };
        g5::forms(&context, &shown, &Vec::new())
            .into_iter()
            .find(|f| f.kind == FormKind::G5(P::Notes))
            .expect("the notes form")
    }

    #[test]
    fn the_notes_form_has_the_part_name_field_and_its_button_sends_the_name() {
        let mut h = HeadlessEditorHarness::new();
        let doc = h.new_document().unwrap();
        h.invoke(doc, CommandKey::ShowNotes);
        h.send(UiIntent::SetPartName { document: doc, name: "Houder".into() });
        let f = form(&h, doc);
        assert!(matches!(f.find(&ControlId::G5(C::NotesPartName)), Some(FormLine::Field { .. })));

        let vm = h.view();
        let context = FormContext { vm: &vm, document: doc, locale: Locale::En };
        let mut values = f.values();
        values.retain(|(id, _)| *id != ControlId::G5(C::NotesPartName));
        values.push((ControlId::G5(C::NotesPartName), ControlValue::Text("Houder links".into())));
        let outcome = g5::react(P::Notes, &ControlId::G5(C::NotesPartNameSave), &values, &context);
        let FormOutcome::Intent(intent) = outcome else { panic!("{outcome:?}") };
        assert_eq!(intent, UiIntent::SetPartName { document: doc, name: "Houder links".into() });
    }
}
