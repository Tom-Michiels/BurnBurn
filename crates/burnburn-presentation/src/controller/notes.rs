//! Bestand > Notities tonen (P04): the project notes panel. The text is document data
//! (`ProcessCommand::SetNotes`); only "is the panel open" is window state.

use super::{AppUpdate, Session, apply_edit};
use crate::notes::NotesViewModel;
use burnburn_document::CommandPayload;
use burnburn_document::process::ProcessCommand;
use burnburn_types::{DocumentId, RequestId};

pub(super) fn view(s: &Session) -> Option<NotesViewModel> {
    s.notes_open.then(|| NotesViewModel {
        text: s.doc.snapshot().process().notes().to_owned(),
        max_chars: ProcessCommand::MAX_NOTES_CHARS,
    })
}

pub(super) fn toggle(s: &mut Session, document: DocumentId) -> AppUpdate {
    s.notes_open = !s.notes_open;
    AppUpdate::redraw(document)
}

/// Stores the text. The same text is no new history entry; a refused text keeps the panel open
/// and the document as it was.
pub(super) fn set(s: &mut Session, document: DocumentId, text: String, request: Option<RequestId>) -> AppUpdate {
    if s.doc.snapshot().process().notes() == text {
        return AppUpdate::redraw(document);
    }
    apply_edit(s, document, CommandPayload::Process(ProcessCommand::SetNotes { text }), request)
}

pub(super) fn close(s: &mut Session, document: DocumentId) -> AppUpdate {
    s.notes_open = false;
    AppUpdate::redraw(document)
}
