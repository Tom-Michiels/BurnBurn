//! Rust-native contour tracing (IM12).
//!
//! Pipeline: [`burnburn_image::prepare`] (binary mask + signed sub-pixel field) → marching squares
//! on pixel centres with a fixed saddle policy → contour tree from connected components (outer
//! contours, holes, islands) → corner detection and snapping → line/cubic fitting within a distance
//! budget → `burnburn_geometry` paths in millimetres (y-up, origin at the bottom-left corner of the
//! adjusted image) at the declared physical scale.
//!
//! The algorithms are written from the published descriptions of marching squares and of
//! Schneider's least-squares Bézier fitting (Graphics Gems, 1990); no tracer source code was used.

mod centerline;
mod colour;
mod contour;
mod correct;
mod document;
mod error;
mod fit;
mod preview;
mod recipe;
mod recipes;
mod result;

pub use centerline::{
    CENTERLINE_ENGINE_VERSION, CenterlineDecision, CenterlineEditError, CenterlineRequest, CenterlineResult,
    CenterlineSettings, Chain, ChainEnd, ChainId, ChainOrigin, Connections, GraphNode, NearContact, NodeId, NodeKind,
    Pairing, Primitive, PrimitiveCandidate, RemovedSpur, ReplacementProposal, Replayed, SkipReason, SkippedDecision,
    Stroke, trace_centerline,
};
pub use colour::{
    BoundaryEdge, BoundaryId, BoundaryUse, COLOUR_ENGINE_VERSION, ColourRequest, Palette, Region, RegionGraph,
    RegionId, SharedEdgePolicy, colour_regions, quantize,
};
pub use contour::{ContourKind, DiagonalPolicy};
pub use correct::{
    Anchor, ContourBinding, CorrectionId, CorrectionRecord, CorrectionRequest, NodeTarget, NodeTargetOutcome,
    correction_chain_key, retrace_region,
};
pub use document::{
    centerline_command, centerline_decisions_from_engine, centerline_from_engine, centerline_to_engine,
    corrections_from_engine, recipe_from_engine, recipe_to_engine, trace_command,
};
pub use error::{StaleTrace, TraceError};
pub use preview::{
    ComparisonPreview, DeviationMap, DeviationSummary, SegmentDeviation, comparison_preview, deviation_map,
    deviation_summary,
};
pub use recipe::{ContourSettings, ENGINE_VERSION, FitSettings, TraceRecipe};
pub use recipes::{
    BatchItem, BatchOutcome, BatchProblem, BatchReport, CompareCorrections, CorrectionReplay, Reapplied,
    RecipeComparison, RecipeSummary, UnboundCorrection, compare_recipes, compare_recipes_corrected, reapply_correction,
    replay_corrections, trace_batch,
};
pub use result::{
    ContourId, ContourOrigin, RemovedFeature, TraceObserver, TraceRequest, TraceResult, TraceStage, TraceStats,
    TracedComponent, TracedContour, trace, trace_observed,
};
