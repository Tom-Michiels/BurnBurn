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
        part_name: s.part_name.clone().unwrap_or_default(),
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

/// Longest explicit part name, in characters (the project file refuses more).
const MAX_PART_NAME_CHARS: usize = 200;

/// Sets the explicit part name: trimmed, blank clears it, a name beyond the limit is refused and the
/// name stays as it was. It is saved with the project but is no undo step.
pub(super) fn set_part_name(s: &mut Session, document: DocumentId, name: &str) -> AppUpdate {
    let name = name.trim();
    if name.chars().count() > MAX_PART_NAME_CHARS {
        return AppUpdate::redraw(document);
    }
    s.part_name = (!name.is_empty()).then(|| name.to_owned());
    AppUpdate::redraw(document)
}

pub(super) fn close(s: &mut Session, document: DocumentId) -> AppUpdate {
    s.notes_open = false;
    AppUpdate::redraw(document)
}
