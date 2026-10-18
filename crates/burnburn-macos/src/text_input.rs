//! IME composition for canvas text input, without AppKit. `NSTextInputClient` on the canvas view
//! forwards `setMarkedText:`, `unmarkText` and `insertText:` here.
//!
//! Composition (marked text) is host state only: it is drawn by the input method's candidate
//! window and never reaches presentation or the document. Only final text leaves the composer,
//! once, and becomes at most one intent/command.

/// The canvas's current composition.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Composer {
    marked: String,
    /// `(location, length)` in chars within `marked`.
    selected: (usize, usize),
}

/// Final text without control characters (Return/Tab/Delete arrive as commands, not text).
fn clean(text: &str) -> Option<String> {
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    (!text.is_empty()).then_some(text)
}

impl Composer {
    /// `setMarkedText:selectedRange:` — replaces the composition; empty text cancels it.
    pub fn set_marked(&mut self, text: &str, selected: (usize, usize)) {
        self.marked = text.to_owned();
        let len = self.marked.chars().count();
        let location = selected.0.min(len);
        self.selected = (location, selected.1.min(len - location));
    }

    /// `unmarkText` — the input method accepts the composition as final text.
    pub fn unmark(&mut self) -> Option<String> {
        let text = std::mem::take(&mut self.marked);
        self.selected = (0, 0);
        clean(&text)
    }

    /// `insertText:replacementRange:` — final text; it replaces any composition.
    pub fn insert(&mut self, text: &str) -> Option<String> {
        self.marked.clear();
        self.selected = (0, 0);
        clean(text)
    }

    pub fn has_marked(&self) -> bool {
        !self.marked.is_empty()
    }

    pub fn marked_text(&self) -> &str {
        &self.marked
    }

    pub fn selected_range(&self) -> (usize, usize) {
        self.selected
    }

    /// Length of the composition in UTF-16 code units (AppKit's `NSRange` unit).
    pub fn marked_utf16_len(&self) -> usize {
        self.marked.encode_utf16().count()
    }
}
