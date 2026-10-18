//! IM20 IME on the canvas: the canvas is an NSTextInputClient. Marked (composition) text stays in
//! the host — no intent, no document change; only final text leaves the composer, once, as one
//! `UiIntent::InsertText` (one text command). Real key events go through `interpretKeyEvents:`
//! (the input-method route), Esc still reaches the tool.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("canvas_text_input: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("canvas_marked_text_is_not_a_document_change_and_final_text_is_one_insert", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_editor::CanvasKey;
    use burnburn_presentation::UiIntent;
    use common::expect;
    use objc2::{Message, msg_send};
    use objc2_foundation::{NSRange, NSString};

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (40.0, 20.0))?;
    let revision = s.doc(doc).ok_or("no document")?.revision;
    let canvas = s.app.with_window(doc, |w| w.canvas_view.retain()).ok_or("no canvas")?;
    s.app.window(doc).ok_or("no window")?.makeFirstResponder(Some(&canvas));
    s.app.take_dispatched();
    s.app.take_text_commits();

    let none = NSRange::new(usize::MAX as _, 0);
    // SAFETY: NSTextInputClient messages to our own canvas view, as the input method sends them.
    unsafe {
        let _: () = msg_send![&canvas, setMarkedText: &*NSString::from_str("に"), selectedRange: NSRange::new(1, 0), replacementRange: none];
    }
    // SAFETY: as above.
    let marked: bool = unsafe { msg_send![&canvas, hasMarkedText] };
    expect(|| "composition is marked".into(), marked)?;
    // SAFETY: as above.
    unsafe {
        let _: () = msg_send![&canvas, setMarkedText: &*NSString::from_str("日本"), selectedRange: NSRange::new(2, 0), replacementRange: none];
    }
    let sent = s.app.take_dispatched();
    expect(|| format!("composition sends nothing, got {sent:?}"), sent.is_empty())?;
    expect(|| "no commit while composing".into(), s.app.take_text_commits().is_empty())?;
    expect(|| "document unchanged".into(), s.doc(doc).is_some_and(|d| d.revision == revision))?;
    // SAFETY: as above.
    unsafe {
        let _: () = msg_send![&canvas, insertText: &*NSString::from_str("日本"), replacementRange: none];
    }
    // SAFETY: as above.
    let marked: bool = unsafe { msg_send![&canvas, hasMarkedText] };
    expect(|| "commit ends the composition".into(), !marked)?;
    let commits = s.app.take_text_commits();
    expect(|| format!("one final text, got {commits:?}"), commits == ["日本"])?;
    let sent = s.app.take_dispatched();
    let inserts: Vec<_> = sent.iter().filter(|i| matches!(i, UiIntent::InsertText { .. })).collect();
    expect(
        || format!("one InsertText for the commit, got {sent:?}"),
        inserts == [&UiIntent::InsertText { document: doc, text: "日本".into() }],
    )?;
    expect(|| "the commit is one document change".into(), s.doc(doc).is_some_and(|d| d.revision != revision))?;
    let revision = s.doc(doc).ok_or("no document")?.revision;

    // A real key press takes the same route (interpretKeyEvents → insertText:).
    s.key(doc, "a", 0)?;
    let commits = s.app.take_text_commits();
    expect(|| format!("typed 'a' once, got {commits:?}"), commits == ["a"])?;
    // Esc (no composition) still reaches presentation as a canvas key.
    s.key(doc, "\u{1b}", 53)?;
    let sent = s.app.take_dispatched();
    expect(
        || format!("Esc as canvas key, got {sent:?}"),
        sent.iter().any(|i| matches!(i, UiIntent::Key { key: CanvasKey::Escape, .. })),
    )?;
    // Esc itself is no text: the only change after the commit is the typed "a" (one more command).
    let texts = s.app.text_contents(doc);
    expect(|| format!("text objects {texts:?}"), texts.concat() == "日本a")?;
    expect(|| "typing changed the document once more".into(), s.doc(doc).is_some_and(|d| d.revision != revision))?;
    Ok(())
}
