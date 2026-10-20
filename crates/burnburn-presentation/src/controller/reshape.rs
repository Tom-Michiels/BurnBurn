//! Warp Selection and Deform Selection (W3-UI-schikken): session handling around the editor's
//! reshape tools. The handles live in the editor tool; this module starts and ends the session,
//! answers Apply/Reset and says what the window shows.

use burnburn_editor::{ReshapeMode, ToolContext, ToolKind, ToolParam, ToolPreview, ToolStep};
use burnburn_geometry::{WarpError, WarpKind};
use burnburn_types::{DocumentId, RequestId};

use crate::intent::CommandKey;
use crate::schikken::{ReshapeProblem, ReshapeViewModel};
use crate::view_model::MessageKey;

use super::canvas::{clear_tool_report, handle_step};
use super::{AppUpdate, Session, error};

pub(super) fn is_reshape_tool(tool: ToolKind) -> bool {
    matches!(tool, ToolKind::Warp | ToolKind::Deform)
}

/// An editor call that needs the tool context.
fn with_tool<R>(s: &mut Session, f: impl FnOnce(&mut burnburn_editor::Editor, &ToolContext<'_>) -> R) -> R {
    let snapshot = s.doc.snapshot();
    let ctx = ToolContext {
        document: &snapshot,
        selection: &s.selection,
        mm_per_point: s.camera.mm_per_point(),
        snap: &s.snap,
    };
    f(&mut s.editor, &ctx)
}

/// Starts the session when `tool` is a reshape tool; `None` for every other tool. A selection the
/// tool cannot use (nothing, a line, locked objects) leaves the Select tool active and says why.
pub(super) fn activate(
    s: &mut Session,
    document: DocumentId,
    tool: ToolKind,
    request: Option<RequestId>,
) -> Option<AppUpdate> {
    if !is_reshape_tool(tool) {
        return None;
    }
    let Some(request) = request else { return Some(error(Some(document), MessageKey::CommandRejected)) };
    let snapshot = s.doc.snapshot();
    if s.selection.ids().iter().any(|id| snapshot.is_locked(*id)) {
        s.editor.select_tool(ToolKind::Select);
        return Some(error(Some(document), MessageKey::ObjectsLocked));
    }
    clear_tool_report(s);
    let step = with_tool(s, |editor, ctx| editor.begin_tool(ctx));
    if matches!(step, ToolStep::Rejected(_)) {
        s.editor.select_tool(ToolKind::Select);
    }
    let mut update = handle_step(s, document, step, request);
    update.view_changed = true;
    update.scenes_changed.push(document);
    Some(update)
}

/// Back to the Select tool after the session ended (applied or dropped).
pub(super) fn leave(s: &mut Session) {
    s.editor.select_tool(ToolKind::Select);
}

/// Warp/Deform commands; `None` when `command` is not one of them.
pub(super) fn command(
    s: &mut Session,
    document: DocumentId,
    command: CommandKey,
    request: Option<RequestId>,
) -> Option<AppUpdate> {
    Some(match command {
        CommandKey::ToolWarp | CommandKey::ToolDeform => {
            let tool = if command == CommandKey::ToolWarp { ToolKind::Warp } else { ToolKind::Deform };
            s.trim_mode = false;
            s.editor.select_tool(tool);
            activate(s, document, tool, request)?
        }
        CommandKey::ReshapeApply => {
            if command_enabled(command, s) != Some(true) {
                return Some(AppUpdate::default());
            }
            let Some(request) = request else { return Some(error(Some(document), MessageKey::CommandRejected)) };
            let step = s.editor.reshape_apply();
            handle_step(s, document, step, request)
        }
        CommandKey::ReshapeReset => {
            let step = with_tool(s, |editor, ctx| editor.reshape_reset(ctx));
            let _ = step;
            AppUpdate::redraw(document)
        }
        _ => return None,
    })
}

pub(super) fn command_enabled(command: CommandKey, s: &Session) -> Option<bool> {
    let preview = match s.editor.preview() {
        Some(ToolPreview::Reshape(p)) => Some(p),
        _ => None,
    };
    Some(match command {
        CommandKey::ToolWarp | CommandKey::ToolDeform => !s.selection.is_empty(),
        CommandKey::ReshapeApply => preview.is_some_and(|p| p.modified && p.error.is_none()),
        CommandKey::ReshapeReset => preview.is_some_and(|p| p.modified),
        _ => return None,
    })
}

pub(super) fn set_warp_kind(s: &mut Session, document: DocumentId, bilinear: bool) -> AppUpdate {
    if s.editor.tool() != ToolKind::Warp || !s.editor.set_param(ToolParam::Bilinear(bilinear)) {
        return AppUpdate::default();
    }
    // The outlines of the preview belong to the old kind.
    let _ = with_tool(s, |editor, ctx| editor.begin_tool(ctx));
    AppUpdate::redraw(document)
}

fn problem(e: WarpError) -> ReshapeProblem {
    match e {
        WarpError::NotConvex => ReshapeProblem::NotConvex,
        WarpError::DegenerateQuad | WarpError::DegenerateSource => ReshapeProblem::Degenerate,
        WarpError::Folded => ReshapeProblem::Folded,
        _ => ReshapeProblem::Other,
    }
}

pub(super) fn view(s: &Session) -> Option<ReshapeViewModel> {
    match s.editor.preview() {
        Some(ToolPreview::Reshape(p)) => Some(ReshapeViewModel {
            bilinear: p.mode == ReshapeMode::Warp && p.kind == WarpKind::Bilinear,
            modified: p.modified,
            problem: p.error.map(problem),
            simplified: p.simplified,
        }),
        _ => None,
    }
}

/// The Tangent Circle tool's picks and solutions.
pub(super) fn tangent_view(s: &Session) -> Option<crate::schikken::TangentViewModel> {
    use burnburn_geometry::TangentError;

    use crate::schikken::{TangentProblem, TangentViewModel};
    match s.editor.preview() {
        Some(ToolPreview::Tangent(p)) => Some(TangentViewModel {
            picked: p.picked,
            candidates: p.candidates.len(),
            chosen: p.chosen.is_some(),
            radius_mm: p.radius_mm,
            problem: p.error.map(|e| match e {
                TangentError::Coincident => TangentProblem::Coincident,
                TangentError::Degenerate => TangentProblem::Degenerate,
                TangentError::NoSolution => TangentProblem::NoSolution,
                _ => TangentProblem::Other,
            }),
        }),
        _ => None,
    }
}
