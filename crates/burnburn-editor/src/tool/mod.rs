//! Tool state machines. A tool reads the immutable snapshot and window selection, keeps its own
//! temporary preview, and returns at most one [`EditCommit`] per finished gesture. Presentation
//! turns that commit into exactly one `DocumentCommand` with the expected revision (contract §4).

mod draw;
mod node;
mod parametric;
mod pick;
mod rectangle;
mod round_corner;
mod select;
mod start_point;

use std::sync::Arc;

use burnburn_document::{DocumentSnapshot, OpsCommand};
use burnburn_geometry::{
    Affine, Bounds, NodeEdit, NodeRef, ParametricShape, Path, Point, Rect, Shape, ShapeParamError,
};
use burnburn_types::{GeometryId, Length};

use crate::input::{CanvasInput, CanvasKey, KeyPhase, Modifiers, PointerButton, PointerEvent, PointerPhase};
use crate::node_ops::NodeCommand;
use crate::selection::{Selection, SelectionChange};
use crate::snap::{SnapCandidate, SnapSettings, snap_point};

pub use draw::{CircleTool, EllipseTool, LineTool, PenTool, PolylineTool};
pub use node::{NodeTarget, NodeTool, path_nodes};
pub use parametric::{BasicShapeTool, ShapeDefaults};
pub(crate) use rectangle::rect_from_bounds;
pub use rectangle::{RectanglePreview, RectangleTool};
pub use round_corner::RoundCornerTool;
pub use select::{Handle, SelectTool, selection_handles};
pub use start_point::StartPointTool;

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
    /// Basic shapes (W1-L2b); sides, points, ratio and radius come from [`ShapeDefaults`].
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

    pub fn exact_size(&mut self, width: Length, height: Length) -> ToolStep {
        self.active.tool().exact_size(width, height)
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
        self.preview().is_some()
    }

    pub fn preview(&self) -> Option<ToolPreview> {
        self.active.tool_ref().preview()
    }

    pub fn snap(&self) -> Option<SnapCandidate> {
        self.active.tool_ref().snap()
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
