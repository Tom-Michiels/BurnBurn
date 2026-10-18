//! IM20 IME contract, headless part: composition (marked text) lives in the host and is never a
//! document change; only the final text (`insertText:`, or accepting the marked text) is handed on,
//! exactly once.

use burnburn_macos::text_input::Composer;

#[test]
fn marked_text_is_not_committed_and_final_text_is_committed_once() {
    let mut c = Composer::default();
    c.set_marked("n", (1, 0));
    c.set_marked("に", (1, 0));
    c.set_marked("日本", (2, 0));
    assert!(c.has_marked());
    assert_eq!(c.marked_text(), "日本");
    assert_eq!(c.selected_range(), (2, 0));
    assert_eq!(c.insert("日本"), Some("日本".to_owned()));
    assert!(!c.has_marked(), "commit ends the composition");
    assert_eq!(c.marked_text(), "");
    // Nothing more to commit afterwards.
    assert_eq!(c.unmark(), None);
}

#[test]
fn accepting_marked_text_commits_it_and_cancelling_does_not() {
    let mut c = Composer::default();
    c.set_marked("ü", (1, 0));
    assert_eq!(c.unmark(), Some("ü".to_owned()));
    c.set_marked("e", (1, 0));
    // The input method cancels by setting empty marked text.
    c.set_marked("", (0, 0));
    assert!(!c.has_marked());
    assert_eq!(c.unmark(), None);
}

#[test]
fn empty_or_control_only_text_is_no_commit() {
    let mut c = Composer::default();
    assert_eq!(c.insert(""), None);
    assert_eq!(c.insert("\u{7f}"), None);
    assert_eq!(c.insert("a\u{0}b"), Some("ab".to_owned()));
}

/// Selected range from AppKit is clamped to the marked text (UTF-16 offsets in AppKit; the
/// composer stores char positions).
#[test]
fn selected_range_is_clamped_to_the_marked_text() {
    let mut c = Composer::default();
    c.set_marked("ab", (10, 5));
    assert_eq!(c.selected_range(), (2, 0));
}
