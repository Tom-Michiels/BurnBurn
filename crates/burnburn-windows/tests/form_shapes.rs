//! The geometry value dialog (join, round corners, optimize), headless (W4-L8a): one value field
//! with the unit typed by the user, OK sends it unparsed, Cancel closes it, a refusal is said in
//! words and keeps the dialog open. Same behaviour as the macOS sheet (`shape_panels.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::forms::{
    ControlId, ControlValue, Form, FormContext, FormKind, FormLine, FormOutcome, Placement, shapes,
};
use burnburn_windows::l10n::Locale;

fn rectangle() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    (h, doc)
}

fn form(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Option<Form> {
    let vm = h.view();
    shapes::dialog_form(&FormContext { vm: &vm, document: doc, locale })
}

fn press(h: &HeadlessEditorHarness, doc: DocumentId, id: ControlId, typed: &str) -> FormOutcome {
    let vm = h.view();
    let values = [(ControlId::GeometryValue, ControlValue::Text(typed.into()))];
    shapes::dialog_react(
        &id,
        &ControlValue::Press,
        &values,
        &FormContext { vm: &vm, document: doc, locale: Locale::En },
    )
}

#[test]
fn there_is_no_dialog_until_a_geometry_command_opens_one() {
    let (h, doc) = rectangle();
    assert!(form(&h, doc, Locale::En).is_none());
}

#[test]
fn round_corners_opens_a_floating_value_dialog_in_the_window_language() {
    let (mut h, doc) = rectangle();
    h.invoke(doc, CommandKey::ShapesRoundCorners);
    let en = form(&h, doc, Locale::En).expect("a dialog");
    assert_eq!((en.kind, en.placement), (FormKind::GeometryDialog, Placement::Floating));
    let nl = form(&h, doc, Locale::Nl).unwrap();
    assert_ne!(en.title, nl.title, "the title follows the language");
    assert!(
        matches!(&en.lines[0], FormLine::Field { id: ControlId::GeometryValue, text, invalid: false, .. } if !text.is_empty())
    );
    assert!(en.ids().contains(&ControlId::GeometryOk) && en.ids().contains(&ControlId::GeometryCancel));
}

#[test]
fn ok_sends_the_typed_value_with_its_unit_unparsed_and_cancel_closes_the_dialog() {
    let (mut h, doc) = rectangle();
    h.invoke(doc, CommandKey::ShapesRoundCorners);
    assert_eq!(
        press(&h, doc, ControlId::GeometryOk, "2,5 mm"),
        FormOutcome::Intent(UiIntent::ApplyGeometryDialog { document: doc, text: "2,5 mm".into() })
    );
    assert_eq!(
        press(&h, doc, ControlId::GeometryCancel, "ignored"),
        FormOutcome::Intent(UiIntent::CancelGeometryDialog { document: doc })
    );
    if let FormOutcome::Intent(i) = press(&h, doc, ControlId::GeometryCancel, "") {
        h.send(i);
    }
    assert!(form(&h, doc, Locale::En).is_none(), "cancel closed it");
}

#[test]
fn a_refused_value_keeps_the_dialog_open_with_the_reason() {
    let (mut h, doc) = rectangle();
    h.invoke(doc, CommandKey::ShapesRoundCorners);
    if let FormOutcome::Intent(i) = press(&h, doc, ControlId::GeometryOk, "veel") {
        h.send(i);
    }
    let form = form(&h, doc, Locale::En).expect("still open");
    assert!(matches!(&form.lines[0], FormLine::Field { invalid: true, .. }));
    assert!(form.lines.iter().any(|l| matches!(l, FormLine::Problem(p) if !p.is_empty())));
}

#[test]
fn the_buttons_of_a_dialog_that_is_gone_send_nothing() {
    let (h, doc) = rectangle();
    assert_eq!(press(&h, doc, ControlId::GeometryOk, "1"), FormOutcome::Nothing);
    assert_eq!(press(&h, doc, ControlId::GeometryCancel, "1"), FormOutcome::Nothing);
}
