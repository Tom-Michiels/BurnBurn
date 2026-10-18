//! The SolidCraft-look modify, pattern and constraint tools (lane S-modify): shared types, the
//! tool-id table and the dispatcher over the individual state machines. Every gesture returns at
//! most one [`EditCommit`]; a refused gesture returns [`ToolStep::Rejected`] with a typed reason
//! and leaves the document and the tool state as they were (no silent fallback).

use std::sync::Arc;

use burnburn_document::GeometrySource;
use burnburn_geometry::{BreakError, CornerError, ExtendError, OffsetError, Path, Point, TrimError};
use burnburn_types::{GeometryId, Length};

use super::{
    CLICK_TOLERANCE_PT, HANDLE_RADIUS_PT, ShapeKind, Tool, ToolContext, ToolKind, ToolParam, ToolPreview, ToolStep,
    chamfer, is_escape, key_down, tool_pointer,
};
use crate::input::{CanvasInput, CanvasKey, PointerEvent, PointerPhase};
use crate::snap::SnapCandidate;

use super::break_tool::BreakAction;
use super::constraint::{AutoAction, ConstraintAction};
use super::construction::ConstructionAction;
use super::dimension::DimensionAction;
use super::extend::ExtendAction;
use super::fillet::CornerAction;
use super::mirror::MirrorAction;
use super::move_scale::{MoveAction, ScaleAction};
use super::offset::OffsetAction;
use super::pattern::{GridAction, RadialAction};
use super::spline::{ConicAction, SplineAction};
use super::text::TextAction;
use super::trim::TrimAction;

/// Settings of the modify, pattern and constraint tools (via `ToolParam::Modify`).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ModifyParam {
    /// Fillet radius in millimetres (positive).
    Radius(f64),
    /// Chamfer distance (equal, first, or distance-angle) in millimetres (positive).
    Distance(f64),
    /// Second chamfer distance in millimetres (positive).
    Distance2(f64),
    /// Chamfer angle in degrees, measured from the first side (0 < angle < 180).
    AngleDeg(f64),
    /// Conic shape in (0, 1); 0.5 is a parabola.
    Rho(f64),
    /// Rectangular pattern columns (at least 1).
    Columns(u32),
    /// Rectangular pattern rows (at least 1).
    Rows(u32),
    /// Circular pattern copies including the original (at least 2).
    Count(u32),
    /// Circular pattern sweep in degrees (0 < |sweep| <= 360).
    SweepDeg(f64),
    /// Scale factor typed instead of picked (positive).
    Factor(f64),
    /// Mirror keeps the originals (copies) instead of mirroring them in place.
    KeepOriginal(bool),
}

/// A distance between two picked points outside a sketch (a readout, not a document edit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub from: Point,
    pub to: Point,
}

impl Measurement {
    pub fn distance(&self) -> f64 {
        self.from.distance(self.to)
    }

    pub fn dx(&self) -> f64 {
        self.to.x - self.from.x
    }

    pub fn dy(&self) -> f64 {
        self.to.y - self.from.y
    }
}

/// What is not available yet, as a typed reason (never a silent fallback to something else).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unavailable {
    /// Construction is a flag of sketch entities; drawn objects have none yet.
    ConstructionOutsideSketch,
    /// Fillet and chamfer need two straight sides; line–arc corners are not supported yet.
    CurvedCorner,
    /// Only lines and arcs can be extended, not Bézier curves.
    BezierExtend,
}

/// Why a gesture was refused. The document is unchanged.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Rejection {
    /// Nothing usable under the pointer.
    NothingPicked,
    /// Sketch output is regenerated from its sketch; edit the sketch instead.
    DerivedGeometry(GeometryId),
    /// The tool works on the selection and nothing is selected.
    NeedsSelection,
    /// The picked entities do not fit this constraint or dimension.
    InvalidSelection,
    /// Auto-constrain found nothing it could add.
    NothingToConstrain,
    /// A typed value is not valid (zero or negative size, count out of range, ...).
    InvalidValue,
    Corner(CornerError),
    Trim(TrimError),
    Extend(ExtendError),
    Break(BreakError),
    Offset(OffsetError),
    Unavailable(Unavailable),
}

impl From<CornerError> for Rejection {
    fn from(e: CornerError) -> Self {
        match e {
            CornerError::CurvedSide => Rejection::Unavailable(Unavailable::CurvedCorner),
            other => Rejection::Corner(other),
        }
    }
}

impl From<ExtendError> for Rejection {
    fn from(e: ExtendError) -> Self {
        match e {
            ExtendError::NotExtendable => Rejection::Unavailable(Unavailable::BezierExtend),
            other => Rejection::Extend(other),
        }
    }
}

/// The tool for a SolidCraft sketch command id (without the `sketch.` prefix), for the tools
/// of this lane.
pub fn modify_tool_kind(id: &str) -> Option<ToolKind> {
    Some(match id {
        "spline.fit_point" => ToolKind::SplineFit,
        "spline.control_point" => ToolKind::SplineControl,
        "conic" => ToolKind::Conic,
        "text" => ToolKind::Text,
        "dimension" => ToolKind::Dimension,
        "fillet" => ToolKind::Fillet,
        "chamfer.equal_distance" => ToolKind::ChamferEqual,
        "chamfer.two_distance" => ToolKind::ChamferTwo,
        "chamfer.distance_angle" => ToolKind::ChamferAngle,
        "trim" => ToolKind::Trim,
        "extend" => ToolKind::Extend,
        "break" => ToolKind::Break,
        "offset" => ToolKind::Offset,
        "mirror" => ToolKind::Mirror,
        "move" => ToolKind::Move,
        "scale" => ToolKind::Scale,
        "construction" => ToolKind::Construction,
        "pattern.rectangular" => ToolKind::PatternRectangular,
        "pattern.circular" => ToolKind::PatternCircular,
        "constraint.coincident" => ToolKind::ConstraintCoincident,
        "constraint.collinear" => ToolKind::ConstraintCollinear,
        "constraint.concentric" => ToolKind::ConstraintConcentric,
        "constraint.equal" => ToolKind::ConstraintEqual,
        "constraint.fix" => ToolKind::ConstraintFix,
        "constraint.horizontal_vertical" => ToolKind::ConstraintHorizontalVertical,
        "constraint.midpoint" => ToolKind::ConstraintMidpoint,
        "constraint.parallel" => ToolKind::ConstraintParallel,
        "constraint.perpendicular" => ToolKind::ConstraintPerpendicular,
        "constraint.symmetry" => ToolKind::ConstraintSymmetry,
        "constraint.tangent" => ToolKind::ConstraintTangent,
        "auto_constrain" => ToolKind::AutoConstrain,
        _ => return None,
    })
}

/// The behaviour of one modify tool inside the shared [`Engine`]: the engine routes the
/// pointer, tracks the snap and turns Esc and pointer cancel into [`Action::reset`].
pub(super) trait Action: std::fmt::Debug + Clone {
    /// A primary press at the (snapped) world position `at`; `ev.position` is the raw position.
    fn press(&mut self, ctx: &ToolContext<'_>, at: Point, ev: &PointerEvent) -> ToolStep;
    /// The pointer moved (with or without a button).
    fn hover(&mut self, _ctx: &ToolContext<'_>, _at: Point, _ev: &PointerEvent) {}
    fn release(&mut self, _ctx: &ToolContext<'_>, _at: Point, _ev: &PointerEvent) -> ToolStep {
        ToolStep::Nothing
    }
    fn key(&mut self, _key: CanvasKey, _ctx: &ToolContext<'_>) -> ToolStep {
        ToolStep::Nothing
    }
    /// Abandons the gesture; `true` when one was in progress.
    fn reset(&mut self) -> bool;
    fn preview(&self) -> Option<ToolPreview> {
        None
    }
    fn set_param(&mut self, _param: ModifyParam) -> bool {
        false
    }
    fn exact_size(&mut self, _width: Length, _height: Length) -> ToolStep {
        ToolStep::Nothing
    }
}

#[derive(Debug, Clone)]
pub(super) struct Engine<A> {
    action: A,
    snap: Option<SnapCandidate>,
}

impl<A: Action> Engine<A> {
    pub(super) fn new(action: A) -> Self {
        Engine { action, snap: None }
    }
}

impl<A: Action> Tool for Engine<A> {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        if let Some((key, _)) = key_down(input) {
            return self.action.key(key, ctx);
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase == PointerPhase::Cancel {
            return self.cancel();
        }
        let (at, candidate) = ctx.snapped(ev, &[]);
        let snap_changed = candidate != self.snap;
        self.snap = candidate;
        match ev.phase {
            PointerPhase::Down => self.action.press(ctx, at, ev),
            PointerPhase::Up => self.action.release(ctx, at, ev),
            PointerPhase::Move => {
                self.action.hover(ctx, at, ev);
                if snap_changed || self.action.preview().is_some() {
                    ToolStep::PreviewChanged
                } else {
                    ToolStep::Nothing
                }
            }
            _ => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        if self.action.reset() { ToolStep::Cancelled } else { ToolStep::Nothing }
    }

    fn preview(&self) -> Option<ToolPreview> {
        self.action.preview()
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }

    fn exact_size(&mut self, width: Length, height: Length) -> ToolStep {
        self.action.exact_size(width, height)
    }

    fn set_param(&mut self, param: ToolParam) -> bool {
        match param {
            ToolParam::Modify(p) => self.action.set_param(p),
            _ => false,
        }
    }
}

macro_rules! tools {
    ($($variant:ident($action:ty)),* $(,)?) => {
        #[derive(Debug, Clone)]
        enum Inner {
            $($variant(Engine<$action>)),*
        }

        macro_rules! with_tool {
            ($self_:expr, $t:ident => $body:expr) => {
                match $self_ {
                    $(Inner::$variant($t) => $body),*
                }
            };
        }
    };
}

tools! {
    Spline(SplineAction),
    Conic(ConicAction),
    Text(TextAction),
    Dimension(DimensionAction),
    Corner(CornerAction),
    Trim(TrimAction),
    Extend(ExtendAction),
    Break(BreakAction),
    Offset(OffsetAction),
    Mirror(MirrorAction),
    Move(MoveAction),
    Scale(ScaleAction),
    Construction(ConstructionAction),
    Grid(GridAction),
    Radial(RadialAction),
    Constraint(ConstraintAction),
    Auto(AutoAction),
}

/// The state machine of one modify, pattern or constraint tool.
#[derive(Debug, Clone)]
pub struct ModifyTool {
    inner: Inner,
}

impl ModifyTool {
    pub(super) fn for_kind(kind: ToolKind) -> Self {
        let inner = match kind {
            ToolKind::SplineFit => Inner::Spline(Engine::new(SplineAction::new(true))),
            ToolKind::SplineControl => Inner::Spline(Engine::new(SplineAction::new(false))),
            ToolKind::Conic => Inner::Conic(Engine::new(ConicAction::default())),
            ToolKind::Dimension => Inner::Dimension(Engine::new(DimensionAction::default())),
            ToolKind::Fillet => Inner::Corner(Engine::new(CornerAction::fillet())),
            ToolKind::ChamferEqual | ToolKind::ChamferTwo | ToolKind::ChamferAngle => {
                Inner::Corner(Engine::new(chamfer::action(kind)))
            }
            ToolKind::Trim => Inner::Trim(Engine::new(TrimAction)),
            ToolKind::Extend => Inner::Extend(Engine::new(ExtendAction)),
            ToolKind::Break => Inner::Break(Engine::new(BreakAction)),
            ToolKind::Offset => Inner::Offset(Engine::new(OffsetAction::default())),
            ToolKind::Mirror => Inner::Mirror(Engine::new(MirrorAction::default())),
            ToolKind::Move => Inner::Move(Engine::new(MoveAction::default())),
            ToolKind::Scale => Inner::Scale(Engine::new(ScaleAction::default())),
            ToolKind::Construction => Inner::Construction(Engine::new(ConstructionAction)),
            ToolKind::PatternRectangular => Inner::Grid(Engine::new(GridAction::default())),
            ToolKind::PatternCircular => Inner::Radial(Engine::new(RadialAction::default())),
            ToolKind::AutoConstrain => Inner::Auto(Engine::new(AutoAction)),
            ToolKind::ConstraintCoincident
            | ToolKind::ConstraintCollinear
            | ToolKind::ConstraintConcentric
            | ToolKind::ConstraintEqual
            | ToolKind::ConstraintFix
            | ToolKind::ConstraintHorizontalVertical
            | ToolKind::ConstraintMidpoint
            | ToolKind::ConstraintParallel
            | ToolKind::ConstraintPerpendicular
            | ToolKind::ConstraintSymmetry
            | ToolKind::ConstraintTangent => Inner::Constraint(Engine::new(ConstraintAction::new(kind))),
            // `Text` and anything that is not a tool of this lane: a click places a text anchor.
            _ => Inner::Text(Engine::new(TextAction)),
        };
        ModifyTool { inner }
    }
}

impl Tool for ModifyTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        with_tool!(&mut self.inner, t => t.input(input, ctx))
    }

    fn cancel(&mut self) -> ToolStep {
        with_tool!(&mut self.inner, t => t.cancel())
    }

    fn preview(&self) -> Option<ToolPreview> {
        with_tool!(&self.inner, t => t.preview())
    }

    fn snap(&self) -> Option<SnapCandidate> {
        with_tool!(&self.inner, t => t.snap())
    }

    fn exact_size(&mut self, width: Length, height: Length) -> ToolStep {
        with_tool!(&mut self.inner, t => t.exact_size(width, height))
    }

    fn set_param(&mut self, param: ToolParam) -> bool {
        with_tool!(&mut self.inner, t => t.set_param(param))
    }
}

/// Reach of a click for corners and open ends, in screen points.
pub(super) const REACH_PT: f64 = HANDLE_RADIUS_PT;

/// The object whose outline passes nearest to `at` within the click tolerance (topmost wins ties).
pub(super) fn outline_hit(ctx: &ToolContext<'_>, at: Point) -> Option<GeometryId> {
    let tol = ctx.tolerance_mm(CLICK_TOLERANCE_PT);
    let mut best: Option<(f64, GeometryId)> = None;
    for (id, obj) in ctx.document.objects() {
        let Some(bounds) = obj.bounds() else { continue };
        if bounds.distance_to(at) > tol {
            continue;
        }
        let Some(near) = obj.path().nearest(at) else { continue };
        if near.distance <= tol && best.is_none_or(|(d, _)| near.distance <= d) {
            best = Some((near.distance, id));
        }
    }
    best.map(|(_, id)| id)
}

/// Sketch output is regenerated from its sketch and cannot be edited as a drawn object.
pub(super) fn ensure_editable(ctx: &ToolContext<'_>, id: GeometryId) -> Result<(), Rejection> {
    match ctx.document.object(id).map(|o| o.source()) {
        Some(GeometrySource::Derived(_)) => Err(Rejection::DerivedGeometry(id)),
        _ => Ok(()),
    }
}

/// A rejection as the step of a refused gesture.
pub(super) fn rejected(r: impl Into<Rejection>) -> ToolStep {
    ToolStep::Rejected(r.into())
}

/// A preview of a finished-looking path.
pub(super) fn path_preview(path: Path) -> ToolPreview {
    ToolPreview::Shape { path: Arc::new(path), shape: ShapeKind::Curve }
}

/// A preview of a rubber line.
pub(super) fn line_preview(from: Point, to: Point) -> Option<ToolPreview> {
    Path::line(from, to).ok().map(|path| ToolPreview::Shape { path: Arc::new(path), shape: ShapeKind::Line })
}

/// Test helpers shared by the tool modules.
#[cfg(test)]
pub(super) mod kit {
    use burnburn_document::DocumentSnapshot;
    use burnburn_document::sketch::{Sketch, SketchId, SketchLayer};
    use burnburn_geometry::{Path, Point};
    use burnburn_types::{GeometryId, StableId};

    use super::super::fixture::*;
    use super::super::{EditCommit, Editor, ToolKind, ToolParam, ToolStep};
    use super::{ModifyParam, Rejection};
    use crate::input::{CanvasKey, PointerPhase};

    pub fn editor(kind: ToolKind) -> Editor {
        let mut ed = Editor::new();
        ed.select_tool(kind);
        ed
    }

    pub fn param(ed: &mut Editor, p: ModifyParam) {
        assert!(ed.set_param(ToolParam::Modify(p)), "{p:?} must be accepted");
    }

    /// Whether the tool accepts the setting.
    pub trait ParamOk {
        fn set_param_ok(&mut self, p: ModifyParam) -> bool;
    }

    impl ParamOk for Editor {
        fn set_param_ok(&mut self, p: ModifyParam) -> bool {
            self.set_param(ToolParam::Modify(p))
        }
    }

    pub fn empty() -> Fixture {
        Fixture::new(Vec::new())
    }

    pub fn with_objects(objects: Vec<burnburn_document::GeometryObject>) -> Fixture {
        Fixture::new(objects)
    }

    /// A fixture whose snapshot also holds `sketch` (as `SketchId` 100).
    pub fn with_sketch(objects: Vec<burnburn_document::GeometryObject>, sketch: Sketch) -> Fixture {
        let mut f = Fixture::new(Vec::new());
        let layer = SketchLayer::from_parts(vec![(SketchId::from_raw(100), sketch)], vec![]).unwrap();
        f.snapshot = DocumentSnapshot::from_parts(objects, 200).unwrap().with_sketches(layer).unwrap();
        f
    }

    pub fn sketch_id() -> SketchId {
        SketchId::from_raw(100)
    }

    /// A press and release in place.
    pub fn click(ed: &mut Editor, f: &Fixture, x: f64, y: f64) -> Vec<ToolStep> {
        vec![
            ed.input(&at(PointerPhase::Down, x, y, none()), &f.ctx()),
            ed.input(&at(PointerPhase::Up, x, y, none()), &f.ctx()),
        ]
    }

    /// Clicks every point in turn and returns all steps.
    pub fn clicks(ed: &mut Editor, f: &Fixture, pts: &[(f64, f64)]) -> Vec<ToolStep> {
        pts.iter().flat_map(|&(x, y)| click(ed, f, x, y)).collect()
    }

    pub fn hover_to(ed: &mut Editor, f: &Fixture, x: f64, y: f64) -> ToolStep {
        ed.input(&hover(x, y), &f.ctx())
    }

    pub fn press_key(ed: &mut Editor, f: &Fixture, k: CanvasKey) -> ToolStep {
        ed.input(&key(k), &f.ctx())
    }

    pub fn commits(steps: &[ToolStep]) -> Vec<EditCommit> {
        steps
            .iter()
            .filter_map(|s| match s {
                ToolStep::Commit(c) => Some(c.clone()),
                _ => None,
            })
            .collect()
    }

    /// The single commit of a gesture.
    pub fn only(steps: &[ToolStep]) -> EditCommit {
        let c = commits(steps);
        assert_eq!(c.len(), 1, "exactly one commit: {steps:?}");
        c[0].clone()
    }

    pub fn rejection(steps: &[ToolStep]) -> Option<Rejection> {
        steps.iter().find_map(|s| match s {
            ToolStep::Rejected(r) => Some(r.clone()),
            _ => None,
        })
    }

    pub fn ids(raw: &[u64]) -> Vec<GeometryId> {
        raw.iter().map(|r| GeometryId::from_raw(*r)).collect()
    }

    pub fn polyline(raw: u64, pts: &[(f64, f64)], closed: bool) -> burnburn_document::GeometryObject {
        let pts: Vec<Point> = pts.iter().map(|(x, y)| Point::new(*x, *y)).collect();
        path_obj(raw, Path::polyline(&pts, closed).unwrap())
    }

    pub fn near(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-9
    }
}

#[cfg(test)]
mod tests {
    use super::super::modify::kit::*;
    use super::*;

    const ALL: [ToolKind; 31] = [
        ToolKind::SplineFit,
        ToolKind::SplineControl,
        ToolKind::Conic,
        ToolKind::Text,
        ToolKind::Dimension,
        ToolKind::Fillet,
        ToolKind::ChamferEqual,
        ToolKind::ChamferTwo,
        ToolKind::ChamferAngle,
        ToolKind::Trim,
        ToolKind::Extend,
        ToolKind::Break,
        ToolKind::Offset,
        ToolKind::Mirror,
        ToolKind::Move,
        ToolKind::Scale,
        ToolKind::Construction,
        ToolKind::PatternRectangular,
        ToolKind::PatternCircular,
        ToolKind::ConstraintCoincident,
        ToolKind::ConstraintCollinear,
        ToolKind::ConstraintConcentric,
        ToolKind::ConstraintEqual,
        ToolKind::ConstraintFix,
        ToolKind::ConstraintHorizontalVertical,
        ToolKind::ConstraintMidpoint,
        ToolKind::ConstraintParallel,
        ToolKind::ConstraintPerpendicular,
        ToolKind::ConstraintSymmetry,
        ToolKind::ConstraintTangent,
        ToolKind::AutoConstrain,
    ];

    const IDS: [&str; 31] = [
        "spline.fit_point",
        "spline.control_point",
        "conic",
        "text",
        "dimension",
        "fillet",
        "chamfer.equal_distance",
        "chamfer.two_distance",
        "chamfer.distance_angle",
        "trim",
        "extend",
        "break",
        "offset",
        "mirror",
        "move",
        "scale",
        "construction",
        "pattern.rectangular",
        "pattern.circular",
        "constraint.coincident",
        "constraint.collinear",
        "constraint.concentric",
        "constraint.equal",
        "constraint.fix",
        "constraint.horizontal_vertical",
        "constraint.midpoint",
        "constraint.parallel",
        "constraint.perpendicular",
        "constraint.symmetry",
        "constraint.tangent",
        "auto_constrain",
    ];

    #[test]
    fn every_tool_id_maps_to_its_own_kind() {
        for (id, kind) in IDS.iter().zip(ALL) {
            assert_eq!(modify_tool_kind(id), Some(kind), "{id}");
        }
        assert_eq!(modify_tool_kind("line"), None, "drawing ids belong to S-draw");
        let distinct: std::collections::BTreeSet<_> = ALL.iter().collect();
        assert_eq!(distinct.len(), ALL.len());
    }

    #[test]
    fn switching_away_from_every_tool_in_mid_gesture_cancels_without_a_commit() {
        let f = empty();
        for kind in ALL {
            let mut ed = editor(kind);
            click(&mut ed, &f, 1.0, 1.0);
            click(&mut ed, &f, 5.0, 3.0);
            let step = ed.select_tool(ToolKind::Select);
            assert!(!matches!(step, ToolStep::Commit(_)), "{kind:?}: {step:?}");
            assert!(!ed.is_gesture_active(), "{kind:?}");
        }
    }
}
