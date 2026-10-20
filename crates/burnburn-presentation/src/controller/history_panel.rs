//! Undo history panel (X02): the document's undo and redo stacks as one list of labelled steps.
//! A click on a step is a jump: a series of ordinary undo or redo commands, nothing else, so the
//! history, dirty state and revision checks behave exactly as with Cmd+Z.

use burnburn_document::UndoLabel;
use burnburn_types::DocumentId;

use super::{AppController, AppUpdate, after_change};

/// One step of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryRow {
    pub label: UndoLabel,
    /// Applied now (undo would revert it); `false`: undone, redo would apply it.
    pub applied: bool,
}

/// The list for one document, oldest step first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HistoryViewModel {
    pub rows: Vec<HistoryRow>,
    /// Number of applied steps: the position between the last applied and the first undone row.
    /// Position 0 is the state the document had when it was opened.
    pub position: usize,
}

impl AppController {
    /// The undo history of `document`; `None` for a window that does not exist.
    pub fn history_view(&self, document: DocumentId) -> Option<HistoryViewModel> {
        let s = self.sessions.get(&document)?;
        let applied = s.doc.undo_labels();
        let position = applied.len();
        let rows = applied
            .into_iter()
            .map(|label| HistoryRow { label, applied: true })
            .chain(s.doc.redo_labels().into_iter().map(|label| HistoryRow { label, applied: false }))
            .collect();
        Some(HistoryViewModel { rows, position })
    }

    /// Goes to the state after `position` applied steps by undoing or redoing one step at a time.
    /// A position outside the list, or the current one, changes nothing. A step that fails stops
    /// the series where it is; the history then shows the position that was reached.
    pub(super) fn history_jump(&mut self, document: DocumentId, position: usize) -> AppUpdate {
        self.with_session(document, |s| {
            let (undo, redo) = (s.doc.undo_labels().len(), s.doc.redo_labels().len());
            if position > undo + redo || position == undo {
                return AppUpdate::default();
            }
            // A gesture in progress is cancelled first; it never reaches history (A4).
            s.editor.cancel();
            let mut update = AppUpdate::redraw(document);
            for _ in 0..position.abs_diff(undo) {
                let expected = s.doc.revision();
                let result = if position < undo { s.doc.undo(expected) } else { s.doc.redo(expected) };
                let failed = result.is_err();
                let step = after_change(s, document, result);
                update.effects.extend(step.effects);
                if failed {
                    break;
                }
            }
            update
        })
    }
}
