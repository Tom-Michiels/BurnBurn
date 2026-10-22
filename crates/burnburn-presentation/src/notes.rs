//! Project notes panel (P04): the view model. The text lives in the document's process layer.

/// The open notes panel: the stored text, which the host shows in a multi-line field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesViewModel {
    pub text: String,
    /// Longest text the document accepts, in characters.
    pub max_chars: usize,
}
