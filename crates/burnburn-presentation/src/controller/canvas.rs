//! Canvas input: pointer, key and camera events, and the tool step they produce.

use crate::field::format_mm;
use crate::view_model::{StatusMessage, ToolMeasurement, ToolRejection};
use burnburn_document::DocumentCommand;
use burnburn_editor::{
    CLICK_TOLERANCE_PT, CanvasInput, CanvasKey, EditCommit, KeyPhase, Modifiers, PointerEvent, PointerPhase,
    ToolContext, ToolKind, ToolStep, hit_test,
};
use burnburn_editor::{Rejection, Unavailable};
use burnburn_types::{DocumentId, RequestId};

use crate::commit::{Refusal, name_kind, payload};

use crate::intent::{CameraIntent, CommandKey, ViewPointerEvent};
use crate::view_model::{MessageKey, SelectionCycle};

use super::geometry::trim_at;
use super::shapes;
use super::sketch;
use super::{AppController, AppUpdate, Session, after_change, apply_command, apply_selection, error};

impl AppController {
    pub(super) fn pointer(&mut self, document: DocumentId, event: ViewPointerEvent) -> AppUpdate {
        if let Some(update) = self.laser_pointer(document, &event) {
            return update;
        }
        if let Some(update) = self.trace_pointer(document, &event) {
            return update;
        }
        if let Some(update) = self.recognition_pointer(document, &event) {
            return update;
        }
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            // DR-04: the one place where canvas points become world millimetres.
            let position = s.camera.points_to_world(&s.viewport, event.position_pt);
            let ev = PointerEvent {
                pointer: event.pointer,
                phase: event.phase,
                button: event.button,
                position,
                modifiers: event.modifiers,
                click_count: event.click_count,
                time: event.time,
            };
            let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
            // A new pointer position starts the Tab list over (E04); a finished gesture does too.
            if s.last_pointer.is_none_or(|last| last.position != ev.position) {
                s.snap.cycle = 0;
            }
            s.last_pointer = Some(ev);
            if let Some(update) = super::registration::pick(s, document, position, &event) {
                return update;
            }
            if s.trim_mode {
                return trim_at(s, document, &ev, request);
            }
            if let Some(update) = sketch::pointer(s, document, &ev, request) {
                return update;
            }
            let alt_press = ev.phase == PointerPhase::Down && ev.modifiers.alt && s.editor.tool() == ToolKind::Select;
            if ev.phase == PointerPhase::Down && !alt_press {
                s.cycle = None; // any other press ends the "2 van 3" display
            }
            let update = run_tool(s, document, &CanvasInput::Pointer(ev), request);
            if ev.phase == PointerPhase::Up {
                s.snap.cycle = 0;
            }
            if alt_press {
                // "2 van 3": where the Alt-click cycle stands among the candidates under the pointer.
                let hits = hit_test(&s.doc.snapshot(), position, s.camera.mm_per_point() * CLICK_TOLERANCE_PT);
                s.cycle = s.selection.single().and_then(|sel| {
                    hits.iter().position(|h| *h == sel).map(|i| SelectionCycle { position: i + 1, count: hits.len() })
                });
            }
            update
        })
    }

    pub(super) fn key(&mut self, document: DocumentId, key: CanvasKey, modifiers: Modifiers) -> AppUpdate {
        let drawing_points = self.sessions.get(&document).is_some_and(|s| {
            s.editor.is_gesture_active() && matches!(s.editor.tool(), ToolKind::Polyline | ToolKind::Pen)
        });
        // In node-edit mode with nodes selected, Delete is for the tool: it removes those nodes.
        let nodes_selected = self
            .sessions
            .get(&document)
            .is_some_and(|s| s.editor.tool() == ToolKind::Node && !s.editor.node_selection().is_empty());
        let key = match key {
            // Delete removes the last placed point while drawing a polyline or pen path.
            CanvasKey::Delete if drawing_points => CanvasKey::Backspace,
            CanvasKey::Delete if nodes_selected => CanvasKey::Delete,
            CanvasKey::Delete => return self.invoke(Some(document), CommandKey::Delete),
            other => other,
        };
        if key == CanvasKey::Tab {
            let request = self.next_id::<RequestId>();
            return self.with_session(document, |s| {
                let (Some(last), Some(request)) = (s.last_pointer, request) else { return AppUpdate::default() };
                s.snap.cycle = s.snap.cycle.wrapping_add(1);
                // The same position again, as a hover: the tool shows the next candidate.
                let hover = PointerEvent { phase: PointerPhase::Move, ..last };
                run_tool(s, document, &CanvasInput::Pointer(hover), request)
            });
        }
        if key == CanvasKey::Escape
            && let Some(s) = self.sessions.get_mut(&document)
            && s.trim_mode
        {
            s.trim_mode = false;
            return AppUpdate::redraw(document);
        }
        if key == CanvasKey::Escape
            && let Some(s) = self.sessions.get_mut(&document)
            && s.sketch.active.is_some()
            && s.editor.tool() == ToolKind::Select
            && !(s.sketch.entities.is_empty() && s.sketch.constraints.is_empty())
        {
            s.sketch.entities.clear();
            s.sketch.constraints.clear();
            return AppUpdate::redraw(document);
        }
        // Escape puts the pointing at the box of a recognition template down.
        if key == CanvasKey::Escape && self.sessions.contains_key(&document) && self.end_recognition_point() {
            return self.workspace_update();
        }
        // Escape puts the brush or the corner pointing of the tracer down.
        if key == CanvasKey::Escape
            && let Some(s) = self.sessions.get_mut(&document)
            && super::trace_g7::end_pick(s)
        {
            return AppUpdate::redraw(document);
        }
        // Escape first ends a Print and Cut / Center Finder pick.
        if key == CanvasKey::Escape
            && let Some(s) = self.sessions.get_mut(&document)
            && super::registration::end_pick(s)
        {
            return AppUpdate::redraw(document);
        }
        // Escape ends a Warp/Deform session whatever the handles show: the tool is done.
        if key == CanvasKey::Escape
            && let Some(s) = self.sessions.get_mut(&document)
            && super::reshape::is_reshape_tool(s.editor.tool())
        {
            s.editor.cancel();
            super::reshape::leave(s);
            return AppUpdate::redraw(document);
        }
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| match request {
            Some(request) => {
                run_tool(s, document, &CanvasInput::Key { key, modifiers, phase: KeyPhase::Down }, request)
            }
            None => error(Some(document), MessageKey::CommandRejected),
        })
    }

    pub(super) fn camera(&mut self, document: DocumentId, intent: CameraIntent) -> AppUpdate {
        self.with_session(document, |s| match intent {
            CameraIntent::Resized { viewport } => {
                s.viewport = viewport;
                AppUpdate::redraw(document)
            }
            // Camera pan/zoom is window state: it never becomes a document command.
            CameraIntent::Scroll { delta_pt } => {
                s.camera.pan_by_points(delta_pt);
                AppUpdate::redraw(document)
            }
            CameraIntent::Magnify { magnification, anchor_pt } => {
                match s.camera.zoom_at(&s.viewport, anchor_pt, 1.0 + magnification) {
                    Ok(()) => AppUpdate::redraw(document),
                    Err(_) => AppUpdate::default(),
                }
            }
        })
    }
}

/// Builds the tool context for one input and processes the resulting step.
pub(super) fn run_tool(s: &mut Session, document: DocumentId, input: &CanvasInput, request: RequestId) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let ctx = ToolContext {
        document: &snapshot,
        selection: &s.selection,
        mm_per_point: s.camera.mm_per_point(),
        snap: &s.snap,
    };
    let step = s.editor.input(input, &ctx);
    handle_step(s, document, step, request)
}

pub(super) fn clear_tool_report(s: &mut Session) {
    s.tool_measurement = None;
    s.messages.retain(|m| !matches!(m.key, MessageKey::ToolRejected(_)));
}

fn rejection_kind(r: &Rejection) -> ToolRejection {
    match r {
        Rejection::NothingPicked => ToolRejection::NothingPicked,
        Rejection::DerivedGeometry(_) => ToolRejection::DerivedGeometry,
        Rejection::NeedsSelection => ToolRejection::NeedsSelection,
        Rejection::InvalidSelection => ToolRejection::InvalidSelection,
        Rejection::NothingToConstrain => ToolRejection::NothingToConstrain,
        Rejection::AmbiguousRelations => ToolRejection::AmbiguousRelations,
        Rejection::InvalidValue => ToolRejection::InvalidValue,
        Rejection::Corner(_) => ToolRejection::Corner,
        Rejection::Trim(_) => ToolRejection::Trim,
        Rejection::Extend(_) => ToolRejection::Extend,
        Rejection::Break(_) => ToolRejection::Break,
        Rejection::Offset(_) => ToolRejection::Offset,
        Rejection::Unavailable(Unavailable::ConstructionOutsideSketch) => ToolRejection::ConstructionOutsideSketch,
        Rejection::Unavailable(Unavailable::CurvedCorner) => ToolRejection::CurvedCorner,
        Rejection::Unavailable(Unavailable::BezierExtend) => ToolRejection::BezierExtend,
        _ => ToolRejection::Unavailable,
    }
}

/// A commit without a document command: the user is told why, the document is unchanged.
fn refused(s: &mut Session, document: DocumentId, refusal: Refusal) -> AppUpdate {
    let reason = match refusal {
        Refusal::SingleObjectOnly => ToolRejection::SingleObjectOnly,
        Refusal::Unsupported => ToolRejection::Unavailable,
    };
    s.messages.clear();
    s.messages.push(StatusMessage { key: MessageKey::ToolRejected(reason) });
    AppUpdate::redraw(document)
}

pub(super) fn handle_step(s: &mut Session, document: DocumentId, step: ToolStep, request: RequestId) -> AppUpdate {
    match step {
        ToolStep::Nothing => AppUpdate::default(),
        ToolStep::Select(change) => {
            let change =
                if s.editor.tool() == ToolKind::Select { super::arrange::whole_groups(s, change) } else { change };
            apply_selection(s, &change);
            s.cycle = None;
            AppUpdate::redraw(document)
        }
        ToolStep::Commit(
            edit @ (EditCommit::CreateRectangle { .. }
            | EditCommit::CreateShape { .. }
            | EditCommit::CreatePath { .. }
            | EditCommit::CreateParametric { .. }),
        ) if s.sketch.active.is_some() => sketch::commit_shape(s, document, &edit, request),
        ToolStep::Rejected(reason) => {
            s.messages.clear();
            s.messages.push(StatusMessage { key: MessageKey::ToolRejected(rejection_kind(&reason)) });
            AppUpdate::redraw(document)
        }
        ToolStep::Measured(m) => {
            s.tool_measurement = Some(ToolMeasurement {
                distance_text: format_mm(m.distance()),
                dx_text: format_mm(m.dx()),
                dy_text: format_mm(m.dy()),
            });
            AppUpdate::redraw(document)
        }
        ToolStep::Commit(EditCommit::PlaceText { at }) => {
            // The anchor of the next committed text; no document change by itself.
            s.text_anchor = Some(at);
            AppUpdate::redraw(document)
        }
        ToolStep::Commit(edit) => {
            s.tool_measurement = None;
            let reshaped = matches!(edit, EditCommit::Reshape(_));
            let kind = name_kind(&edit);
            let name = kind.map(|k| s.next_name(k)).unwrap_or_default();
            let snapshot = s.doc.snapshot();
            let payload = match payload(edit, name, &snapshot) {
                Ok(payload) => payload,
                Err(refusal) => return refused(s, document, refusal),
            };
            let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
            match apply_command(&s.fonts, &mut s.doc, cmd) {
                Ok(outcome) => {
                    if let Some(k) = kind {
                        *s.created.entry(k).or_insert(0) += 1;
                    }
                    if reshaped {
                        super::reshape::leave(s);
                    }
                    after_change(s, document, Ok(outcome))
                }
                // "Nothing to change" is said in words; a real refusal stays a rejected command.
                Err(e) if shapes::is_nothing_to_do(&e) => shapes::failed_commit(s, document, e),
                Err(e) => after_change(s, document, Err(e)),
            }
        }
        _ => AppUpdate::redraw(document),
    }
}
