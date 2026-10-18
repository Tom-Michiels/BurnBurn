//! Shared picking for single-click node tools: the anchor under the pointer on the selected
//! object, or else a plain object selection.

use burnburn_geometry::NodeRef;
use burnburn_types::GeometryId;

use super::{CLICK_TOLERANCE_PT, HANDLE_RADIUS_PT, ToolContext, ToolStep};
use crate::input::PointerEvent;
use crate::selection::{SelectionChange, hit_test};
use crate::tool::node::{NodeTarget, path_nodes};

/// The anchor node (not a Bézier handle) of the single selected object within the pick radius.
pub(super) fn anchor_under(ctx: &ToolContext<'_>, ev: &PointerEvent) -> Option<(GeometryId, NodeRef)> {
    let target = ctx.selection.single()?;
    let obj = ctx.document.object(target)?;
    let radius = ctx.tolerance_mm(HANDLE_RADIUS_PT);
    path_nodes(obj.path())
        .into_iter()
        .filter_map(|(t, p)| match t {
            NodeTarget::Node(n) => Some((p.distance(ev.position), n)),
            NodeTarget::Handle(_) => None,
        })
        .filter(|(d, _)| *d <= radius)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, n)| (target, n))
}

/// A press that hit no node: select the object under the pointer, or clear the selection.
pub(super) fn select_under(ctx: &ToolContext<'_>, ev: &PointerEvent) -> ToolStep {
    let hits = hit_test(ctx.document, ev.position, ctx.tolerance_mm(CLICK_TOLERANCE_PT));
    match hits.first() {
        Some(top) if ctx.selection.single() == Some(*top) => ToolStep::Nothing,
        Some(top) => ToolStep::Select(SelectionChange::Replace(vec![*top])),
        None if ctx.selection.is_empty() => ToolStep::Nothing,
        None => ToolStep::Select(SelectionChange::Clear),
    }
}
