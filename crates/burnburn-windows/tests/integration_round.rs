//! The integration-round functions on the Windows host, headless (the Win32 layer itself is only
//! type-checked for `x86_64-pc-windows-msvc` here; no Windows runner): the rotation field, typed
//! and IME text on the canvas (`WM_CHAR`, UTF-16 surrogate pairs) as one `InsertText` per
//! character commit, and the recovery question's answers.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};

use burnburn_presentation::{CommandKey, FieldKey, HeadlessEditorHarness, UiIntent};
use burnburn_types::DocumentId;
use burnburn_windows::adapter::{
    CharDecoder, Control, INSPECTOR_FIELDS, MessageContext, Reaction, button, msg, on_message, on_recovery_box,
    recovery_dir,
};

fn one_rectangle() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    (h, doc)
}

fn ctx<'a>(document: DocumentId, control: Control, text: &'a str) -> MessageContext<'a> {
    MessageContext { document, control, dpi: 96, text, selected_rows: &[], alt: false, cursor_px: [0, 0], time_ms: 0 }
}

#[test]
fn inspector_has_a_rotation_field_that_commits_raw_text() {
    assert_eq!(INSPECTOR_FIELDS[..], [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height, FieldKey::Rotation]);
    let (h, doc) = one_rectangle();
    let r = on_message(&ctx(doc, Control::Field(FieldKey::Rotation), "45"), msg::WM_KEYDOWN, 0x0D, 0, &h.view());
    assert_eq!(
        r,
        Reaction::Intent(UiIntent::CommitField { document: doc, field: FieldKey::Rotation, text: "45".into() })
    );
}

#[test]
fn utf16_units_become_final_text_and_controls_are_not_text() {
    let mut d = CharDecoder::default();
    assert_eq!(d.push(u16::from(b'a')), Some("a".to_owned()));
    // 😀 = U+1F600 = D83D DE00: nothing until the pair is complete.
    assert_eq!(d.push(0xD83D), None);
    assert_eq!(d.push(0xDE00), Some("😀".to_owned()));
    assert_eq!(d.push(0x65E5), Some("日".to_owned()));
    // A lone low surrogate, or a high one followed by a normal unit, is dropped, not mangled.
    assert_eq!(d.push(0xDE00), None);
    assert_eq!(d.push(0xD83D), None);
    assert_eq!(d.push(u16::from(b'b')), Some("b".to_owned()));
    // Return, Backspace, Esc and Tab arrive as WM_CHAR too: they are keys, not text.
    for c in [0x0D, 0x08, 0x1B, 0x09] {
        assert_eq!(d.push(c), None, "{c:#x}");
    }
}

#[test]
fn a_character_on_the_canvas_is_one_insert_text() {
    let (h, doc) = one_rectangle();
    let r = on_message(&ctx(doc, Control::Canvas, "日"), msg::WM_CHAR, 0x65E5, 0, &h.view());
    assert_eq!(r, Reaction::Intent(UiIntent::InsertText { document: doc, text: "日".into() }));
    // Half a surrogate pair (no text yet) is handled, not passed to DefWindowProc.
    assert_eq!(on_message(&ctx(doc, Control::Canvas, ""), msg::WM_CHAR, 0xD83D, 0, &h.view()), Reaction::Nothing);
}

#[test]
fn recovery_question_restores_discards_or_keeps_for_later() {
    let (h, _doc) = one_rectangle();
    let path = PathBuf::from("C:/r/00ff.burnburn");
    assert_eq!(
        on_recovery_box(&path, button::IDYES),
        Reaction::Intent(UiIntent::RestoreRecovery { path: path.clone() })
    );
    assert_eq!(
        on_recovery_box(&path, button::IDNO),
        Reaction::Intent(UiIntent::DiscardRecovery { path: path.clone() })
    );
    assert_eq!(on_recovery_box(&path, button::IDCANCEL), Reaction::Nothing, "Annuleer = later: the file stays");
    assert_eq!(on_recovery_box(&path, 0), Reaction::Nothing, "a failed box never deletes");
    let _ = h;
    assert_eq!(
        recovery_dir(Path::new("C:/Users/x/AppData/Local")),
        PathBuf::from("C:/Users/x/AppData/Local").join("BurnBurn").join("Recovery")
    );
}
