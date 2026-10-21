//! W4-UI-gaten, tekst (E07, E08).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, OperationKind, SelectMode, TextEdit, UiIntent,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontStyle, MemoryFontSource};
use burnburn_types::DocumentId;

use common::*;

fn fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let regular = src.insert("Liberation Sans", FontStyle::Regular, data);
    (Arc::new(src), regular)
}

fn text_doc(content: &str) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let (src, font) = fonts();
    h.controller().attach_fonts(src, font);
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h.send(UiIntent::InsertText { document: doc, text: content.into() });
    (h, doc)
}

/// E08: single-line text is drawn as open stroke paths, and a Fill operation on it does not
/// scan anything (an open stroke has no inside), so it stays unfilled.
#[test]
fn single_line_text_stays_open_and_unfilled() {
    let (mut h, doc) = text_doc("HIT");
    h.send(UiIntent::Text { document: doc, edit: TextEdit::SingleLine(true) });
    let scene = h.scene(doc).unwrap();
    assert!(!scene.items.is_empty());
    for item in &scene.items {
        assert!(item.path.subpaths().iter().all(|s| !s.is_closed()), "strokes stay open");
    }
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AddOperationText {
        document: doc,
        name: "Vullen".into(),
        kind: OperationKind::Fill,
        speed: "300".into(),
        power: "30".into(),
    });
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let plan = h.document(doc).unwrap().plan.expect("compiled");
    assert!(plan.preview.scan_length_mm < 1e-9, "no fill scan lines over open strokes: {}", plan.preview.scan_length_mm);
    assert!(plan.warnings + plan.errors > 0, "the operator is told the fill has nothing to fill: {:?}", plan.findings);
}

// ---------------------------------------------------------------------------------------------
// E07: ontbrekend lettertype
// ---------------------------------------------------------------------------------------------

fn two_fonts() -> (Arc<MemoryFontSource>, FontRef) {
    let mut src = MemoryFontSource::new();
    let liberation = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let biz = Arc::new(FontData::from_bytes(read_fixture("fonts", "BIZUDPGothic-Regular.ttf").unwrap()).unwrap());
    let regular = src.insert("Liberation Sans", FontStyle::Regular, liberation);
    src.insert("BIZ UDPGothic", FontStyle::Regular, biz);
    (Arc::new(src), regular)
}

fn ink_width(h: &HeadlessEditorHarness, doc: DocumentId) -> f64 {
    let scene = h.scene(doc).unwrap();
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    for item in &scene.items {
        if let Some(b) = item.path.bounds() {
            min = min.min(b.min().x);
            max = max.max(b.max().x);
        }
    }
    max - min
}

/// A project whose text uses a font this computer does not have keeps showing the stored
/// outlines, says which font is missing, and offers the fonts that are there; choosing one is a
/// single undoable step.
#[test]
fn missing_font_shows_stored_outline_and_replacement_choice() {
    let mut first = HeadlessEditorHarness::new();
    let doc = first.new_document().unwrap();
    let (src, font) = two_fonts();
    first.controller().attach_fonts(src, font);
    first.send(UiIntent::InsertText { document: doc, text: "HIT".into() });
    let width_stored = ink_width(&first, doc);
    first.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let path = std::path::PathBuf::from("/work/tekst.burnburn");
    first.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    first.run_tasks();

    // Another computer: only BIZ UDPGothic is installed.
    let mut h = HeadlessEditorHarness::with_fs(first.shared_fs());
    let mut only = MemoryFontSource::new();
    let biz = Arc::new(FontData::from_bytes(read_fixture("fonts", "BIZUDPGothic-Regular.ttf").unwrap()).unwrap());
    let biz_ref = only.insert("BIZ UDPGothic", FontStyle::Regular, biz);
    h.controller().attach_fonts(Arc::new(only), biz_ref);
    h.send(UiIntent::Open { path });
    let doc = h
        .run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            burnburn_presentation::HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened");
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    let bar = h.document(doc).unwrap().text_toolbar.expect("toolbar");
    assert!(bar.font_missing);
    let choice = bar.missing_font.expect("a replacement choice is offered");
    assert_eq!(choice.requested, "Liberation Sans");
    assert_eq!(choice.replacements, ["BIZ UDPGothic"]);
    assert!((ink_width(&h, doc) - width_stored).abs() < 1e-9, "the stored outline is still drawn");

    let revision = h.document(doc).unwrap().revision;
    h.send(UiIntent::Text { document: doc, edit: TextEdit::Family("BIZ UDPGothic".into()) });
    let after = h.document(doc).unwrap();
    assert!(after.revision > revision);
    let bar = after.text_toolbar.unwrap();
    assert!(!bar.font_missing && bar.missing_font.is_none());
    assert!((ink_width(&h, doc) - width_stored).abs() > 1e-6, "new font, new outline");
    h.invoke(doc, CommandKey::Undo);
    assert!(h.document(doc).unwrap().text_toolbar.unwrap().font_missing, "one undo brings the stored state back");
}
