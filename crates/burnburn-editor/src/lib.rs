//! Shared editor interaction: normalised canvas input, camera math, tool state machines,
//! snapping, window-local selection, arrange helpers and the scene snapshot.
//!
//! The same code runs behind the AppKit host, the Windows host and the headless harness. It holds
//! no platform widgets, no renderer and never mutates the document: a finished gesture is returned
//! as exactly one semantic [`EditCommit`] in [`ToolStep::Commit`]; presentation turns it into one
//! `DocumentCommand` with the expected revision (contract §4).

mod arrange;
mod input;
mod issues;
mod scene;
mod selection;
mod snap;
mod tool;
mod view;

pub use arrange::{ArrangeError, boolean_operands};
pub use input::{CanvasInput, CanvasKey, KeyPhase, Modifiers, PointerButton, PointerEvent, PointerId, PointerPhase};
pub use issues::{GeometryIssue, IssueKind, NEARLY_CLOSED_MM, geometry_issues, open_contours, self_intersections};
pub use scene::{
    MIN_GRID_SPACING_PT, Overlay, SceneItem, SceneRequest, SceneSnapshot, StyleRole, build_scene, grid_spacing_mm,
};
pub use selection::{Selection, SelectionChange, hit_test, next_in_cycle, objects_inside};
pub use snap::{SnapCandidate, SnapKind, SnapSettings, snap_candidates, snap_point};
pub use tool::{
    CLICK_TOLERANCE_PT, CircleTool, DRAG_THRESHOLD_PT, EditCommit, Editor, EllipseTool, HANDLE_RADIUS_PT, Handle,
    LineTool, NodeTarget, NodeTool, PenTool, PolylineTool, ROTATE_HANDLE_OFFSET_PT, RectanglePreview, RectangleTool,
    SelectTool, ShapeKind, Tool, ToolContext, ToolKind, ToolPreview, ToolStep, TransformKind, path_nodes,
    selection_handles,
};
pub use view::{Camera, ViewError, Viewport};
