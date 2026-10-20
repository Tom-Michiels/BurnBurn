//! CAM compiler (IM13) and preflight (IM15): document snapshot → immutable [`JobPlan`].
//!
//! The compiler reads a [`DocumentSnapshot`] (design geometry + process layer) and a
//! [`MachineProfile`], and produces one frozen plan. It never mutates the document. Steps:
//!
//! 1. Execution order from the process layer (hard constraints first, list order otherwise).
//! 2. Production flattening of every referenced geometry with an explicit
//!    [`ProductionTolerance`](burnburn_geometry::ProductionTolerance).
//! 3. Placement (absolute or anchored) and the machine frame (origin corner).
//! 4. Line planning: inner contours before the contours that contain them, otherwise nearest
//!    neighbour from the current head position with a fixed tie-break (input order).
//! 5. Fill planning: scan rows at the line interval (row centres), even-odd spans,
//!    bidirectional or unidirectional, with overscan recorded on each run.
//!
//! Kerf compensation offsets each Line target by `kerf / 2` with `geometry::offset` (outer
//! contours outward, holes inward); open contours with kerf are rejected
//! ([`CompileError::KerfOnOpenContour`]) instead of being cut at nominal position.
//!
//! [`JobPlan`]: burnburn_job::JobPlan
//! [`DocumentSnapshot`]: burnburn_document::DocumentSnapshot
//! [`MachineProfile`]: burnburn_job::MachineProfile

mod array;
mod compile;
mod error;
mod features;
mod fill;
mod image;
mod line;
pub mod nest;
mod offset_fill;
mod options;
mod overlap;
mod preflight;
mod source;

pub use array::{ArrayError, CircularArray, GridArray, MAX_COPIES, circular_array, grid_array};
pub use compile::{COMPILER_VERSION, compile, compile_with};
pub use error::CompileError;
pub use features::required_features;
pub use image::{NoRasters, RasterSource, processed_bitmap};
pub use options::{CompileOptions, JobAnchor, Placement, StartPoint};
pub use preflight::{Finding, PreflightReport, Severity, SizeCheck, preflight};
pub use source::source_hash;
