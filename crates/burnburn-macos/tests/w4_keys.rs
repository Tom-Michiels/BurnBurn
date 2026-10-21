//! W4-UI-gaten (E04): the Tab key reaches the canvas as `CanvasKey::Tab`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_editor::{CanvasKey, Modifiers};
use burnburn_macos::adapter::{NativeEvent, Reaction, key_code, translate};
use burnburn_presentation::{HeadlessEditorHarness, UiIntent};

#[test]
fn tab_is_translated_to_the_canvas_tab_key() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let event = NativeEvent::KeyDown { document: doc, key_code: key_code::TAB, flags: 0 };
    assert_eq!(
        translate(event, &h.view()),
        Reaction::Intent(UiIntent::Key { document: doc, key: CanvasKey::Tab, modifiers: Modifiers::default() })
    );
}
