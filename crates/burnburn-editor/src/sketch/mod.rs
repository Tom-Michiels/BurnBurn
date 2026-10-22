//! Sketch mode (W1-L1b): everything the editor knows about a parametric sketch that is not the
//! solver — where the sketch is (solved positions), what a click picks, which relations a drawn
//! shape suggests, how drawn objects become a sketch and which overlays show the sketch's
//! relations, dimensions, freedom and conflicts.
//!
//! Like the rest of the editor this reads data and returns data: the document is changed only by
//! the `SketchCommand` presentation builds from a [`ShapePlan`].

mod convert;
mod draw;
mod geometry;
mod overlays;
mod pick;

pub use convert::sketch_from_paths;
pub use draw::{PlanError, ShapePlan, Suggestion, SuggestionKind, plan_commit, suggest_preview};
pub use geometry::SketchGeometry;
pub(crate) use geometry::ellipse_distance;
pub use overlays::{PointFreedom, SketchGlyph, SketchMark, SketchOverlays, SketchView, glyph_of, sketch_overlays};
pub use pick::{pick_constraint, pick_entity};
