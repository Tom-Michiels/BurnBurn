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
mod error;
mod flatten;
mod intersect;
pub mod math;
mod node;
mod path;
mod point;
mod query;
mod repair;
mod segment;
mod shapes;
mod snap;
mod spatial;
mod tolerance;

pub use affine::Affine;
pub use align::{AlignEdge, AlignError, AlignTo, Direction, Spacing, align_offsets, distribute_offsets};
pub use boolean::{BooleanError, BooleanOp, BooleanOptions, FillRule, OffsetError, OffsetJoin, boolean, offset};
pub use bounds::Bounds;
pub use error::GeometryError;
pub use flatten::Polyline;
pub use intersect::Intersection;
pub use node::{EndpointRef, HandleMode, HandleRef, HandleSide, NodeEdit, NodeEditError, NodeRef};
pub use path::{Path, PathBuilder, Rect, Subpath, Winding};
pub use point::{Point, Vector};
pub use query::{HitKind, Nearest, SegmentRef};
pub use repair::{ExtendError, GeometryIssue, IssueKind, TrimError, close_gaps, diagnose};
pub use segment::{EllipticalArc, Segment};
pub use shapes::Shape;
pub use snap::{SnapCandidate, SnapFilter, SnapKind, snap_candidates};
pub use spatial::{HitCandidate, SpatialIndex, hit_test};
pub use tolerance::{
    DiagnosticTolerance, DisplayTolerance, FlatteningTolerance, HitTestTolerance, ProductionTolerance, ToleranceError,
};
