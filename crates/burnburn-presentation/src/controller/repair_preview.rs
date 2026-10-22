//! Repair preview (E09): the before/after of closing the gaps of one problem. A revision-checked
//! task on a scratch copy; the repaired outlines are a scene overlay next to the problem marks
//! ("before"), and the panel says how many gaps close and how many stay open at the typed
//! tolerance. The document does not change; applying is `UiIntent::RepairIssue`.

use std::sync::Arc;

use burnburn_geometry::{DiagnosticTolerance, Path};
use burnburn_types::{CancellationSource, DocumentId, RevisionId, TaskId};

use crate::field::parse_length;
use crate::offset::RepairPreviewViewModel;
use crate::task::{TaskInput, TaskKind, TaskRequest, revision_key};
use crate::view_model::{MessageKey, ProblemViewModel};

use super::{AppController, AppUpdate, Session, error};

#[derive(Debug, Clone)]
struct Computed {
    revision: RevisionId,
    issue: usize,
    closes: usize,
    stays_open: usize,
    result: Arc<Path>,
}

#[derive(Debug, Default)]
pub(super) struct RepairState {
    /// The problem the open preview was asked for.
    asked: Option<usize>,
    computed: Option<Computed>,
}

pub(super) fn view(s: &Session) -> Option<RepairPreviewViewModel> {
    let c = s.repair.computed.as_ref().filter(|c| c.revision == s.doc.revision())?;
    Some(RepairPreviewViewModel { issue: c.issue, closes: c.closes, stays_open: c.stays_open })
}

pub(super) fn current_result(s: &Session) -> Option<Arc<Path>> {
    s.repair.computed.as_ref().filter(|c| c.revision == s.doc.revision()).map(|c| Arc::clone(&c.result))
}

/// A result arrived; it is kept only for the revision it was made for.
pub(super) fn arrived(s: &mut Session, source: RevisionId, result: Path, closes: usize, stays_open: usize) {
    if source != s.doc.revision() {
        return;
    }
    if let Some(issue) = s.repair.asked {
        s.repair.computed = Some(Computed { revision: source, issue, closes, stays_open, result: Arc::new(result) });
    }
}

pub(super) fn failed(s: &mut Session, source: RevisionId) {
    if source == s.doc.revision() {
        s.repair.computed = None;
    }
}

impl AppController {
    pub(super) fn preview_repair(&mut self, document: DocumentId, issue: usize, text: String) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let Some(chosen) = s.issues.get(issue).cloned() else { return AppUpdate::default() };
            let tolerance = match parse_length(&text) {
                Ok(t) => t,
                Err(e) => {
                    s.problem = Some(ProblemViewModel {
                        message: MessageKey::InvalidLength(e),
                        object: chosen.objects.first().copied(),
                        at: chosen.at,
                    });
                    return AppUpdate { view_changed: true, ..AppUpdate::default() };
                }
            };
            let Ok(tolerance) = DiagnosticTolerance::new(tolerance) else {
                return error(Some(document), MessageKey::CommandRejected);
            };
            let snapshot = s.doc.snapshot();
            let source = snapshot.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::RepairPreview, document, source),
                cancel: cancel.token(),
                input: TaskInput::RepairPreview { snapshot, targets: chosen.objects.clone(), tolerance },
            };
            match tasks.submit(request) {
                Ok(()) => {
                    for (kind, older) in s.pending.values() {
                        if *kind == TaskKind::RepairPreview {
                            older.cancel();
                        }
                    }
                    s.pending.insert(id, (TaskKind::RepairPreview, cancel));
                    s.repair.asked = Some(issue);
                    s.problem = None;
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                Err(_) => error(Some(document), MessageKey::CommandRejected),
            }
        })
    }
}
