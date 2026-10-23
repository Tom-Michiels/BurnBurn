//! LFB-rest: the Windows status line shows the tone of a message (LF3 `StatusTone`) as a symbol and
//! a colour, never as colour alone. (The `STATIC` control painting is in `win32`: Windows runner.)

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, MessageKey};
use burnburn_windows::l10n::Locale;
use burnburn_windows::status_line::{StatusLine, status_line};
use burnburn_windows::style::{Rgb, StatusTone, contrast_ratio};

/// The dialog background the `STATIC` control is painted on (`COLOR_BTNFACE`, 0xF0F0F0).
const BTNFACE: Rgb = Rgb(0xF0, 0xF0, 0xF0);

#[test]
fn an_error_and_a_warning_carry_their_glyph_and_neutral_text_stays_plain() {
    let error = StatusLine::message(MessageKey::OpenFailed, Locale::Nl);
    assert_eq!(error.tone, StatusTone::Error);
    assert!(error.text.starts_with("\u{2716} "), "the error glyph comes first: {:?}", error.text);

    let warning = StatusLine::message(MessageKey::PreviousVersionKept, Locale::Nl);
    assert_eq!(warning.tone, StatusTone::Warning);
    assert!(warning.text.starts_with("\u{26A0} "), "{:?}", warning.text);

    let neutral = StatusLine::message(MessageKey::NothingFound, Locale::Nl);
    assert_eq!(neutral.tone, StatusTone::Neutral);
    assert!(neutral.text.chars().next().is_some_and(|c| c.is_alphanumeric()), "no glyph: {:?}", neutral.text);
}

#[test]
fn the_three_tones_have_three_different_colours_that_read_on_the_dialog_background() {
    let lines = [
        StatusLine::toned("a", StatusTone::Neutral),
        StatusLine::toned("a", StatusTone::Warning),
        StatusLine::toned("a", StatusTone::Error),
    ];
    for line in &lines {
        let ratio = contrast_ratio(line.color(), BTNFACE);
        assert!(ratio >= 4.5, "{:?} only {ratio:.2}:1", line.tone);
    }
    assert!(lines[0].color() != lines[1].color() && lines[1].color() != lines[2].color());
    let Rgb(r, g, b) = lines[2].color();
    assert_eq!(lines[2].colorref(), u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16, "COLORREF is 0x00BBGGRR");
}

#[test]
fn an_empty_message_is_empty_and_neutral_without_a_dangling_glyph() {
    let line = StatusLine::toned("", StatusTone::Error);
    assert_eq!((line.text.as_str(), line.tone), ("", StatusTone::Neutral));
}

#[test]
fn the_line_of_a_document_follows_its_newest_message() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert_eq!(status_line(&h.document(doc).unwrap(), Locale::Nl).text, "", "nothing to say yet");
    // Looking for duplicates in an empty drawing finds none: the newest message says so (neutral).
    h.invoke(doc, CommandKey::DeleteDuplicates);
    let vm = h.document(doc).unwrap();
    let last = vm.messages.last().expect("the command leaves a message").key;
    let line = status_line(&vm, Locale::Nl);
    assert_eq!(line, StatusLine::message(last, Locale::Nl));
    assert!(!line.text.is_empty());
}
