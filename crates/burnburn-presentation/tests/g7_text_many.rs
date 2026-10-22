//! G7-editor E07: several texts edited at once. The toolbar follows every selected text object,
//! says which values differ, and one edit changes them all in one undo step. A refusal for one of
//! them refuses the whole edit, and a selection that holds anything but texts has no toolbar.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_document::text::{TextLabel, TextMode};
use burnburn_document::{CompoundLabel, UndoLabel};
use burnburn_presentation::{
    Align, CommandKey, DocumentViewModel, HeadlessEditorHarness, SelectMode, TextEdit, TextIssue, UiIntent,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::{DocumentId, GeometryId};

fn face(name: &str) -> Arc<FontData> {
    Arc::new(FontData::from_bytes(read_fixture("fonts", name).unwrap()).unwrap())
}

/// "Liberation Sans" with a regular and a stand-in bold face, "BIZ UDPGothic" regular only.
fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let liberation = face("LiberationSans-Regular.ttf");
    let regular = src.insert("Liberation Sans", FontStyle::Regular, liberation.clone());
    src.insert("Liberation Sans", FontStyle::Bold, liberation);
    src.insert("BIZ UDPGothic", FontStyle::Regular, face("BIZUDPGothic-Regular.ttf"));
    (Arc::new(src), regular)
}

/// A window with texts "eerste" and "tweede" (two objects), nothing selected.
fn two_texts() -> (HeadlessEditorHarness, DocumentId) {
    two_texts_of("eerste", "tweede")
}

fn two_texts_of(first: &str, second: &str) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    h.send(UiIntent::InsertText { document: doc, text: first.into() });
    h.invoke(doc, CommandKey::ToolSelect);
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    h.send(UiIntent::InsertText { document: doc, text: second.into() });
    assert_eq!(h.text_contents(doc).len(), 2);
    (h, doc)
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn objects(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<GeometryId> {
    vm(h, doc).objects.iter().map(|o| o.id).collect()
}

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: Vec<GeometryId>) {
    h.send(UiIntent::Select { document: doc, ids, mode: SelectMode::Replace });
}

fn select_all(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    let all = objects(h, doc);
    select(h, doc, all);
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, edit: TextEdit) {
    h.send(UiIntent::Text { document: doc, edit });
}

fn sources(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Vec<burnburn_text::TextObject> {
    h.controller().text_sources(doc)
}

#[test]
fn the_toolbar_follows_all_selected_texts_and_counts_them() {
    let (mut h, doc) = two_texts();
    select_all(&mut h, doc);
    let bar = vm(&h, doc).text_toolbar.expect("a toolbar for two texts");
    assert_eq!(bar.count, 2);
    assert_eq!(bar.family, "Liberation Sans");
    assert_eq!(bar.mixed, burnburn_presentation::TextMixed::default(), "they are alike");
    assert_eq!(bar.height, "10");
}

#[test]
fn a_value_that_differs_is_marked_mixed_and_shows_the_first_texts_value() {
    let (mut h, doc) = two_texts();
    let ids = objects(&h, doc);
    select(&mut h, doc, vec![ids[1]]);
    edit(&mut h, doc, TextEdit::Height("20".into()));
    edit(&mut h, doc, TextEdit::Uppercase(true));
    select_all(&mut h, doc);
    let bar = vm(&h, doc).text_toolbar.unwrap();
    assert_eq!(bar.count, 2);
    assert!(bar.mixed.height && bar.mixed.uppercase);
    assert!(!bar.mixed.h_space && !bar.mixed.family && !bar.mixed.align && !bar.mixed.single_line);
    assert_eq!(bar.height, "10", "the first text's value");
    assert!(!bar.uppercase);
}

#[test]
fn one_edit_changes_every_selected_text_in_one_undo_step() {
    let (mut h, doc) = two_texts();
    select_all(&mut h, doc);
    edit(&mut h, doc, TextEdit::Height("12".into()));
    let s = sources(&mut h, doc);
    assert!(s.iter().all(|t| (t.size.em_mm() - 12.0).abs() < 1e-9), "both are 12 mm now");
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Compound(CompoundLabel::EditTexts)));
    edit(&mut h, doc, TextEdit::Align(Align::Right));
    assert!(sources(&mut h, doc).iter().all(|t| t.layout.align == Align::Right));
    h.invoke(doc, CommandKey::Undo);
    let s = sources(&mut h, doc);
    assert!(s.iter().all(|t| t.layout.align == Align::Left && (t.size.em_mm() - 12.0).abs() < 1e-9));
    h.invoke(doc, CommandKey::Undo);
    assert!(sources(&mut h, doc).iter().all(|t| (t.size.em_mm() - 10.0).abs() < 1e-9), "one step undoes both");
    h.invoke(doc, CommandKey::Redo);
    assert!(sources(&mut h, doc).iter().all(|t| (t.size.em_mm() - 12.0).abs() < 1e-9));
}

#[test]
fn the_outlines_of_every_text_follow_the_edit() {
    let (mut h, doc) = two_texts();
    let width = |h: &HeadlessEditorHarness, doc| {
        let scene = h.scene(doc).unwrap();
        scene.items.iter().filter_map(|i| i.path.bounds()).map(|b| b.width()).collect::<Vec<_>>()
    };
    let before = width(&h, doc);
    select_all(&mut h, doc);
    edit(&mut h, doc, TextEdit::Height("20".into()));
    let after = width(&h, doc);
    for (b, a) in before.iter().zip(&after) {
        assert!((a / b - 2.0).abs() < 0.02, "twice the size: {b} -> {a}");
    }
}

#[test]
fn an_edit_that_only_one_text_needs_is_a_plain_single_edit() {
    let (mut h, doc) = two_texts();
    let ids = objects(&h, doc);
    select(&mut h, doc, vec![ids[0]]);
    edit(&mut h, doc, TextEdit::Uppercase(true));
    select_all(&mut h, doc);
    edit(&mut h, doc, TextEdit::Uppercase(true));
    assert!(sources(&mut h, doc).iter().all(|t| t.layout.uppercase));
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Text(TextLabel::Edit)), "only the second text changed");
    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, TextEdit::Uppercase(true));
    assert_eq!(vm(&h, doc).revision, rev, "nothing left to change: no command");
}

#[test]
fn a_toggle_one_of_them_cannot_take_refuses_the_whole_edit() {
    let (mut h, doc) = two_texts();
    let ids = objects(&h, doc);
    select(&mut h, doc, vec![ids[1]]);
    edit(&mut h, doc, TextEdit::Family("BIZ UDPGothic".into()));
    select_all(&mut h, doc);
    let rev = vm(&h, doc).revision;
    let before = sources(&mut h, doc);
    edit(&mut h, doc, TextEdit::Bold(true));
    assert_eq!(vm(&h, doc).revision, rev, "nothing was written, not even for the text that could");
    assert_eq!(sources(&mut h, doc), before);
    assert_eq!(vm(&h, doc).text_toolbar.unwrap().issue, Some(TextIssue::StyleMissing));
    let bar = vm(&h, doc).text_toolbar.unwrap();
    assert!(!bar.bold.enabled, "and the toggle is off while one of them lacks the face");
    assert!(bar.mixed.family);
}

#[test]
fn a_bad_number_changes_none_of_them() {
    let (mut h, doc) = two_texts();
    select_all(&mut h, doc);
    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, TextEdit::Height("groot".into()));
    assert_eq!(vm(&h, doc).revision, rev);
    assert!(matches!(vm(&h, doc).text_toolbar.unwrap().issue, Some(TextIssue::InvalidNumber(_))));
    edit(&mut h, doc, TextEdit::Height("0".into()));
    assert_eq!(vm(&h, doc).revision, rev);
}

#[test]
fn single_line_and_welded_follow_the_same_rule_for_all() {
    // The built-in stroke set draws digits and capitals.
    let (mut h, doc) = two_texts_of("EERSTE", "TWEEDE");
    select_all(&mut h, doc);
    edit(&mut h, doc, TextEdit::SingleLine(true));
    assert!(sources(&mut h, doc).iter().all(|t| t.mode == TextMode::SingleLine));
    edit(&mut h, doc, TextEdit::Welded(true));
    assert!(sources(&mut h, doc).iter().all(|t| !t.layout.welded), "welded needs outline text: refused for all");
    assert_eq!(vm(&h, doc).text_toolbar.unwrap().issue, Some(TextIssue::WeldedNeedsOutline));
}

#[test]
fn a_text_with_anything_else_has_no_toolbar_and_an_edit_changes_nothing() {
    let (mut h, doc) = two_texts();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (150.0, 150.0), (180.0, 170.0));
    h.invoke(doc, CommandKey::ToolSelect);
    select_all(&mut h, doc);
    assert!(vm(&h, doc).text_toolbar.is_none());
    let rev = vm(&h, doc).revision;
    edit(&mut h, doc, TextEdit::Uppercase(true));
    assert_eq!(vm(&h, doc).revision, rev);
}

#[test]
fn a_text_of_several_lines_counts_once() {
    let (mut h, doc) = two_texts();
    let ids = objects(&h, doc);
    select(&mut h, doc, vec![ids[0]]);
    h.send(UiIntent::InsertText { document: doc, text: "\nderde".into() });
    select_all(&mut h, doc);
    let bar = vm(&h, doc).text_toolbar.unwrap();
    assert_eq!(bar.count, 2, "two text objects, three outlines");
}
