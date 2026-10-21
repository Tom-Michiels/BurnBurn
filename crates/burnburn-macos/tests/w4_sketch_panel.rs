//! W4-UI-gaten, schetspaneel headless: "Herstel laatste geldige" (S04) and parameters verwijderen
//! (S02) as panel model and native events. No AppKit here; the widgets only copy this model.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_macos::adapter::{NativeEvent, Reaction, translate};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::sketch_panel;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SketchDimensionKind, SketchStatus, UiIntent};
use burnburn_types::DocumentId;

fn intent(h: &HeadlessEditorHarness, event: NativeEvent) -> Option<UiIntent> {
    match translate(event, &h.view()) {
        Reaction::Intent(i) => Some(i),
        _ => None,
    }
}

/// A new sketch with a 20 × 10 rectangle drawn into it.
fn rectangle_sketch() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::SketchNew);
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    (h, doc)
}

fn conflict(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    for text in ["20 mm", "30 mm"] {
        h.send(UiIntent::SketchSelect {
            document: doc,
            entities: vec![burnburn_document::sketch::EntityId::from_raw(4)],
            constraints: vec![],
            mode: burnburn_presentation::SelectMode::Replace,
        });
        h.send(UiIntent::AddSketchDimension { document: doc, kind: SketchDimensionKind::Auto, text: text.into() });
    }
}

#[test]
fn the_restore_button_applies_only_to_a_failed_sketch_and_sends_the_restore_intent() {
    let (mut h, doc) = rectangle_sketch();
    assert!(!sketch_panel(&h.view(), doc, Locale::Nl).unwrap().restore_enabled, "a valid sketch");
    assert_eq!(intent(&h, NativeEvent::SketchRestoreRequested { document: doc }), None);

    conflict(&mut h, doc);
    let vm = h.document(doc).unwrap().sketch_mode.unwrap();
    assert_eq!(vm.status, SketchStatus::Failed);
    assert!(sketch_panel(&h.view(), doc, Locale::Nl).unwrap().restore_enabled);
    let sent = intent(&h, NativeEvent::SketchRestoreRequested { document: doc }).expect("intent");
    assert_eq!(sent, UiIntent::RestoreLastValidSketch { document: doc, sketch: vm.sketch });
    h.send(sent);
    let after = h.document(doc).unwrap().sketch_mode.unwrap();
    assert_eq!(after.status, SketchStatus::Valid, "both contradicting lengths were undone");
    assert!(!sketch_panel(&h.view(), doc, Locale::En).unwrap().restore_enabled);
}

#[test]
fn removing_a_parameter_row_sends_its_name_and_a_bad_row_sends_nothing() {
    let (mut h, doc) = rectangle_sketch();
    let sketch = h.document(doc).unwrap().sketch_mode.unwrap().sketch;
    for (name, text) in [("speling", "0,2 mm"), ("dikte", "3 mm")] {
        h.send(UiIntent::SetSketchParameter { document: doc, sketch, name: name.into(), text: text.into() });
    }
    let panel = sketch_panel(&h.view(), doc, Locale::Nl).unwrap();
    assert_eq!(panel.parameters.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["speling", "dikte"]);
    assert_eq!(panel.parameters[0].1, "0.2 mm", "the decimal comma was read as a decimal");
    let sent = intent(&h, NativeEvent::SketchParameterRemoveRequested { document: doc, row: 1 }).expect("intent");
    assert_eq!(sent, UiIntent::RemoveSketchParameter { document: doc, sketch, name: "dikte".into() });
    h.send(sent);
    let names: Vec<String> =
        sketch_panel(&h.view(), doc, Locale::Nl).unwrap().parameters.into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["speling"]);
    assert_eq!(intent(&h, NativeEvent::SketchParameterRemoveRequested { document: doc, row: 5 }), None);
}

#[test]
fn the_new_texts_exist_in_both_languages() {
    use burnburn_macos::l10n::{HostText, host_text};
    for text in [HostText::SketchRestoreLast, HostText::SketchParameterDelete] {
        assert!(!host_text(text, Locale::Nl).is_empty() && !host_text(text, Locale::En).is_empty());
        assert_ne!(host_text(text, Locale::Nl), host_text(text, Locale::En));
    }
}
