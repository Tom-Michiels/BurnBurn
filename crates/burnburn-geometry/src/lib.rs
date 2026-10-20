//! Canonical 2D geometry in millimetres (`f64`), with typed errors and explicit tolerances.
//!
//! Coordinates are y-up (mathematical orientation): a counter-clockwise contour has positive
//! signed area. This crate has no GUI, GPU, filesystem or network dependency.

mod affine;
mod align;
pub mod array;
mod boolean;
mod bounds;
mod canonical;
mod construct;
mod corner;
mod cut;
mod cutting;
mod dock;
mod error;
mod fillet;
mod flatten;
mod intersect;
mod join;
pub mod math;
mod node;
mod node_ops;
mod param;
mod path;
mod path_op;
mod point;
mod query;
mod registration;
mod repair;
mod ring;
mod segment;
mod shapes;
mod simplify;
mod slots;
mod snap;
mod spatial;
mod spline;
mod tangent;
mod together;
mod tolerance;
mod warp;

pub use affine::Affine;
pub use align::{AlignEdge, AlignError, AlignTo, Direction, Spacing, align_offsets, distribute_offsets};
pub use boolean::{BooleanError, BooleanOp, BooleanOptions, FillRule, OffsetError, OffsetJoin, boolean, offset};
pub use bounds::Bounds;
pub use construct::{
    Circle, ConstructError, EllipseSpec, Line, PolygonSpec, arc_center_start_end, arc_tangent, arc_through_three,
    circle_tangent_three_lines, circle_tangent_two_lines, circle_tangent_two_lines_radius, circle_through_three,
    ellipse_from_axes, polygon_circumscribed, polygon_from_edge, polygon_inscribed, rectangle_three_point,
    rounded_rectangle, slot_along_arc, slot_center_to_center, slot_overall,
};
pub use corner::{CornerCut, CornerError};
pub use cut::{CutError, CutPiece, CutResult, CutSide, cut_shapes};
pub use cutting::BreakError;
pub use dock::{DockContact, DockDirection, DockError, dock_offset};
pub use error::GeometryError;
pub use fillet::{RoundError, RoundOutcome};
pub use flatten::Polyline;
pub use intersect::Intersection;
pub use join::{JoinError, JoinOutcome, join_shapes};
pub use node::{
    EndpointRef, HandleMode, HandleRef, HandleSide, NodeEdit, NodeEditError, NodeKind, NodeRef, SegmentKind,
};
pub use param::{MAX_SIDES, ParametricShape, ShapeParamError, ShapeParams};
pub use path::{Path, PathBuilder, Rect, Subpath, Winding};
pub use path_op::{NodeSetOp, PathOp, PathOpError};
pub use point::{Point, Vector};
pub use query::{HitKind, Nearest, SegmentRef};
pub use registration::{
    CircleFit, CircleFitError, Registration, RegistrationError, RegistrationOptions, ScaleMode, fit_circle, register,
};
pub use repair::{ExtendError, GeometryIssue, IssueKind, TrimError, close_gaps, diagnose};
pub use segment::{EllipticalArc, Segment};
pub use shapes::Shape;
pub use simplify::{OptimizeError, OptimizeOutcome};
pub use slots::{ResizeSlots, SlotSelect, SlotsError, SlotsOutcome};
pub use snap::{SnapCandidate, SnapFilter, SnapKind, snap_candidates};
pub use spatial::{HitCandidate, SpatialIndex, hit_test};
pub use tangent::{TangentError, TangentTarget, nearest_circle, tangent_circles, tangent_circles_radius};
pub use together::{TogetherAnchor, TogetherError, move_together_offsets};
pub use tolerance::{
    DiagnosticTolerance, DisplayTolerance, FlatteningTolerance, HitTestTolerance, ProductionTolerance, ToleranceError,
};
pub use warp::{DeformGrid, PointMap, QuadWarp, WarpError, WarpKind, warp_path};
