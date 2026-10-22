//! Offset preview and options (E05). The preview is a revision-checked task on the selection: it
//! never changes the document, a newer request supersedes the older one, and a result for an older
//! revision is never shown. The options (corner shape, keep sources) are window state that the
//! apply command uses.

use std::sync::Arc;

use burnburn_geometry::{OffsetJoin, Path};
use burnburn_types::{CancellationSource, DocumentId, Length, RevisionId, TaskId};

use crate::field::{format_mm, parse_position};
use crate::offset::{OffsetOptions, OffsetPreviewViewModel, OffsetViewModel};
use crate::task::{TaskInput, TaskKind, TaskRequest, revision_key};
use crate::view_model::{MessageKey, ProblemViewModel};

use super::geometry::centre_of;
use super::{AppController, AppUpdate, Session, error};

/// A computed preview and what it was made for.
#[derive(Debug, Clone)]
struct Computed {
    revision: RevisionId,
    distance: Length,
    result: Arc<Path>,
}

#[derive(Debug, Default)]
pub(super) struct OffsetState {
    pub(super) options: OffsetOptions,
    /// The distance of the open preview as typed, so a changed option can recompute it.
    open: Option<String>,
    computed: Option<Computed>,
}

pub(super) fn view(s: &Session) -> OffsetViewModel {
    let preview = s.offset.computed.as_ref().filter(|c| c.revision == s.doc.revision()).map(|c| {
        let bounds = c.result.bounds();
        OffsetPreviewViewModel {
            distance: format_mm(c.distance.mm()),
            width: bounds.map_or_else(String::new, |b| format_mm(b.width())),
            height: bounds.map_or_else(String::new, |b| format_mm(b.height())),
            area_mm2: c.result.signed_area().abs(),
        }
    });
    OffsetViewModel { options: s.offset.options, preview }
}

/// The preview that belongs to the current revision, for the scene.
pub(super) fn current_result(s: &Session) -> Option<Arc<Path>> {
    s.offset.computed.as_ref().filter(|c| c.revision == s.doc.revision()).map(|c| Arc::clone(&c.result))
}

/// A result arrived from the worker; it is kept only for the revision it was made for.
pub(super) fn arrived(s: &mut Session, source: RevisionId, distance: Length, _join: OffsetJoin, result: Path) {
    if source == s.doc.revision() {
        s.offset.computed = Some(Computed { revision: source, distance, result: Arc::new(result) });
    }
}

/// The worker could not make the offset: say where, like applying it would.
pub(super) fn failed(s: &mut Session, source: RevisionId) {
    if source != s.doc.revision() {
        return;
    }
    s.offset.computed = None;
    let first = s.selection.ids().first().copied();
    let at = first.map_or_else(|| burnburn_geometry::Point::new(0.0, 0.0), |id| centre_of(&s.doc.snapshot(), id));
    s.problem = Some(ProblemViewModel { message: MessageKey::OffsetImpossible, object: first, at });
}

impl AppController {
    pub(super) fn preview_offset(&mut self, document: DocumentId, text: String) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| submit(s, &tasks, document, id, text))
    }

    pub(super) fn set_offset_options(&mut self, document: DocumentId, options: OffsetOptions) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            if s.offset.options == options {
                return AppUpdate::default();
            }
            s.offset.options = options;
            match s.offset.open.clone() {
                Some(text) => submit(s, &tasks, document, id, text),
                None => AppUpdate { view_changed: true, ..AppUpdate::default() },
            }
        })
    }
}

fn submit(
    s: &mut Session,
    tasks: &Arc<dyn crate::task::TaskPort>,
    document: DocumentId,
    id: TaskId,
    text: String,
) -> AppUpdate {
    let targets = s.selection.ids().to_vec();
    let snapshot = s.doc.snapshot();
    let Some(first) = targets.first().copied() else {
        s.offset.open = None;
        s.offset.computed = None;
        return AppUpdate { view_changed: true, ..AppUpdate::default() };
    };
    let distance = match parse_position(&text) {
        Ok(d) if d.mm() != 0.0 => d,
        Ok(_) => {
            s.offset.open = None;
            s.offset.computed = None;
            return AppUpdate { view_changed: true, ..AppUpdate::default() };
        }
        Err(e) => {
            let at = centre_of(&snapshot, first);
            s.problem = Some(ProblemViewModel { message: MessageKey::InvalidLength(e), object: None, at });
            return AppUpdate { view_changed: true, ..AppUpdate::default() };
        }
    };
    let source = snapshot.revision();
    let cancel = CancellationSource::new();
    let request = TaskRequest {
        id,
        document,
        source,
        key: revision_key(TaskKind::OffsetPreview, document, source),
        cancel: cancel.token(),
        input: TaskInput::OffsetPreview { snapshot, targets, distance, join: s.offset.options.join },
    };
    match tasks.submit(request) {
        Ok(()) => {
            for (kind, older) in s.pending.values() {
                if *kind == TaskKind::OffsetPreview {
                    older.cancel();
                }
            }
            s.pending.insert(id, (TaskKind::OffsetPreview, cancel));
            s.offset.open = Some(text);
            s.problem = None;
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        }
        Err(_) => error(Some(document), MessageKey::CommandRejected),
    }
}
