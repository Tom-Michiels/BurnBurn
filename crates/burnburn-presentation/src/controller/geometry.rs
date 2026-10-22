//! Geometry operations run from panels: measure, boolean preview, offset, repair and trim.

use std::sync::Arc;

use burnburn_document::{CommandError, CommandPayload, DocumentCommand, DocumentSession, EditError, GeometryCommand};
use burnburn_editor::{CLICK_TOLERANCE_PT, PointerEvent, PointerPhase, boolean_operands, hit_test};
use burnburn_geometry::{BooleanOp, DiagnosticTolerance, Point, ProductionTolerance};
use burnburn_types::{CancellationSource, DocumentId, GeometryId, Length, RequestId, TaskId};

use crate::field::{parse_length, parse_position};
use crate::task::{TaskInput, TaskKind, TaskRequest, revision_key};
use crate::view_model::{BooleanPreview, MessageKey, ProblemViewModel};

use super::{AppController, AppUpdate, Session, after_change, apply_command, error};

impl AppController {
    pub(super) fn submit_measure(&mut self, document: DocumentId) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let source = s.doc.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::Measure, document, source),
                cancel: cancel.token(),
                input: TaskInput::Measure(s.doc.snapshot()),
            };
            match tasks.submit(request) {
                Ok(()) => {
                    s.pending.insert(id, (TaskKind::Measure, cancel));
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                Err(_) => error(Some(document), MessageKey::CommandRejected),
            }
        })
    }
}

impl AppController {
    /// Live boolean preview (E05): a revision-checked task; a newer request supersedes the older.
    pub(super) fn submit_boolean(&mut self, document: DocumentId, op: BooleanOp) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let snapshot = s.doc.snapshot();
            let Ok(operands) = boolean_operands(&snapshot, &s.selection) else { return AppUpdate::default() };
            let source = snapshot.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::Boolean, document, source),
                cancel: cancel.token(),
                input: TaskInput::Boolean { snapshot, op, operands },
            };
            match tasks.submit(request) {
                Ok(()) => {
                    for (kind, older) in s.pending.values() {
                        if *kind == TaskKind::Boolean {
                            older.cancel();
                        }
                    }
                    s.pending.insert(id, (TaskKind::Boolean, cancel));
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                Err(_) => error(Some(document), MessageKey::CommandRejected),
            }
        })
    }
}

pub(super) fn current_boolean_preview(s: &Session) -> Option<&BooleanPreview> {
    s.boolean_preview.as_ref().filter(|b| b.revision == s.doc.revision())
}

/// Chord tolerance for offsets applied from the panel (production geometry).
const OFFSET_TOLERANCE_MM: f64 = 0.01;

pub(super) fn located(
    s: &mut Session,
    document: DocumentId,
    message: MessageKey,
    object: Option<GeometryId>,
    at: Point,
) -> AppUpdate {
    s.problem = Some(ProblemViewModel { message, object, at });
    let mut update = error(Some(document), message);
    update.view_changed = true;
    update.scenes_changed.push(document);
    update
}

pub(super) fn centre_of(snapshot: &burnburn_document::DocumentSnapshot, id: GeometryId) -> Point {
    snapshot
        .object(id)
        .and_then(|o| o.bounds())
        .map_or(Point::new(0.0, 0.0), |b| Point::new((b.min().x + b.max().x) / 2.0, (b.min().y + b.max().y) / 2.0))
}

/// E05 offset panel: one command for the whole selection (one undo entry, sources kept). When it
/// fails, each target is tried alone on a scratch copy to name the object that cannot be offset.
pub(super) fn offset_selection(
    s: &mut Session,
    document: DocumentId,
    text: &str,
    request: Option<RequestId>,
) -> AppUpdate {
    let targets = s.selection.ids().to_vec();
    let Some(first) = targets.first().copied() else { return AppUpdate::default() };
    let snapshot = s.doc.snapshot();
    let distance = match parse_position(text) {
        Ok(d) if d.mm() != 0.0 => d,
        Ok(_) => return AppUpdate::default(),
        Err(e) => {
            let at = centre_of(&snapshot, first);
            s.problem = Some(ProblemViewModel { message: MessageKey::InvalidLength(e), object: None, at });
            return AppUpdate { view_changed: true, ..AppUpdate::default() };
        }
    };
    let Some(tolerance) = Length::from_mm(OFFSET_TOLERANCE_MM).ok().and_then(|l| ProductionTolerance::new(l).ok())
    else {
        return error(Some(document), MessageKey::CommandRejected);
    };
    let edit = |targets: Vec<GeometryId>| {
        CommandPayload::Edit(GeometryCommand::Offset {
            targets,
            distance,
            join: s.offset.options.join,
            tolerance,
            keep_sources: s.offset.options.keep_sources,
        })
    };
    let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload: edit(targets.clone()) };
    match apply_command(&s.fonts, &mut s.doc, cmd) {
        Err(CommandError::Edit(EditError::Offset(_))) => {
            let culprit = targets.iter().copied().find(|id| {
                let mut scratch = DocumentSession::from_snapshot(document, (*snapshot).clone());
                let probe =
                    DocumentCommand { document, expected: scratch.revision(), request, payload: edit(vec![*id]) };
                apply_command(&s.fonts, &mut scratch, probe).is_err()
            });
            let at = centre_of(&snapshot, culprit.unwrap_or(first));
            located(s, document, MessageKey::OffsetImpossible, culprit, at)
        }
        other => after_change(s, document, other),
    }
}

/// E09: close the gaps of one chosen problem with its own tolerance (one undo entry).
pub(super) fn repair_issue(
    s: &mut Session,
    document: DocumentId,
    issue: usize,
    text: &str,
    request: Option<RequestId>,
) -> AppUpdate {
    let Some(chosen) = s.issues.get(issue).cloned() else { return AppUpdate::default() };
    let tolerance = match parse_length(text) {
        Ok(t) => t,
        Err(e) => return located(s, document, MessageKey::InvalidLength(e), None, chosen.at),
    };
    let Ok(tolerance) = DiagnosticTolerance::new(tolerance) else {
        return error(Some(document), MessageKey::CommandRejected);
    };
    let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
    let payload = CommandPayload::Edit(GeometryCommand::CloseGaps { targets: chosen.objects.clone(), tolerance });
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
    match apply_command(&s.fonts, &mut s.doc, cmd) {
        Err(CommandError::Edit(EditError::NothingChanged)) => {
            s.problem = Some(ProblemViewModel {
                message: MessageKey::NothingToRepair,
                object: chosen.objects.first().copied(),
                at: chosen.at,
            });
            AppUpdate::redraw(document)
        }
        other => after_change(s, document, other),
    }
}

/// Trim mode: a click (release) on a line trims it against every other object.
pub(super) fn trim_at(s: &mut Session, document: DocumentId, ev: &PointerEvent, request: RequestId) -> AppUpdate {
    if ev.phase != PointerPhase::Up {
        return AppUpdate::default();
    }
    let snapshot = s.doc.snapshot();
    let hits = hit_test(&snapshot, ev.position, s.camera.mm_per_point() * CLICK_TOLERANCE_PT);
    let Some(target) = hits.first().copied() else { return AppUpdate::default() };
    let cutters = snapshot.objects().map(|(id, _)| id).filter(|id| *id != target).collect();
    let payload = CommandPayload::Edit(GeometryCommand::TrimLine { target, pick: ev.position, cutters });
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
    match apply_command(&s.fonts, &mut s.doc, cmd) {
        Err(CommandError::Edit(EditError::Trim(_))) => {
            located(s, document, MessageKey::TrimImpossible, Some(target), ev.position)
        }
        other => after_change(s, document, other),
    }
}
