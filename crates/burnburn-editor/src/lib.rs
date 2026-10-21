//! Shared editor interaction: normalised canvas input, camera math, tool state machines,
//! snapping, window-local selection, arrange helpers and the scene snapshot.
//!
//! The same code runs behind the AppKit host, the Windows host and the headless harness. It holds
//! no platform widgets, no renderer and never mutates the document: a finished gesture is returned
//! as exactly one semantic [`EditCommit`] in [`ToolStep::Commit`]; presentation turns it into one
//! `DocumentCommand` with the expected revision (contract §4).

mod arrange;
mod bed_image;
mod input;
mod issues;
mod node_ops;
mod operands;
mod pick_index;
mod place;
mod registration;
mod scene;
mod selection;
mod sketch;
mod snap;
mod tool;
mod view;

pub use arrange::{ArrangeError, boolean_operands};
pub use bed_image::{BedPicture, BedQuad};
pub use input::{CanvasInput, CanvasKey, KeyPhase, Modifiers, PointerButton, PointerEvent, PointerId, PointerPhase};
pub use issues::{GeometryIssue, IssueKind, NEARLY_CLOSED_MM, geometry_issues, open_contours, self_intersections};
pub use node_ops::{NodeCommand, NodeCommandError, node_command_op};
pub use operands::{cut_operands, join_operands, path_op_operands};
pub use pick_index::pick_index_builds;
pub use place::{
    PreviewError, ReferenceChoice, WarpPreview, dock_operands, expand_to_groups, move_together_operands,
    reshape_operands, warp_preview, warp_start_corners,
};
pub use registration::{CenterFinderSession, RegistrationFlowError, RegistrationSession};
pub use scene::{
    MIN_GRID_SPACING_PT, Overlay, SceneItem, SceneRequest, SceneSnapshot, StyleRole, build_scene, grid_spacing_mm,
};
pub use selection::{Selection, SelectionChange, hit_test, next_in_cycle, objects_inside};
pub use sketch::{
    PlanError, PointFreedom, ShapePlan, SketchGeometry, SketchGlyph, SketchMark, SketchOverlays, SketchView,
    Suggestion, SuggestionKind, glyph_of, pick_constraint, pick_entity, plan_commit, sketch_from_paths,
    sketch_overlays, suggest_preview,
};
pub use snap::{SnapCandidate, SnapKind, SnapSettings, snap_candidates, snap_point};
pub use tool::{
    BasicShapeTool, CLICK_TOLERANCE_PT, CircleTool, DEFAULT_ROUND_RADIUS_MM, DRAG_THRESHOLD_PT, DrawTool, EditCommit,
    Editor, EllipseTool, HANDLE_RADIUS_PT, Handle, LineTool, NodeTarget, NodeTool, PREVIEW_SEGMENT_LIMIT, PenTool,
    PolygonConstruction, PolylineTool, ROTATE_HANDLE_OFFSET_PT, RectanglePreview, RectangleTool, ReshapeMode,
    ReshapePreview, ReshapeTool, RoundCornerTool, SelectTool, ShapeDefaults, ShapeKind, StartPointTool, TangentArcTool,
    TangentCircleTool, TangentCirclesTool, TangentPreview, Tool, ToolContext, ToolKind, ToolParam, ToolPreview,
    ToolStep, TransformKind, path_nodes, selection_handles,
};
pub use tool::{ConstraintKind, constraint_relations};
pub use tool::{Measurement, ModifyParam, ModifyTool, Rejection, Unavailable, modify_tool_kind};
pub use view::{Camera, ViewError, Viewport};
