//! G8-editor E07: Convert to Path and typing work on several texts at once, each in one undo step
//! (G7 did the toolbar). A selection that holds anything but texts stays as it was: Convert to Path
//! refuses it and typing does not touch the other objects.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_document::text::TextLabel;
use burnburn_document::{CompoundLabel, UndoLabel};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, UiIntent};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::{DocumentId, GeometryId};

fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let regular = src.insert("Liberation Sans", FontStyle::Regular, data);
    (Arc::new(src), regular)
}

/// A window with the texts "eerste" and "tweede", nothing selected.
fn two_texts() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    h.send(UiIntent::InsertText { document: doc, text: "eerste".into() });
    h.invoke(doc, CommandKey::ToolSelect);
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    h.send(UiIntent::InsertText { document: doc, text: "tweede".into() });
    assert_eq!(h.text_contents(doc).len(), 2);
    (h, doc)
}

fn objects(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<GeometryId> {
    h.document(doc).unwrap().objects.iter().map(|o| o.id).collect()
}

fn select_all(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    let ids = objects(h, doc);
    h.send(UiIntent::Select { document: doc, ids, mode: SelectMode::Replace });
}

fn enabled(h: &HeadlessEditorHarness, command: CommandKey) -> bool {
    h.view().commands.get(&command).is_some_and(|c| c.enabled)
}

fn outline_area(h: &HeadlessEditorHarness, doc: DocumentId) -> f64 {
    h.scene(doc).unwrap().items.iter().map(|i| i.path.signed_area().abs()).sum()
}

#[test]
fn convert_to_path_turns_every_selected_text_into_plain_objects_in_one_undo_step() {
    let (mut h, doc) = two_texts();
    select_all(&mut h, doc);
    assert!(enabled(&h, CommandKey::ShapesConvertToPath), "two texts can be converted together");
    let outlines = objects(&h, doc).len();
    let area = outline_area(&h, doc);
    h.invoke(doc, CommandKey::ShapesConvertToPath);
    assert!(h.text_contents(doc).is_empty(), "both texts are gone as texts");
    assert_eq!(objects(&h, doc).len(), outlines, "their outlines stay as ordinary objects");
    assert!((outline_area(&h, doc) - area).abs() < 1e-6, "the drawing is unchanged");
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Compound(CompoundLabel::ConvertTexts)));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(h.text_contents(doc).len(), 2, "one undo brings both texts back");
    h.invoke(doc, CommandKey::Redo);
    assert!(h.text_contents(doc).is_empty());
}

#[test]
fn convert_to_path_of_a_single_text_stays_a_plain_single_step() {
    let (mut h, doc) = two_texts();
    let first = objects(&h, doc)[0];
    h.send(UiIntent::Select { document: doc, ids: vec![first], mode: SelectMode::Replace });
    h.invoke(doc, CommandKey::ShapesConvertToPath);
    assert_eq!(h.text_contents(doc).len(), 1);
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Text(TextLabel::ConvertToPath)));
}

#[test]
fn convert_to_path_leaves_the_texts_alone_when_the_selection_holds_something_else() {
    let (mut h, doc) = two_texts();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (150.0, 150.0), (180.0, 170.0));
    h.invoke(doc, CommandKey::ToolSelect);
    select_all(&mut h, doc);
    let before = h.text_contents(doc);
    h.invoke(doc, CommandKey::ShapesConvertToPath);
    assert_eq!(h.text_contents(doc), before, "the texts were not touched");
}

#[test]
fn typing_with_several_texts_selected_extends_each_of_them_in_one_undo_step() {
    let (mut h, doc) = two_texts();
    select_all(&mut h, doc);
    h.send(UiIntent::InsertText { document: doc, text: "!".into() });
    assert_eq!(h.text_contents(doc), ["eerste!", "tweede!"], "no third text appears");
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Compound(CompoundLabel::EditTexts)));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(h.text_contents(doc), ["eerste", "tweede"], "one undo takes the typing from both");
}

#[test]
fn typing_with_one_text_selected_stays_a_plain_edit() {
    let (mut h, doc) = two_texts();
    let ids = objects(&h, doc);
    h.send(UiIntent::Select { document: doc, ids: vec![ids[1]], mode: SelectMode::Replace });
    h.send(UiIntent::InsertText { document: doc, text: "?".into() });
    assert_eq!(h.text_contents(doc), ["eerste", "tweede?"]);
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Text(TextLabel::Edit)));
}
