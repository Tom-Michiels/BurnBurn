//! Tool state machines. A tool reads the immutable snapshot and window selection, keeps its own
//! temporary preview, and returns at most one [`EditCommit`] per finished gesture. Presentation
//! turns that commit into exactly one `DocumentCommand` with the expected revision (contract §4).

mod arc;
mod break_tool;
mod chamfer;
mod circle;
mod click;
mod constraint;
mod construction;
mod dimension;
mod draw;
mod ellipse;
mod extend;
mod fillet;
mod line;
mod mirror;
mod modify;
mod move_scale;
mod node;
mod offset;
mod parametric;
mod pattern;
mod pick;
mod point;
mod rect;
mod rectangle;
mod reshape;
mod round_corner;
mod select;
mod sketch_pick;
mod slot;
mod spline;
mod start_point;
mod tangent;
mod text;
mod trim;

use std::sync::Arc;

use burnburn_document::sketch::{SketchCommand, SketchId};
use burnburn_document::structure::Modifier;
use burnburn_document::{DocumentSnapshot, OpsCommand, ReshapeCommand};
use burnburn_geometry::{
    Affine, Bounds, CornerCut, EndpointRef, ExtendNote, NodeEdit, NodeRef, ParametricShape, Path, Point, Rect, Shape,
    ShapeParamError,
};
use burnburn_types::{GeometryId, Length};

use crate::input::{CanvasInput, CanvasKey, KeyPhase, Modifiers, PointerButton, PointerEvent, PointerPhase};
use crate::node_ops::NodeCommand;
use crate::selection::{Selection, SelectionChange};
use crate::snap::{SnapCandidate, SnapSettings, snap_point};

pub use arc::TangentArcTool;
pub use circle::TangentCircleTool;
pub use click::DrawTool;
pub use constraint::{ConstraintKind, constraint_relations};
pub use draw::{CircleTool, EllipseTool, LineTool, PenTool, PolylineTool};
pub use modify::{Measurement, ModifyParam, ModifyTool, Rejection, Unavailable, modify_tool_kind};
pub use node::{NodeTarget, NodeTool, path_nodes};
pub use parametric::{BasicShapeTool, PolygonConstruction, ShapeDefaults};
pub(crate) use rectangle::rect_from_bounds;
pub use rectangle::{RectanglePreview, RectangleTool};
pub use reshape::{PREVIEW_SEGMENT_LIMIT, ReshapeMode, ReshapePreview, ReshapeTool};
pub use round_corner::RoundCornerTool;
pub use select::{Handle, ScaleGrip, SelectTool, selection_handles};
pub use start_point::StartPointTool;
pub use tangent::{TangentCirclesTool, TangentPreview};

/// Hit-test tolerance for clicks, in logical screen points (independent of zoom).
pub const CLICK_TOLERANCE_PT: f64 = 4.0;
/// A press must move this far (screen points) before it becomes a drag.
pub const DRAG_THRESHOLD_PT: f64 = 3.0;
/// Pick radius of selection handles and path nodes, in screen points.
pub const HANDLE_RADIUS_PT: f64 = 6.0;
/// Distance of the rotate handle above the selection's top edge, in screen points.
pub const ROTATE_HANDLE_OFFSET_PT: f64 = 24.0;

/// Read-only context for one input. The document snapshot is never mutated by a tool.
pub struct ToolContext<'a> {
    pub document: &'a DocumentSnapshot,
    pub selection: &'a Selection,
    /// World millimetres per logical screen point at this window's zoom.
    pub mm_per_point: f64,
    pub snap: &'a SnapSettings,
}

impl ToolContext<'_> {
    /// Snapped position for a pointer event (primary modifier suppresses snapping, E04).
    pub fn snapped(&self, ev: &PointerEvent, exclude: &[GeometryId]) -> (Point, Option<SnapCandidate>) {
        let c = snap_point(self.document, self.snap, self.mm_per_point, ev.position, ev.modifiers.command, exclude);
        (c.map_or(ev.position, |c| c.at), c)
    }

    pub fn tolerance_mm(&self, points: f64) -> f64 {
        points * self.mm_per_point
    }
}

/// What a drawing tool created; presentation maps it to an object kind and a localised name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ShapeKind {
    Ellipse,
    Circle,
    Line,
    Polyline,
    /// Pen tool: lines and cubic Béziers.
    Curve,
    /// Parametric basic shapes (W1-L2b): regular polygon (triangle, pentagon, octagon, …).
    Polygon,
    Star,
    DoubleStar,
    RoundedRectangle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformKind {
    Move,
    Scale,
    Rotate,
}

/// The semantic result of one finished gesture. Presentation adds document, expected revision
/// and request id; the editor knows neither.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum EditCommit {
    CreateRectangle {
        rect: Rect,
    },
    /// Ellipse, circle, line or polyline as a domain shape (resolved with `Shape::to_path`).
    CreateShape {
        shape: Shape,
    },
    /// A basic shape that stays editable (W1-L2b): resolve with `ParametricShape::to_path`.
    CreateParametric {
        shape: ParametricShape,
    },
    /// A free curve from the pen tool.
    CreatePath {
        path: Path,
    },
    /// Construction geometry (centerline): drawn and snappable, never part of production output.
    /// Presentation maps it once the document has a construction flag for drawn geometry.
    CreateConstruction {
        shape: Shape,
    },
    /// A reference point. Presentation maps it once the document has a point object.
    CreatePoint {
        at: Point,
    },
    /// Fillet or chamfer one corner. The document re-applies `GeometryCommand::CutCorner`
    /// (`target`, `pick`, `reach`, `cut`); `path` is the previewed result.
    CutCorner {
        target: GeometryId,
        pick: Point,
        reach: Length,
        cut: CornerCut,
        path: Path,
    },
    /// Trim at `pick` between the crossings with `cutters` (`GeometryCommand::TrimAt`).
    Trim {
        target: GeometryId,
        pick: Point,
        cutters: Vec<GeometryId>,
        path: Path,
    },
    /// Extend an open end to the nearest boundary (`GeometryCommand::ExtendEnd`).
    Extend {
        target: GeometryId,
        end: EndpointRef,
        boundaries: Vec<GeometryId>,
        path: Path,
        /// What the extension did that is worth telling (a Bézier that goes straight, G9-editor).
        note: ExtendNote,
    },
    /// Split at `pick` (`GeometryCommand::BreakAt`).
    Break {
        target: GeometryId,
        pick: Point,
        path: Path,
    },
    /// Mirror the targets with `transform` (a reflection). With `keep_original` the mirrored
    /// objects are copies and the originals stay; without, the targets themselves are mirrored
    /// (`CommandPayload::Transform`). Copies need a document duplicate command that does not exist yet.
    Mirror {
        targets: Vec<GeometryId>,
        transform: Affine,
        keep_original: bool,
    },
    /// A rectangular or circular array of `targets` as a non-destructive modifier
    /// (`Modifier::GridArray` / `Modifier::CircularArray`).
    AddArray {
        targets: Vec<GeometryId>,
        modifier: Modifier,
    },
    /// Sketch commands of one gesture, applied in order as one history entry.
    Sketch {
        sketch: SketchId,
        commands: Vec<SketchCommand>,
    },
    /// Place a text object at `at`; the text itself arrives through the host's text input
    /// (`UiIntent::InsertText`), which creates the `TextCommand` anchored here.
    PlaceText {
        at: Point,
    },
    /// One history entry for all targets.
    Transform {
        targets: Vec<GeometryId>,
        transform: Affine,
        kind: TransformKind,
    },
    /// One domain node edit and the outline it produces (untouched spans bit-identical).
    EditNodes {
        target: GeometryId,
        edit: NodeEdit,
        path: Path,
    },
    /// A selection-wide geometry operation (node set, round corners, optimize, cut, join). The
    /// document recomputes the result from the same domain operation; this is its exact payload.
    Ops(OpsCommand),
    /// Warp 4 points or Deform 16 points (`GeometryCommand::Reshape`), applied with Enter.
    Reshape(ReshapeCommand),
}

/// What presentation must do after one input.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ToolStep {
    Nothing,
    /// Preview or snap indicator changed: rebuild the scene; the document is untouched.
    PreviewChanged,
    /// Exactly one commit per gesture.
    Commit(EditCommit),
    Cancelled,
    /// Window-local selection change (never a document command).
    Select(SelectionChange),
    /// A measurement outside a sketch: a readout, never a document command.
    Measured(Measurement),
    /// The gesture cannot do what was asked; the document is untouched and the reason is typed.
    Rejected(Rejection),
}

/// The temporary gesture geometry a scene must draw. Never document data.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ToolPreview {
    Rectangle(RectanglePreview),
    /// Ellipse, circle, line, polyline or pen path as it would be committed (plus the rubber
    /// segment to the pointer for polyline/pen).
    Shape {
        path: Arc<Path>,
        shape: ShapeKind,
    },
    RubberBand {
        bounds: Bounds,
    },
    /// Move/scale/rotate drag: the targets drawn with `transform` applied.
    Transform {
        targets: Vec<GeometryId>,
        transform: Affine,
        kind: TransformKind,
    },
    /// Node drag: the edited outline of `target`.
    Node {
        target: GeometryId,
        path: Arc<Path>,
        dragged: NodeTarget,
    },
    /// Warp or Deform: the handles, their frame and the reshaped outlines.
    Reshape(ReshapePreview),
    /// Tangent Circle: the picked targets and every solution.
    Tangent(TangentPreview),
}

/// A tool setting that is not part of the gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ToolParam {
    /// Number of sides of the polygon tool (at least 3); the editor's [`ShapeDefaults`] hold it.
    Sides(u32),
    /// Corner radius in millimetres of the rounded rectangle (not negative, finite); also held in
    /// the [`ShapeDefaults`].
    CornerRadius(f64),
    /// Settings of the modify, pattern and constraint tools.
    Modify(ModifyParam),
    /// Warp 4 points: bilinear (`true`) or projective interpolation of the corners.
    Bilinear(bool),
}

/// Common behaviour of all tools.
pub trait Tool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep;
    /// Abandons the gesture in progress; never commits.
    fn cancel(&mut self) -> ToolStep;
    fn preview(&self) -> Option<ToolPreview>;
    /// The snap target of the last pointer event, for the cursor label and overlay.
    fn snap(&self) -> Option<SnapCandidate>;
    /// Exact size typed while drawing (E01). Default: not applicable.
    fn exact_size(&mut self, _width: Length, _height: Length) -> ToolStep {
        ToolStep::Nothing
    }
    /// Whether a gesture is in progress. Defaults to "has a preview"; tools whose gesture can be
    /// running without anything to draw (a first click on a degenerate pair) say so themselves.
    fn is_active(&self) -> bool {
        self.preview().is_some()
    }
    /// Applies a tool setting; `false` when this tool has no such setting or the value is invalid.
    fn set_param(&mut self, _param: ToolParam) -> bool {
        false
    }
}

pub(crate) fn is_escape(input: &CanvasInput) -> bool {
    matches!(input, CanvasInput::Key { key: CanvasKey::Escape, phase: KeyPhase::Down, .. })
}

pub(crate) fn key_down(input: &CanvasInput) -> Option<(CanvasKey, Modifiers)> {
    match input {
        CanvasInput::Key { key, modifiers, phase: KeyPhase::Down } => Some((*key, *modifiers)),
        _ => None,
    }
}

/// Pointer events a tool reacts to: presses/releases of the primary button, and every move or
/// cancel (hover moves have no button).
pub(crate) fn tool_pointer(input: &CanvasInput) -> Option<&PointerEvent> {
    match input {
        CanvasInput::Pointer(ev) => match ev.phase {
            PointerPhase::Move | PointerPhase::Cancel => Some(ev),
            PointerPhase::Down | PointerPhase::Up => (ev.button == Some(PointerButton::Primary)).then_some(ev),
        },
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ToolKind {
    Select,
    Node,
    Rectangle,
    Ellipse,
    Circle,
    Line,
    Polyline,
    Pen,
    /// Basic shapes (W1-L2b); sides, points, ratio and radius come from [`ShapeDefaults`]. They are
    /// also the SolidCraft rounded rectangle and the inscribed, circumscribed and edge polygons:
    /// one implementation per tool, with the construction mode as input
    /// ([`PolygonConstruction`]) and an editable shape as the result.
    Triangle,
    Polygon,
    Pentagon,
    Octagon,
    Star,
    DoubleStar,
    RoundedRectangle,
    /// Click a node to make it the cutting start (W1-L2a).
    StartPoint,
    /// Click a corner to round it with the editor's corner radius (W1-L2a).
    RoundCorner,
    /// Tangent Circle with every solution shown (W3-UI-schikken).
    TangentCircle,
    /// Warp Selection: four corner handles (W3-UI-schikken).
    Warp,
    /// Deform Selection: a 4 × 4 control net (W3-UI-schikken).
    Deform,
    // Sketch tools (S-draw); ids in docs/coordination/userflow.md.
    LineMidpoint,
    Centerline,
    RectangleCenter,
    RectangleThreePoint,
    CircleTwoPoint,
    CircleThreePoint,
    CircleTwoTangent,
    CircleThreeTangent,
    ArcThreePoint,
    ArcCenterPoint,
    ArcTangent,
    /// Centre, major-axis end, minor-axis point (the bounding-box ellipse stays `Ellipse`).
    EllipseAxes,
    SlotCenterToCenter,
    SlotOverall,
    SlotCenterPoint,
    SlotArcThreePoint,
    SlotArcCenter,
    Point,
    // Sketch tools (S-modify); ids in docs/coordination/userflow.md.
    SplineFit,
    SplineControl,
    Conic,
    Text,
    Dimension,
    Fillet,
    ChamferEqual,
    ChamferTwo,
    ChamferAngle,
    Trim,
    Extend,
    Break,
    Offset,
    Mirror,
    Move,
    Scale,
    Construction,
    PatternRectangular,
    PatternCircular,
    ConstraintCoincident,
    ConstraintCollinear,
    ConstraintConcentric,
    ConstraintEqual,
    ConstraintFix,
    ConstraintHorizontalVertical,
    ConstraintMidpoint,
    ConstraintParallel,
    ConstraintPerpendicular,
    ConstraintSymmetry,
    ConstraintTangent,
    AutoConstrain,
}

#[derive(Debug, Clone)]
enum Active {
    Select(SelectTool),
    Node(NodeTool),
    Rectangle(RectangleTool),
    Ellipse(EllipseTool),
    Circle(CircleTool),
    Line(LineTool),
    Polyline(PolylineTool),
    Pen(PenTool),
    Shape(BasicShapeTool),
    StartPoint(StartPointTool),
    RoundCorner(RoundCornerTool),
    Reshape(ReshapeTool),
    Tangent(TangentCirclesTool),
    Draw(DrawTool),
    TangentCircle(TangentCircleTool),
    TangentArc(TangentArcTool),
    Modify(ModifyTool),
}

impl Active {
    fn new(kind: ToolKind, defaults: &ShapeDefaults, round_radius_mm: f64) -> Self {
        match kind {
            ToolKind::Select => Active::Select(SelectTool::default()),
            ToolKind::Node => Active::Node(NodeTool::default()),
            ToolKind::Rectangle => Active::Rectangle(RectangleTool::default()),
            ToolKind::Ellipse => Active::Ellipse(EllipseTool::default()),
            ToolKind::Circle => Active::Circle(CircleTool::default()),
            ToolKind::Line => Active::Line(LineTool::default()),
            ToolKind::Polyline => Active::Polyline(PolylineTool::default()),
            ToolKind::Pen => Active::Pen(PenTool::default()),
            ToolKind::Triangle
            | ToolKind::Polygon
            | ToolKind::Pentagon
            | ToolKind::Octagon
            | ToolKind::Star
            | ToolKind::DoubleStar
            | ToolKind::RoundedRectangle => Active::Shape(BasicShapeTool::for_kind(kind, defaults)),
            ToolKind::StartPoint => Active::StartPoint(StartPointTool),
            ToolKind::RoundCorner => Active::RoundCorner(RoundCornerTool::new(round_radius_mm)),
            ToolKind::TangentCircle => Active::Tangent(TangentCirclesTool::new()),
            ToolKind::Warp => Active::Reshape(ReshapeTool::new(ReshapeMode::Warp)),
            ToolKind::Deform => Active::Reshape(ReshapeTool::new(ReshapeMode::Deform)),
            ToolKind::CircleTwoTangent => Active::TangentCircle(TangentCircleTool::new(2)),
            ToolKind::CircleThreeTangent => Active::TangentCircle(TangentCircleTool::new(3)),
            ToolKind::ArcTangent => Active::TangentArc(TangentArcTool::default()),
            ToolKind::LineMidpoint
            | ToolKind::Centerline
            | ToolKind::RectangleCenter
            | ToolKind::RectangleThreePoint
            | ToolKind::CircleTwoPoint
            | ToolKind::CircleThreePoint
            | ToolKind::ArcThreePoint
            | ToolKind::ArcCenterPoint
            | ToolKind::EllipseAxes
            | ToolKind::SlotCenterToCenter
            | ToolKind::SlotOverall
            | ToolKind::SlotCenterPoint
            | ToolKind::SlotArcThreePoint
            | ToolKind::SlotArcCenter
            | ToolKind::Point => Active::Draw(DrawTool::for_kind(kind)),
            other => Active::Modify(ModifyTool::for_kind(other)),
        }
    }

    fn tool(&mut self) -> &mut dyn Tool {
        match self {
            Active::Select(t) => t,
            Active::Node(t) => t,
            Active::Rectangle(t) => t,
            Active::Ellipse(t) => t,
            Active::Circle(t) => t,
            Active::Line(t) => t,
            Active::Polyline(t) => t,
            Active::Pen(t) => t,
            Active::Shape(t) => t,
            Active::StartPoint(t) => t,
            Active::RoundCorner(t) => t,
            Active::Reshape(t) => t,
            Active::Tangent(t) => t,
            Active::Draw(t) => t,
            Active::TangentCircle(t) => t,
            Active::TangentArc(t) => t,
            Active::Modify(t) => t,
        }
    }

    fn tool_ref(&self) -> &dyn Tool {
        match self {
            Active::Select(t) => t,
            Active::Node(t) => t,
            Active::Rectangle(t) => t,
            Active::Ellipse(t) => t,
            Active::Circle(t) => t,
            Active::Line(t) => t,
            Active::Polyline(t) => t,
            Active::Pen(t) => t,
            Active::Shape(t) => t,
            Active::StartPoint(t) => t,
            Active::RoundCorner(t) => t,
            Active::Reshape(t) => t,
            Active::Tangent(t) => t,
            Active::Draw(t) => t,
            Active::TangentCircle(t) => t,
            Active::TangentArc(t) => t,
            Active::Modify(t) => t,
        }
    }
}

/// The active tool of one document window. Switching tools cancels a gesture in progress.
#[derive(Debug, Clone)]
pub struct Editor {
    kind: ToolKind,
    active: Active,
    defaults: ShapeDefaults,
    /// Radius of the Round Corners tool, kept across tool switches.
    round_radius_mm: f64,
}

/// Corner radius the Round Corners tool starts with.
pub const DEFAULT_ROUND_RADIUS_MM: f64 = 2.0;

impl Default for Editor {
    fn default() -> Self {
        Editor::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        let defaults = ShapeDefaults::default();
        Editor {
            kind: ToolKind::Select,
            active: Active::new(ToolKind::Select, &defaults, DEFAULT_ROUND_RADIUS_MM),
            defaults,
            round_radius_mm: DEFAULT_ROUND_RADIUS_MM,
        }
    }

    pub fn tool(&self) -> ToolKind {
        self.kind
    }

    /// Cancels any gesture, then activates `kind` with a fresh state.
    pub fn select_tool(&mut self, kind: ToolKind) -> ToolStep {
        let step = self.cancel();
        self.kind = kind;
        self.active = Active::new(kind, &self.defaults, self.round_radius_mm);
        step
    }

    pub fn shape_defaults(&self) -> &ShapeDefaults {
        &self.defaults
    }

    /// Sides, points, ratio and corner radius the basic-shape tools draw with. An invalid set is
    /// refused and nothing changes. Takes effect for the next gesture; one in progress is not changed.
    pub fn set_shape_defaults(&mut self, defaults: ShapeDefaults) -> Result<(), ShapeParamError> {
        defaults.validate()?;
        self.defaults = defaults;
        if let Active::Shape(tool) = &mut self.active {
            tool.retarget(self.kind, &self.defaults);
        }
        Ok(())
    }

    pub fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        self.active.tool().input(input, ctx)
    }

    pub fn cancel(&mut self) -> ToolStep {
        self.active.tool().cancel()
    }

    /// Sets a tool setting (polygon sides, rounded-rectangle radius); `false` when the active tool
    /// has no such setting or the value is invalid. Sides and radius live in the shape defaults, so
    /// they survive a tool switch like every other option of the basic shapes.
    pub fn set_param(&mut self, param: ToolParam) -> bool {
        let defaults = match param {
            ToolParam::Sides(polygon_sides) if self.kind == ToolKind::Polygon => {
                ShapeDefaults { polygon_sides, ..self.defaults }
            }
            ToolParam::CornerRadius(corner_radius) if self.kind == ToolKind::RoundedRectangle => {
                ShapeDefaults { corner_radius, ..self.defaults }
            }
            ToolParam::Sides(_) | ToolParam::CornerRadius(_) => return false,
            other => return self.active.tool().set_param(other),
        };
        self.set_shape_defaults(defaults).is_ok()
    }

    pub fn exact_size(&mut self, width: Length, height: Length) -> ToolStep {
        self.active.tool().exact_size(width, height)
    }

    /// Starts the session of a tool that works on the selection as a whole (Warp, Deform): its
    /// handles appear. `Nothing` for every other tool; a refused selection is `Rejected`.
    pub fn begin_tool(&mut self, ctx: &ToolContext<'_>) -> ToolStep {
        match &mut self.active {
            Active::Reshape(t) => t.begin(ctx),
            _ => ToolStep::Nothing,
        }
    }

    /// Warp/Deform: every handle back on its start position.
    pub fn reshape_reset(&mut self, ctx: &ToolContext<'_>) -> ToolStep {
        match &mut self.active {
            Active::Reshape(t) => t.reset(ctx),
            _ => ToolStep::Nothing,
        }
    }

    /// Warp/Deform: applies the handles as they are (the Enter key does the same).
    pub fn reshape_apply(&mut self) -> ToolStep {
        match &mut self.active {
            Active::Reshape(t) => t.apply(),
            _ => ToolStep::Nothing,
        }
    }

    /// Selected nodes in node-edit mode; empty for every other tool.
    pub fn node_selection(&self) -> &[NodeRef] {
        match &self.active {
            Active::Node(t) => t.selected_nodes(),
            _ => &[],
        }
    }

    /// Runs a node-edit command on the selected nodes (`Nothing` outside node-edit mode).
    pub fn node_command(&mut self, cmd: NodeCommand, ctx: &ToolContext<'_>) -> ToolStep {
        match &mut self.active {
            Active::Node(t) => t.command(cmd, ctx),
            _ => ToolStep::Nothing,
        }
    }

    /// Whether the node-edit button for `cmd` can act on the current selection.
    pub fn node_command_enabled(&self, cmd: NodeCommand, ctx: &ToolContext<'_>) -> bool {
        matches!(&self.active, Active::Node(t) if t.command_enabled(cmd, ctx))
    }

    /// The Round Corners radius (mm); applies to the active tool at once and to later activations.
    pub fn set_round_radius(&mut self, radius: Length) {
        self.round_radius_mm = radius.mm();
        if let Active::RoundCorner(t) = &mut self.active {
            t.set_radius_mm(self.round_radius_mm);
        }
    }

    pub fn round_radius(&self) -> f64 {
        self.round_radius_mm
    }

    pub fn is_gesture_active(&self) -> bool {
        self.active.tool_ref().is_active()
    }

    pub fn preview(&self) -> Option<ToolPreview> {
        self.active.tool_ref().preview()
    }

    pub fn snap(&self) -> Option<SnapCandidate> {
        self.active.tool_ref().snap()
    }

    /// The selection grip under the pointer (select tool only); drawn larger.
    pub fn hovered_handle(&self) -> Option<Handle> {
        match &self.active {
            Active::Select(t) => t.hovered_handle(),
            _ => None,
        }
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    use std::time::Duration;

    use burnburn_document::{DocumentSnapshot, GeometryObject, GeometrySource, ObjectKind};
    use burnburn_geometry::{Path, Point, Rect};
    use burnburn_types::{GeometryId, Length, MonotonicTime, StableId};

    use super::{ToolContext, ToolStep};
    use crate::input::{CanvasInput, CanvasKey, KeyPhase, Modifiers, PointerButton, PointerEvent, PointerPhase};
    use crate::selection::{Selection, SelectionChange};
    use crate::snap::SnapSettings;

    pub struct Fixture {
        pub snapshot: DocumentSnapshot,
        pub selection: Selection,
        pub mm_per_point: f64,
        pub snap: SnapSettings,
    }

    impl Fixture {
        /// 0.5 mm per point (2 pt/mm), object snapping off unless a test turns it on.
        pub fn new(objects: Vec<GeometryObject>) -> Self {
            let next = objects.iter().map(|o| o.id().raw()).max().unwrap_or(0) + 1;
            Fixture {
                snapshot: DocumentSnapshot::from_parts(objects, next).unwrap(),
                selection: Selection::default(),
                mm_per_point: 0.5,
                snap: SnapSettings { objects: false, ..SnapSettings::default() },
            }
        }

        pub fn ctx(&self) -> ToolContext<'_> {
            ToolContext {
                document: &self.snapshot,
                selection: &self.selection,
                mm_per_point: self.mm_per_point,
                snap: &self.snap,
            }
        }

        pub fn select(&mut self, ids: &[u64]) {
            self.selection.apply(&SelectionChange::Replace(ids.iter().map(|r| GeometryId::from_raw(*r)).collect()));
        }

        /// Applies a selection step the way presentation would.
        pub fn follow(&mut self, step: &ToolStep) {
            if let ToolStep::Select(change) = step {
                self.selection.apply(change);
            }
        }
    }

    pub fn none() -> Modifiers {
        Modifiers::default()
    }

    pub fn shift() -> Modifiers {
        Modifiers { shift: true, ..none() }
    }

    pub fn alt() -> Modifiers {
        Modifiers { alt: true, ..none() }
    }

    pub fn command() -> Modifiers {
        Modifiers { command: true, ..none() }
    }

    pub fn at(phase: PointerPhase, x: f64, y: f64, mods: Modifiers) -> CanvasInput {
        let mut ev = PointerEvent::mouse(phase, Point::new(x, y), MonotonicTime::from_offset(Duration::from_millis(1)));
        ev.modifiers = mods;
        CanvasInput::Pointer(ev)
    }

    pub fn hover(x: f64, y: f64) -> CanvasInput {
        let mut ev = PointerEvent::mouse(
            PointerPhase::Move,
            Point::new(x, y),
            MonotonicTime::from_offset(Duration::from_millis(1)),
        );
        ev.button = None;
        CanvasInput::Pointer(ev)
    }

    pub fn double_click(x: f64, y: f64) -> CanvasInput {
        let mut ev = PointerEvent::mouse(
            PointerPhase::Down,
            Point::new(x, y),
            MonotonicTime::from_offset(Duration::from_millis(1)),
        );
        ev.click_count = 2;
        ev.button = Some(PointerButton::Primary);
        CanvasInput::Pointer(ev)
    }

    pub fn key(key: CanvasKey) -> CanvasInput {
        CanvasInput::Key { key, modifiers: none(), phase: KeyPhase::Down }
    }

    pub fn rect(raw: u64, x: f64, y: f64, w: f64, h: f64) -> GeometryObject {
        let r = Rect::new(Point::new(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap();
        GeometryObject::new(
            GeometryId::from_raw(raw),
            format!("r{raw}"),
            ObjectKind::Rectangle,
            GeometrySource::Drawn,
            Path::rectangle(r),
        )
    }

    pub fn path_obj(raw: u64, path: Path) -> GeometryObject {
        GeometryObject::new(GeometryId::from_raw(raw), format!("p{raw}"), ObjectKind::Path, GeometrySource::Drawn, path)
    }

    pub fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-9
    }
}
