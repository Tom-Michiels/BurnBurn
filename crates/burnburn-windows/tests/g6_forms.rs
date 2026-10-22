//! The golf-6 editor panels as Windows forms, headless (G6-editor): the generator panel (E10).
//! Same lines, words and intents as the macOS panel (`tests/g6_panels.rs` there): a control reports
//! its value, presentation decides what changes. The Win32 realisation (`win32/forms.rs`) is only
//! type-checked from macOS, never run.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use burnburn_presentation::{
    CommandKey, GeneratorEdit, GeneratorField, GeneratorKind, HeadlessEditorHarness, UiIntent,
};
use burnburn_types::DocumentId;
use burnburn_windows::forms::g5;
use burnburn_windows::forms::layout::layout;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, PanelState, Placement,
};
use burnburn_windows::l10n::Locale;
use burnburn_windows::panels::{G5Control as C, G5Panel as P};

type Values = Vec<(ControlId, ControlValue)>;

fn open() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::GeneratorNew);
    (h, doc)
}

fn forms(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<Form> {
    let vm = h.view();
    g5::forms(&FormContext { vm: &vm, document: doc, locale }, &PanelState::default(), &Values::new())
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId) -> Form {
    forms(h, doc, Locale::En).into_iter().find(|f| f.kind == FormKind::G5(P::Generator)).expect("the generator form")
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C, overrides: &[(C, ControlValue)]) -> Option<UiIntent> {
    let mut values = form(h, doc).values();
    for (c, v) in overrides {
        match values.iter_mut().find(|(id, _)| *id == ControlId::G5(*c)) {
            Some(slot) => slot.1 = v.clone(),
            None => values.push((ControlId::G5(*c), v.clone())),
        }
    }
    let vm = h.view();
    let outcome = g5::react(
        P::Generator,
        &ControlId::G5(c),
        &values,
        &FormContext { vm: &vm, document: doc, locale: Locale::En },
    );
    match outcome {
        FormOutcome::Intent(i) => {
            h.send(i.clone());
            Some(i)
        }
        FormOutcome::Nothing => None,
        other => panic!("{other:?}"),
    }
}

fn text(s: &str) -> ControlValue {
    ControlValue::Text(s.to_owned())
}

#[test]
fn the_generator_form_opens_by_itself_with_the_dialog_and_goes_with_it() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert!(forms(&h, doc, Locale::En).is_empty());
    h.invoke(doc, CommandKey::GeneratorNew);
    let f = form(&h, doc);
    assert_eq!(f.placement, Placement::Floating);
    assert_eq!(f.title, "Box or Hinge");
    assert_eq!(forms(&h, doc, Locale::Nl).iter().find(|f| f.kind == FormKind::G5(P::Generator)).unwrap().title, "Doos of scharnier");
    h.send(UiIntent::Generator { document: doc, edit: GeneratorEdit::Cancel });
    assert!(forms(&h, doc, Locale::En).is_empty());
}

#[test]
fn the_form_lists_the_fields_of_the_kind_and_each_control_becomes_one_intent() {
    let (mut h, doc) = open();
    let ids = form(&h, doc).ids();
    for field in [GeneratorField::Width, GeneratorField::Depth, GeneratorField::Height, GeneratorField::Thickness] {
        assert!(ids.contains(&ControlId::G5(C::GeneratorField(field))), "{field:?}");
    }
    assert!(ids.contains(&ControlId::G5(C::GeneratorLid)));
    assert_eq!(press(&mut h, doc, C::GeneratorField(GeneratorField::Depth), &[(C::GeneratorField(GeneratorField::Depth), text("60"))]), None);
    assert_eq!(
        press(&mut h, doc, C::GeneratorField(GeneratorField::Depth), &[(C::GeneratorField(GeneratorField::Depth), text("45"))]),
        Some(UiIntent::Generator { document: doc, edit: GeneratorEdit::Field(GeneratorField::Depth, "45".into()) })
    );
    assert_eq!(
        press(&mut h, doc, C::GeneratorKind, &[(C::GeneratorKind, ControlValue::Choice(1))]),
        Some(UiIntent::Generator { document: doc, edit: GeneratorEdit::Kind(GeneratorKind::LivingHinge) })
    );
    assert!(!form(&h, doc).ids().contains(&ControlId::G5(C::GeneratorLid)), "no lid on a hinge");
    assert!(form(&h, doc).ids().contains(&ControlId::G5(C::GeneratorField(GeneratorField::CutLength))));
}

#[test]
fn insert_makes_the_object_and_closes_the_form_and_the_form_fits_every_width() {
    let (mut h, doc) = open();
    for width in [260, 380, 520] {
        let l = layout(&form(&h, doc), width, 1.0);
        for (i, a) in l.items.iter().enumerate() {
            assert!(a.rect.x >= 0 && a.rect.right() <= width, "{:?}", a.rect);
            for b in l.items.iter().skip(i + 1) {
                assert!(!a.rect.overlaps(&b.rect), "{:?} / {:?}", a.rect, b.rect);
            }
        }
    }
    assert_eq!(
        press(&mut h, doc, C::GeneratorInsert, &[]),
        Some(UiIntent::Generator { document: doc, edit: GeneratorEdit::Insert })
    );
    assert!(forms(&h, doc, Locale::En).is_empty());
    assert_eq!(h.document(doc).unwrap().objects.len(), 6);
}
