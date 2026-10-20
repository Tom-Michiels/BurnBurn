//! X03 (W3-UI-app): the display unit preference. Lengths in the inspector are shown in the chosen
//! unit, a bare number typed in a length field is in that unit, an explicit unit always wins, and
//! the document keeps its millimetres.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_presentation::{AppIntent, CommandKey, FieldKey, HeadlessEditorHarness, PrefKey, UiIntent};
use burnburn_types::{DocumentId, Unit};

/// A 120 × 80 mm rectangle with its lower left corner at (10, 10) mm.
fn rectangle() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (130.0, 90.0));
    h.invoke(doc, CommandKey::ToolSelect);
    (h, doc)
}

fn units(h: &mut HeadlessEditorHarness, text: &str) {
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::Units, text: text.into() }));
}

fn shown(h: &HeadlessEditorHarness, doc: DocumentId, key: FieldKey) -> (String, Unit) {
    let field = h.document(doc).unwrap().inspector.unwrap().field(key).unwrap().clone();
    (field.text, field.unit)
}

fn commit(h: &mut HeadlessEditorHarness, doc: DocumentId, field: FieldKey, text: &str) {
    h.send(UiIntent::CommitField { document: doc, field, text: text.into() });
}

fn mm(text: &str) -> f64 {
    text.parse().unwrap()
}

#[test]
fn the_inspector_shows_lengths_in_the_chosen_unit_and_says_which() {
    let (mut h, doc) = rectangle();
    assert_eq!(shown(&h, doc, FieldKey::Width), ("120".into(), Unit::Millimeter));
    units(&mut h, "inch");
    assert_eq!(shown(&h, doc, FieldKey::Width), ("4.724".into(), Unit::Inch));
    assert_eq!(shown(&h, doc, FieldKey::Height), ("3.15".into(), Unit::Inch));
    assert_eq!(shown(&h, doc, FieldKey::X), ("0.394".into(), Unit::Inch));
    assert_eq!(shown(&h, doc, FieldKey::Y), ("0.394".into(), Unit::Inch));
    // Percent, shear and rotation are not lengths.
    assert_eq!(shown(&h, doc, FieldKey::ScaleX).0, "100");
    assert_eq!(shown(&h, doc, FieldKey::Rotation).0, "0");
    units(&mut h, "mm");
    assert_eq!(shown(&h, doc, FieldKey::Width), ("120".into(), Unit::Millimeter), "back to millimetres");
}

#[test]
fn a_bare_number_in_a_length_field_is_in_the_display_unit_and_an_explicit_unit_wins() {
    let (mut h, doc) = rectangle();
    units(&mut h, "inch");
    commit(&mut h, doc, FieldKey::Width, "2");
    assert_eq!(shown(&h, doc, FieldKey::Width).0, "2", "2 in");
    commit(&mut h, doc, FieldKey::Height, "30 mm");
    let height_in_inches = mm(&shown(&h, doc, FieldKey::Height).0);
    assert!((height_in_inches - 30.0 / 25.4).abs() < 1e-3, "{height_in_inches}");
    // The same document seen in millimetres: the physical values are exact.
    units(&mut h, "mm");
    assert_eq!(shown(&h, doc, FieldKey::Width).0, "50.8");
    assert_eq!(shown(&h, doc, FieldKey::Height).0, "30");
}

#[test]
fn position_fields_take_the_display_unit_too_and_may_be_negative() {
    let (mut h, doc) = rectangle();
    units(&mut h, "inch");
    commit(&mut h, doc, FieldKey::X, "1");
    units(&mut h, "mm");
    assert_eq!(shown(&h, doc, FieldKey::X).0, "25.4");
    units(&mut h, "inch");
    commit(&mut h, doc, FieldKey::Y, "-0,5");
    units(&mut h, "mm");
    assert_eq!(shown(&h, doc, FieldKey::Y).0, "-12.7");
}

#[test]
fn the_exact_size_typed_while_drawing_follows_the_unit() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    units(&mut h, "inch");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.begin_drag(doc, (10.0, 10.0), (50.0, 40.0));
    h.send(UiIntent::CommitToolSize { document: doc, width: "2".into(), height: "1".into() });
    units(&mut h, "mm");
    assert_eq!(shown(&h, doc, FieldKey::Width).0, "50.8", "2 in");
    assert_eq!(shown(&h, doc, FieldKey::Height).0, "25.4", "1 in");
}

#[test]
fn units_never_reach_a_project() {
    let mm_bytes = {
        let (mut h, doc) = rectangle();
        save(&mut h, doc)
    };
    let inch_bytes = {
        let (mut h, doc) = rectangle();
        units(&mut h, "inch");
        save(&mut h, doc)
    };
    assert_eq!(mm_bytes, inch_bytes);
}

fn save(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Vec<u8> {
    let path = std::path::PathBuf::from("/work/units.burnburn");
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    burnburn_io::fs::FileSystem::read(h.fs(), &path, u64::MAX).expect("saved")
}
