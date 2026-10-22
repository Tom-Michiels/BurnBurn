//! Shape operations (W2-L1-tekenen): node commands, the join/round/optimize dialogs, cut,
//! direction, convert to path, and the options and parameters of the basic shapes. Window state
//! only; every change to the document is one command and one history entry, and a command that
//! cannot do anything says so instead of recording an empty entry.

use burnburn_document::{CommandError, CommandPayload, DocumentCommand, EditError, OpsCommand, ShapeCommand};
use burnburn_editor::ModifyParam;
use burnburn_editor::{
    NodeCommand, ShapeDefaults, ToolContext, ToolKind, ToolParam, ToolStep, cut_operands, join_operands,
    path_op_operands,
};
use burnburn_geometry::{
    DiagnosticTolerance, PathOp, PathOpError, ProductionTolerance, ShapeParamError, ShapeParams, Winding,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId};

use crate::catalog::DisabledReason;
use crate::commit::boolean_options;
use crate::field::{FieldError, format_mm, parse_length, parse_number};
use crate::intent::{CommandKey, ParamKey};
use crate::view_model::{
    GeometryDialogKind, GeometryDialogViewModel, MessageKey, ParamRow, ShapePanelViewModel, StatusMessage,
    ToolRejection,
};

use super::canvas::handle_step;
use super::{AppUpdate, Session, after_change, apply_command, error};

/// Join/Close tolerance the dialog starts with (mm), as LightBurn offers.
const DEFAULT_JOIN_TOLERANCE_MM: f64 = 0.05;
/// Error budget of Optimize Shapes the dialog starts with (mm).
const DEFAULT_OPTIMIZE_TOLERANCE_MM: f64 = 0.01;

/// The open geometry dialog of a window.
#[derive(Debug, Clone)]
pub(super) struct GeometryDialog {
    kind: GeometryDialogKind,
    text: String,
    error: Option<MessageKey>,
}

/// Options of the tools that have any, kept across tool switches (per window). The basic-shape
/// options live in the editor's `ShapeDefaults`; these are the ones the editor does not expose.
#[derive(Debug, Clone)]
pub(super) struct ToolOptions {
    columns: u32,
    rows: u32,
    count: u32,
    sweep_deg: f64,
    keep_original: bool,
    /// Fillet radius and the chamfer sizes (G7-editor, E05); the same defaults as the editor's tools.
    fillet_radius: f64,
    chamfer_distance: f64,
    chamfer_distance2: f64,
    chamfer_angle: f64,
    /// The option whose last input was refused, and why (cleared by the next tool choice).
    refused: Option<(ParamKey, MessageKey)>,
    /// The shape parameter whose last input was refused: object, key and reason.
    shape_refused: Option<(GeometryId, ParamKey, MessageKey)>,
}

impl Default for ToolOptions {
    fn default() -> Self {
        ToolOptions {
            columns: 3,
            rows: 3,
            count: 6,
            sweep_deg: 360.0,
            keep_original: false,
            fillet_radius: 2.0,
            chamfer_distance: 2.0,
            chamfer_distance2: 2.0,
            chamfer_angle: 45.0,
            refused: None,
            shape_refused: None,
        }
    }
}

fn node_command(key: CommandKey) -> Option<NodeCommand> {
    Some(match key {
        CommandKey::NodeDelete => NodeCommand::DeleteNodes,
        CommandKey::NodeBreak => NodeCommand::BreakAtNodes,
        CommandKey::NodeSmooth => NodeCommand::MakeSmooth,
        CommandKey::NodeSymmetric => NodeCommand::MakeSymmetric,
        CommandKey::NodeCorner => NodeCommand::MakeCorner,
        CommandKey::NodeToLine => NodeCommand::SegmentsToLines,
        CommandKey::NodeToCurve => NodeCommand::SegmentsToCurves,
        CommandKey::NodeClosePath => NodeCommand::ClosePaths,
        CommandKey::NodeJoinEnds => NodeCommand::JoinEnds,
        CommandKey::NodeStartPoint => NodeCommand::SetStartPoint,
        _ => return None,
    })
}

fn dialog_kind(key: CommandKey) -> Option<GeometryDialogKind> {
    Some(match key {
        CommandKey::ShapesJoin => GeometryDialogKind::Join,
        CommandKey::ShapesRoundCorners => GeometryDialogKind::RoundCorners,
        CommandKey::ShapesOptimize => GeometryDialogKind::Optimize,
        _ => return None,
    })
}

/// The tool context of `s` for the editor's own enablement questions.
fn with_context<R>(s: &Session, f: impl FnOnce(&ToolContext<'_>) -> R) -> R {
    let snapshot = s.doc.snapshot();
    f(&ToolContext {
        document: &snapshot,
        selection: &s.selection,
        mm_per_point: s.camera.mm_per_point(),
        snap: &s.snap,
    })
}

/// Whether `command` can act on the current selection; `None` when it is not a shape command.
pub(super) fn command_enabled(command: CommandKey, s: &Session) -> Option<bool> {
    let snapshot = s.doc.snapshot();
    Some(match command {
        CommandKey::ShapesJoin => join_operands(&snapshot, &s.selection).is_ok(),
        CommandKey::ShapesCut => cut_operands(&snapshot, &s.selection).is_ok(),
        CommandKey::ShapesRoundCorners
        | CommandKey::ShapesOptimize
        | CommandKey::ShapesReverse
        | CommandKey::ShapesClockwise
        | CommandKey::ShapesCounterClockwise => path_op_operands(&snapshot, &s.selection).is_ok(),
        CommandKey::ShapesConvertToPath => {
            super::text::convertible(s)
                || (!s.selection.is_empty()
                    && s.selection
                        .ids()
                        .iter()
                        .all(|id| snapshot.object(*id).is_some_and(|o| o.parametric().is_some())))
        }
        other => {
            let cmd = node_command(other)?;
            with_context(s, |ctx| s.editor.node_command_enabled(cmd, ctx))
        }
    })
}

/// Why `command` cannot act on the current selection, for the toolbar button.
pub(super) fn disabled_reason(command: CommandKey, s: &Session) -> Option<DisabledReason> {
    match command_enabled(command, s)? {
        true => None,
        false if node_command(command).is_some() => Some(DisabledReason::NeedsNodes),
        false => Some(DisabledReason::NeedsSelection),
    }
}

/// Runs `command` if it is a shape or node command; `None` when it is not ours.
pub(super) fn run_command(
    s: &mut Session,
    document: DocumentId,
    command: CommandKey,
    request: Option<RequestId>,
) -> Option<AppUpdate> {
    command_enabled(command, s)?;
    let Some(request) = request else { return Some(error(Some(document), MessageKey::CommandRejected)) };
    if !command_enabled(command, s).unwrap_or(false) {
        return Some(AppUpdate::default());
    }
    if let Some(kind) = dialog_kind(command) {
        let text = match kind {
            GeometryDialogKind::Join => format_mm(DEFAULT_JOIN_TOLERANCE_MM),
            GeometryDialogKind::RoundCorners => format_mm(s.editor.round_radius()),
            GeometryDialogKind::Optimize => format_mm(DEFAULT_OPTIMIZE_TOLERANCE_MM),
        };
        s.geometry_dialog = Some(GeometryDialog { kind, text, error: None });
        return Some(AppUpdate::redraw(document));
    }
    if let Some(cmd) = node_command(command) {
        // The editor needs `&mut`, so the context is built here and not through `with_context`.
        let snapshot = s.doc.snapshot();
        let ctx = ToolContext {
            document: &snapshot,
            selection: &s.selection,
            mm_per_point: s.camera.mm_per_point(),
            snap: &s.snap,
        };
        let step: ToolStep = s.editor.node_command(cmd, &ctx);
        return Some(handle_step(s, document, step, request));
    }
    let snapshot = s.doc.snapshot();
    let ops = match command {
        CommandKey::ShapesCut => {
            let (subjects, cutter) = cut_operands(&snapshot, &s.selection).ok()?;
            let options = boolean_options()?;
            OpsCommand::CutShapes { subjects, cutter, keep_cutter: false, options }
        }
        CommandKey::ShapesReverse => path_op(s, PathOp::Reverse { subpaths: None })?,
        CommandKey::ShapesClockwise => path_op(s, PathOp::SetWinding { winding: Winding::Clockwise })?,
        CommandKey::ShapesCounterClockwise => path_op(s, PathOp::SetWinding { winding: Winding::CounterClockwise })?,
        CommandKey::ShapesConvertToPath if super::text::convertible(s) => {
            return Some(super::text::convert(s, document, Some(request)));
        }
        CommandKey::ShapesConvertToPath => {
            let payload = CommandPayload::Shape(ShapeCommand::ConvertToPath { targets: s.selection.ids().to_vec() });
            return Some(run(s, document, payload, request).unwrap_or_else(|e| failed(s, document, &e)));
        }
        _ => return None,
    };
    let payload = CommandPayload::Edit(burnburn_document::GeometryCommand::Ops(ops));
    Some(run(s, document, payload, request).unwrap_or_else(|e| failed(s, document, &e)))
}

fn path_op(s: &Session, op: PathOp) -> Option<OpsCommand> {
    let targets = path_op_operands(&s.doc.snapshot(), &s.selection).ok()?;
    Some(OpsCommand::PathOp { targets, op })
}

/// Applies one payload. On success the usual aftermath (selection follows, issues recomputed);
/// on failure the error, untouched, so the caller decides how to say it.
fn run(
    s: &mut Session,
    document: DocumentId,
    payload: CommandPayload,
    request: RequestId,
) -> Result<AppUpdate, CommandError> {
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
    let outcome = apply_command(&s.fonts, &mut s.doc, cmd)?;
    Ok(after_change(s, document, Ok(outcome)))
}

/// Whether the document refused because the operation would leave everything as it is.
pub(super) fn is_nothing_to_do(e: &CommandError) -> bool {
    use burnburn_geometry::RoundError;
    matches!(
        e,
        CommandError::Edit(
            EditError::NothingChanged
                | EditError::PathOp(PathOpError::NoChange | PathOpError::Round(RoundError::NothingToRound))
        )
    )
}

/// A refused command outside a dialog: "nothing to do" is a status message, anything else a
/// rejected command.
fn failed(s: &mut Session, document: DocumentId, e: &CommandError) -> AppUpdate {
    if is_nothing_to_do(e) {
        s.messages.clear();
        s.messages.push(StatusMessage { key: MessageKey::NothingToChange });
        return AppUpdate::redraw(document);
    }
    s.messages.push(StatusMessage { key: MessageKey::CommandRejected });
    let mut update = error(Some(document), MessageKey::CommandRejected);
    update.view_changed = true;
    update
}

/// The same for a commit that arrived through a tool (`handle_step`).
pub(super) fn failed_commit(s: &mut Session, document: DocumentId, e: CommandError) -> AppUpdate {
    failed(s, document, &e)
}

// ---------------------------------------------------------------------------------------------
// The geometry dialog
// ---------------------------------------------------------------------------------------------

fn parse_value(text: &str) -> Result<Length, FieldError> {
    parse_length(text)
}

pub(super) fn apply_dialog(s: &mut Session, document: DocumentId, text: &str, request: Option<RequestId>) -> AppUpdate {
    let Some(kind) = s.geometry_dialog.as_ref().map(|d| d.kind) else { return AppUpdate::default() };
    let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
    let refuse = |s: &mut Session, key: MessageKey| {
        s.geometry_dialog = Some(GeometryDialog { kind, text: text.to_owned(), error: Some(key) });
        AppUpdate::redraw(document)
    };
    let value = match parse_value(text) {
        Ok(v) => v,
        Err(e) => return refuse(s, MessageKey::InvalidLength(e)),
    };
    let snapshot = s.doc.snapshot();
    let ops = match kind {
        GeometryDialogKind::Join => {
            let (Ok(targets), Ok(tolerance)) =
                (join_operands(&snapshot, &s.selection), DiagnosticTolerance::new(value))
            else {
                return refuse(s, MessageKey::ToolRejected(ToolRejection::InvalidSelection));
            };
            OpsCommand::JoinShapes { targets, tolerance }
        }
        GeometryDialogKind::RoundCorners => {
            let Ok(targets) = path_op_operands(&snapshot, &s.selection) else {
                return refuse(s, MessageKey::ToolRejected(ToolRejection::InvalidSelection));
            };
            OpsCommand::PathOp { targets, op: PathOp::RoundCorners { radius: value.mm(), nodes: None } }
        }
        GeometryDialogKind::Optimize => {
            let (Ok(targets), Ok(tolerance)) =
                (path_op_operands(&snapshot, &s.selection), ProductionTolerance::new(value))
            else {
                return refuse(s, MessageKey::ToolRejected(ToolRejection::InvalidSelection));
            };
            OpsCommand::PathOp { targets, op: PathOp::Optimize { tolerance } }
        }
    };
    let payload = CommandPayload::Edit(burnburn_document::GeometryCommand::Ops(ops));
    match run(s, document, payload, request) {
        Ok(update) => {
            s.geometry_dialog = None;
            if kind == GeometryDialogKind::RoundCorners {
                // One radius for the tool and the dialog: the last one used.
                s.editor.set_round_radius(value);
            }
            update
        }
        Err(e) if is_nothing_to_do(&e) => refuse(s, MessageKey::NothingToChange),
        Err(_) => refuse(s, MessageKey::CommandRejected),
    }
}

pub(super) fn cancel_dialog(s: &mut Session, document: DocumentId) -> AppUpdate {
    if s.geometry_dialog.take().is_some() { AppUpdate::redraw(document) } else { AppUpdate::default() }
}

pub(super) fn dialog_view(s: &Session) -> Option<GeometryDialogViewModel> {
    s.geometry_dialog.as_ref().map(|d| GeometryDialogViewModel { kind: d.kind, text: d.text.clone(), error: d.error })
}

// ---------------------------------------------------------------------------------------------
// Tool options
// ---------------------------------------------------------------------------------------------

fn whole(text: &str, min: u32) -> Result<u32, MessageKey> {
    let v = parse_number(text, &[]).map_err(MessageKey::InvalidLength)?;
    if v.fract() == 0.0 && v >= f64::from(min) && v <= f64::from(burnburn_geometry::MAX_SIDES) {
        Ok(v as u32)
    } else {
        Err(MessageKey::ShapeParamInvalid)
    }
}

fn number(text: &str) -> Result<f64, MessageKey> {
    parse_number(text, &[]).map_err(MessageKey::InvalidLength)
}

fn row(key: ParamKey, text: String, s: &Session) -> ParamRow {
    let error = s.options.refused.filter(|(k, _)| *k == key).map(|(_, m)| m);
    ParamRow { key, text, toggle: key == ParamKey::KeepOriginal, error }
}

/// The options rows of the active tool.
pub(super) fn option_rows(s: &Session) -> Vec<ParamRow> {
    let d = s.editor.shape_defaults();
    let o = &s.options;
    match s.editor.tool() {
        ToolKind::Polygon => vec![row(ParamKey::Sides, d.polygon_sides.to_string(), s)],
        ToolKind::Star => {
            vec![row(ParamKey::Points, d.star_points.to_string(), s), row(ParamKey::Ratio, format_mm(d.star_ratio), s)]
        }
        ToolKind::DoubleStar => vec![
            row(ParamKey::Points, d.double_star_points.to_string(), s),
            row(ParamKey::Ratio, format_mm(d.double_star_ratio), s),
        ],
        ToolKind::RoundedRectangle => vec![row(ParamKey::CornerRadius, format_mm(d.corner_radius), s)],
        ToolKind::RoundCorner => vec![row(ParamKey::CornerRadius, format_mm(s.editor.round_radius()), s)],
        ToolKind::PatternRectangular => {
            vec![row(ParamKey::Columns, o.columns.to_string(), s), row(ParamKey::Rows, o.rows.to_string(), s)]
        }
        ToolKind::PatternCircular => {
            vec![row(ParamKey::Count, o.count.to_string(), s), row(ParamKey::Sweep, format_mm(o.sweep_deg), s)]
        }
        ToolKind::Mirror => vec![row(ParamKey::KeepOriginal, u8::from(o.keep_original).to_string(), s)],
        ToolKind::Fillet => vec![row(ParamKey::FilletRadius, format_mm(o.fillet_radius), s)],
        ToolKind::ChamferEqual => vec![row(ParamKey::ChamferDistance, format_mm(o.chamfer_distance), s)],
        ToolKind::ChamferTwo => vec![
            row(ParamKey::ChamferDistance, format_mm(o.chamfer_distance), s),
            row(ParamKey::ChamferDistance2, format_mm(o.chamfer_distance2), s),
        ],
        ToolKind::ChamferAngle => vec![
            row(ParamKey::ChamferDistance, format_mm(o.chamfer_distance), s),
            row(ParamKey::ChamferAngle, format_mm(o.chamfer_angle), s),
        ],
        _ => Vec::new(),
    }
}

/// Applies the kept options to a freshly selected tool.
pub(super) fn apply_options(s: &mut Session) {
    s.options.refused = None;
    let o = s.options.clone();
    for param in [
        ModifyParam::Columns(o.columns),
        ModifyParam::Rows(o.rows),
        ModifyParam::Count(o.count),
        ModifyParam::SweepDeg(o.sweep_deg),
        ModifyParam::KeepOriginal(o.keep_original),
        ModifyParam::Radius(o.fillet_radius),
        ModifyParam::Distance(o.chamfer_distance),
        ModifyParam::Distance2(o.chamfer_distance2),
        ModifyParam::AngleDeg(o.chamfer_angle),
    ] {
        s.editor.set_param(ToolParam::Modify(param));
    }
}

pub(super) fn set_tool_option(s: &mut Session, document: DocumentId, key: ParamKey, text: &str) -> AppUpdate {
    let tool = s.editor.tool();
    let d = *s.editor.shape_defaults();
    let accepted: Result<(), MessageKey> = (|| match (tool, key) {
        (ToolKind::Polygon, ParamKey::Sides) => {
            let sides = whole(text, 3)?;
            set_defaults(s, ShapeDefaults { polygon_sides: sides, ..d })
        }
        (ToolKind::Star, ParamKey::Points) => {
            let points = whole(text, 3)?;
            set_defaults(s, ShapeDefaults { star_points: points, ..d })
        }
        (ToolKind::Star, ParamKey::Ratio) => {
            let ratio = number(text)?;
            set_defaults(s, ShapeDefaults { star_ratio: ratio, ..d })
        }
        (ToolKind::DoubleStar, ParamKey::Points) => {
            let points = whole(text, 3)?;
            set_defaults(s, ShapeDefaults { double_star_points: points, ..d })
        }
        (ToolKind::DoubleStar, ParamKey::Ratio) => {
            let ratio = number(text)?;
            set_defaults(s, ShapeDefaults { double_star_ratio: ratio, ..d })
        }
        (ToolKind::RoundedRectangle, ParamKey::CornerRadius) => {
            let radius = number(text)?;
            set_defaults(s, ShapeDefaults { corner_radius: radius, ..d })
        }
        (ToolKind::RoundCorner, ParamKey::CornerRadius) => {
            let radius = parse_value(text).map_err(MessageKey::InvalidLength)?;
            s.editor.set_round_radius(radius);
            Ok(())
        }
        (ToolKind::PatternRectangular, ParamKey::Columns) => {
            let n = whole(text, 1)?;
            keep(s, ModifyParam::Columns(n), |o| o.columns = n)
        }
        (ToolKind::PatternRectangular, ParamKey::Rows) => {
            let n = whole(text, 1)?;
            keep(s, ModifyParam::Rows(n), |o| o.rows = n)
        }
        (ToolKind::PatternCircular, ParamKey::Count) => {
            let n = whole(text, 2)?;
            keep(s, ModifyParam::Count(n), |o| o.count = n)
        }
        (ToolKind::PatternCircular, ParamKey::Sweep) => {
            let degrees = number(text)?;
            keep(s, ModifyParam::SweepDeg(degrees), |o| o.sweep_deg = degrees)
        }
        (ToolKind::Mirror, ParamKey::KeepOriginal) => {
            let on = number(text)? != 0.0;
            keep(s, ModifyParam::KeepOriginal(on), |o| o.keep_original = on)
        }
        (ToolKind::Fillet, ParamKey::FilletRadius) => {
            let radius = number(text)?;
            keep(s, ModifyParam::Radius(radius), |o| o.fillet_radius = radius)
        }
        (ToolKind::ChamferEqual | ToolKind::ChamferTwo | ToolKind::ChamferAngle, ParamKey::ChamferDistance) => {
            let distance = number(text)?;
            keep(s, ModifyParam::Distance(distance), |o| o.chamfer_distance = distance)
        }
        (ToolKind::ChamferTwo, ParamKey::ChamferDistance2) => {
            let distance = number(text)?;
            keep(s, ModifyParam::Distance2(distance), |o| o.chamfer_distance2 = distance)
        }
        (ToolKind::ChamferAngle, ParamKey::ChamferAngle) => {
            let degrees = number(text)?;
            keep(s, ModifyParam::AngleDeg(degrees), |o| o.chamfer_angle = degrees)
        }
        _ => Err(MessageKey::ShapeParamInvalid),
    })();
    s.options.refused = accepted.err().map(|m| (key, m));
    AppUpdate::redraw(document)
}

fn set_defaults(s: &mut Session, defaults: ShapeDefaults) -> Result<(), MessageKey> {
    s.editor.set_shape_defaults(defaults).map_err(|_: ShapeParamError| MessageKey::ShapeParamInvalid)
}

/// Gives the active tool a modify option and, when it takes it, remembers it for later tools.
fn keep(s: &mut Session, param: ModifyParam, remember: impl FnOnce(&mut ToolOptions)) -> Result<(), MessageKey> {
    if s.editor.set_param(ToolParam::Modify(param)) {
        remember(&mut s.options);
        Ok(())
    } else {
        Err(MessageKey::ShapeParamInvalid)
    }
}

// ---------------------------------------------------------------------------------------------
// The parameters of the selected basic shape
// ---------------------------------------------------------------------------------------------

fn param_rows(params: &ShapeParams) -> Vec<(ParamKey, String)> {
    match *params {
        ShapeParams::Polygon { sides } => vec![(ParamKey::Sides, sides.to_string())],
        ShapeParams::Star { points, ratio } | ShapeParams::DoubleStar { points, ratio } => {
            vec![(ParamKey::Points, points.to_string()), (ParamKey::Ratio, format_mm(ratio))]
        }
        ShapeParams::RoundedRectangle { radius, .. } => vec![(ParamKey::CornerRadius, format_mm(radius))],
        _ => Vec::new(),
    }
}

pub(super) fn shape_panel(s: &Session) -> Option<ShapePanelViewModel> {
    let id = s.selection.single()?;
    let snapshot = s.doc.snapshot();
    let shape = snapshot.object(id)?.parametric()?;
    let refused = s.options.shape_refused.filter(|(object, _, _)| *object == id);
    let rows = param_rows(shape.params())
        .into_iter()
        .map(|(key, text)| ParamRow {
            key,
            text,
            toggle: false,
            error: refused.filter(|(_, k, _)| *k == key).map(|(_, _, m)| m),
        })
        .collect();
    Some(ShapePanelViewModel { object: id, rows })
}

/// The parameters with `key` set from `text`.
fn edited(params: ShapeParams, key: ParamKey, text: &str) -> Result<ShapeParams, MessageKey> {
    Ok(match (params, key) {
        (ShapeParams::Polygon { .. }, ParamKey::Sides) => ShapeParams::Polygon { sides: whole(text, 3)? },
        (ShapeParams::Star { ratio, .. }, ParamKey::Points) => ShapeParams::Star { points: whole(text, 3)?, ratio },
        (ShapeParams::Star { points, .. }, ParamKey::Ratio) => ShapeParams::Star { points, ratio: number(text)? },
        (ShapeParams::DoubleStar { ratio, .. }, ParamKey::Points) => {
            ShapeParams::DoubleStar { points: whole(text, 3)?, ratio }
        }
        (ShapeParams::DoubleStar { points, .. }, ParamKey::Ratio) => {
            ShapeParams::DoubleStar { points, ratio: number(text)? }
        }
        (ShapeParams::RoundedRectangle { width, height, .. }, ParamKey::CornerRadius) => {
            ShapeParams::RoundedRectangle { width, height, radius: number(text)? }
        }
        _ => return Err(MessageKey::ShapeParamInvalid),
    })
}

pub(super) fn commit_shape_param(
    s: &mut Session,
    document: DocumentId,
    key: ParamKey,
    text: &str,
    request: Option<RequestId>,
) -> AppUpdate {
    let Some(id) = s.selection.single() else { return AppUpdate::default() };
    let snapshot = s.doc.snapshot();
    let Some(current) = snapshot.object(id).and_then(|o| o.parametric()).map(|p| *p.params()) else {
        return AppUpdate::default();
    };
    let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
    let refuse = |s: &mut Session, key_: MessageKey| {
        s.options.shape_refused = Some((id, key, key_));
        AppUpdate::redraw(document)
    };
    let params = match edited(current, key, text) {
        Ok(p) => p,
        Err(m) => return refuse(s, m),
    };
    if params == current {
        s.options.shape_refused = None;
        return AppUpdate::redraw(document);
    }
    if params.validate().is_err() {
        return refuse(s, MessageKey::ShapeParamInvalid);
    }
    let payload = CommandPayload::Shape(ShapeCommand::SetParams { target: id, params });
    match run(s, document, payload, request) {
        Ok(update) => {
            s.options.shape_refused = None;
            update
        }
        Err(_) => refuse(s, MessageKey::ShapeParamInvalid),
    }
}
