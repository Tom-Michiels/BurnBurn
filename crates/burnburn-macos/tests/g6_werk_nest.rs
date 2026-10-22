//! The nest panel, headless (G6-werk, C12 and W07): mirroring and the number of sheets as choices,
//! a remnant contour as the sheet, and the report per sheet, per part and in complete sets. What it
//! shows comes from presentation's view model in the window language; a control becomes one intent
//! with the text unparsed (DR-05).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{SchikkenControl as C, SchikkenLine, schikken_panel};
use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, NestFailure, NestMirror, SchikkenIntent, SchikkenMessage, UiIntent,
};
use burnburn_types::DocumentId;

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.drag(doc, (40.0, 0.0), (60.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::SelectAll);
    (h, doc)
}

fn lines(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<SchikkenLine> {
    schikken_panel(&h.view(), doc, locale).expect("a panel").lines
}

fn control(h: &HeadlessEditorHarness, doc: DocumentId, c: C) -> Option<SchikkenLine> {
    lines(h, doc, Locale::En).into_iter().find(|l| l.control() == Some(c))
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, c: C, values: Vec<(C, String)>) -> Option<UiIntent> {
    match translate(NativeEvent::SchikkenControl { document: doc, control: c, values }, &h.view()) {
        Reaction::Intent(i) => {
            h.send(i.clone());
            Some(i)
        }
        _ => None,
    }
}

fn texts(h: &HeadlessEditorHarness, doc: DocumentId, locale: Locale) -> Vec<String> {
    lines(h, doc, locale)
        .into_iter()
        .filter_map(|l| match l {
            SchikkenLine::Text(t) | SchikkenLine::Problem(t) | SchikkenLine::Heading(t) => Some(t),
            _ => None,
        })
        .collect()
}

#[test]
fn a_remnant_contour_is_chosen_from_the_closed_objects_and_becomes_the_sheet() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::NestSelected);
    let Some(SchikkenLine::Choice { options, selected, .. }) = control(&h, doc, C::NestSheet) else {
        panic!("a sheet choice")
    };
    assert_eq!((options.len(), selected), (3, 0), "the bed and the two rectangles: {options:?}");
    assert_eq!(options[0], "Machine bed");

    // Choosing a rectangle that is selected as a part is refused when the nest starts.
    press(&mut h, doc, C::NestSheet, vec![(C::NestSheet, "1".into())]).unwrap();
    let panel = h.document(doc).unwrap().schikken.nest.unwrap();
    assert!(panel.input.sheet_object.is_some());
    assert_eq!(panel.sheet.unwrap().contour, panel.input.sheet_object);
    assert!(texts(&h, doc, Locale::En).iter().any(|t| t.starts_with("Sheet: contour of a remnant (")));
    assert!(texts(&h, doc, Locale::Nl).iter().any(|t| t.starts_with("Plaat: contour van een restplaat (")));
    press(&mut h, doc, C::NestStart, vec![]).unwrap();
    assert_eq!(
        h.document(doc).unwrap().schikken.nest.unwrap().error,
        Some(SchikkenMessage::Nest(NestFailure::SheetIsPart))
    );
    assert!(
        texts(&h, doc, Locale::En).iter().any(|t| t.contains("one of the parts being nested")),
        "{:?}",
        texts(&h, doc, Locale::En)
    );

    // Back to the bed.
    press(&mut h, doc, C::NestSheet, vec![(C::NestSheet, "0".into())]).unwrap();
    assert_eq!(h.document(doc).unwrap().schikken.nest.unwrap().input.sheet_object, None);
}

#[test]
fn mirroring_and_the_number_of_sheets_are_kept_when_another_option_changes() {
    let (mut h, doc) = window();
    h.invoke(doc, CommandKey::QuickNest);
    press(&mut h, doc, C::NestMirror, vec![(C::NestMirror, "1".into())]).unwrap();
    press(&mut h, doc, C::NestSheets, vec![(C::NestSheets, "3".into())]).unwrap();
    press(&mut h, doc, C::NestPadding, vec![(C::NestPadding, "2".into())]).unwrap();
    let input = h.document(doc).unwrap().schikken.nest.unwrap().input;
    assert_eq!((input.mirror, input.sheets.as_str(), input.padding.as_str()), (NestMirror::Allowed, "3", "2"));
    let _ = SchikkenIntent::CloseNest { document: doc };
}
