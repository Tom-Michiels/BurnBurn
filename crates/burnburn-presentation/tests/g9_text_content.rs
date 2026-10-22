//! G9-editor E07: inserting and deleting in the middle of the text of several texts at once. The
//! rule is one for all of them: a position is counted in characters from the start of each text (a
//! negative one from its end, empty means the end), and a position beyond the end of a shorter text
//! is its end. All texts change in one undo step, or none changes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_document::text::TextLabel;
use burnburn_document::{CompoundLabel, UndoLabel};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, TextEdit, TextField, TextIssue, UiIntent};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::{DocumentId, GeometryId};

fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let regular = src.insert("Liberation Sans", FontStyle::Regular, data);
    (Arc::new(src), regular)
}

/// A window with the given texts, nothing selected.
fn texts(contents: &[&str]) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    for (n, content) in contents.iter().enumerate() {
        if n > 0 {
            h.invoke(doc, CommandKey::ToolSelect);
            h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
        }
        h.send(UiIntent::InsertText { document: doc, text: (*content).into() });
    }
    assert_eq!(h.text_contents(doc).len(), contents.len());
    (h, doc)
}

fn select_all(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    let ids: Vec<GeometryId> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    h.send(UiIntent::Select { document: doc, ids, mode: SelectMode::Replace });
}

fn insert(h: &mut HeadlessEditorHarness, doc: DocumentId, position: &str, text: &str) {
    h.send(UiIntent::Text { document: doc, edit: TextEdit::InsertAt { position: position.into(), text: text.into() } });
}

fn delete(h: &mut HeadlessEditorHarness, doc: DocumentId, position: &str, count: &str) {
    h.send(UiIntent::Text {
        document: doc,
        edit: TextEdit::DeleteAt { position: position.into(), count: count.into() },
    });
}

fn undo_label(h: &HeadlessEditorHarness, doc: DocumentId) -> Option<UndoLabel> {
    h.document(doc).unwrap().undo
}

#[test]
fn inserting_in_the_middle_counts_from_the_start_of_each_text() {
    let (mut h, doc) = texts(&["eerste", "tweede"]);
    select_all(&mut h, doc);
    insert(&mut h, doc, "2", "!");
    assert_eq!(h.text_contents(doc), ["ee!rste", "tw!eede"]);
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Compound(CompoundLabel::EditTexts)));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(h.text_contents(doc), ["eerste", "tweede"], "one undo takes it from both");
}

#[test]
fn a_negative_position_counts_from_the_end_and_an_empty_one_is_the_end() {
    let (mut h, doc) = texts(&["eerste", "tweede"]);
    select_all(&mut h, doc);
    insert(&mut h, doc, "-1", "!");
    assert_eq!(h.text_contents(doc), ["eerst!e", "tweed!e"]);
    insert(&mut h, doc, "", "?");
    assert_eq!(h.text_contents(doc), ["eerst!e?", "tweed!e?"], "the end, like typing");
    insert(&mut h, doc, "0", ">");
    assert_eq!(h.text_contents(doc), [">eerst!e?", ">tweed!e?"], "the start");
}

#[test]
fn a_position_beyond_a_shorter_text_is_its_end() {
    let (mut h, doc) = texts(&["ab", "abcdef"]);
    select_all(&mut h, doc);
    insert(&mut h, doc, "4", "!");
    assert_eq!(h.text_contents(doc), ["ab!", "abcd!ef"]);
    // The same holds from the other side: -9 is before the start of both.
    insert(&mut h, doc, "-9", "<");
    assert_eq!(h.text_contents(doc), ["<ab!", "<abcd!ef"]);
}

#[test]
fn positions_count_characters_not_bytes() {
    let (mut h, doc) = texts(&["naïef", "ééé"]);
    select_all(&mut h, doc);
    insert(&mut h, doc, "3", "|");
    assert_eq!(h.text_contents(doc), ["naï|ef", "ééé|"]);
}

#[test]
fn deleting_removes_the_characters_after_the_position_in_each_text() {
    let (mut h, doc) = texts(&["eerste", "tweede"]);
    select_all(&mut h, doc);
    delete(&mut h, doc, "1", "2");
    assert_eq!(h.text_contents(doc), ["este", "tede"]);
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Compound(CompoundLabel::EditTexts)));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(h.text_contents(doc), ["eerste", "tweede"]);
}

#[test]
fn deleting_more_than_is_left_stops_at_the_end_and_nothing_to_delete_is_no_command() {
    let (mut h, doc) = texts(&["abc", "abcdef"]);
    select_all(&mut h, doc);
    delete(&mut h, doc, "-2", "9");
    assert_eq!(h.text_contents(doc), ["a", "abcd"]);
    let label = undo_label(&h, doc);
    delete(&mut h, doc, "", "3");
    delete(&mut h, doc, "1", "0");
    assert_eq!(h.text_contents(doc), ["a", "abcd"]);
    assert_eq!(undo_label(&h, doc), label, "an edit that changes nothing is not an undo step");
}

#[test]
fn a_single_text_is_a_plain_edit() {
    let (mut h, doc) = texts(&["eerste", "tweede"]);
    let second = h.document(doc).unwrap().objects[1].id;
    h.send(UiIntent::Select { document: doc, ids: vec![second], mode: SelectMode::Replace });
    insert(&mut h, doc, "1", "-");
    assert_eq!(h.text_contents(doc), ["eerste", "t-weede"]);
    assert_eq!(undo_label(&h, doc), Some(UndoLabel::Text(TextLabel::Edit)));
}

#[test]
fn bad_numbers_are_named_in_the_toolbar_and_change_nothing() {
    let (mut h, doc) = texts(&["eerste", "tweede"]);
    select_all(&mut h, doc);
    let label = undo_label(&h, doc);
    insert(&mut h, doc, "twee", "!");
    assert_eq!(
        h.document(doc).unwrap().text_toolbar.unwrap().issue,
        Some(TextIssue::InvalidNumber(TextField::Position))
    );
    delete(&mut h, doc, "1", "veel");
    assert_eq!(h.document(doc).unwrap().text_toolbar.unwrap().issue, Some(TextIssue::InvalidNumber(TextField::Count)));
    delete(&mut h, doc, "1", "-1");
    assert_eq!(h.document(doc).unwrap().text_toolbar.unwrap().issue, Some(TextIssue::InvalidNumber(TextField::Count)));
    assert_eq!(h.text_contents(doc), ["eerste", "tweede"]);
    assert_eq!(undo_label(&h, doc), label);
}

#[test]
fn the_toolbar_lists_the_content_of_every_selected_text() {
    let (mut h, doc) = texts(&["eerste", "tweede"]);
    select_all(&mut h, doc);
    assert_eq!(h.document(doc).unwrap().text_toolbar.unwrap().contents, ["eerste", "tweede"]);
    insert(&mut h, doc, "1", "x");
    assert_eq!(h.document(doc).unwrap().text_toolbar.unwrap().contents, ["exerste", "txweede"]);
}
