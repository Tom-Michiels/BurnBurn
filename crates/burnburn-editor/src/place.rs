//! Operand checks and helpers for the placement and reshaping commands (W2-L2): Dock, Move
//! Together, Warp 4 points, Deform 16 points and Resize Slots. They decide enablement and which
//! objects the command names; the geometry itself is a domain operation.

use burnburn_document::DocumentSnapshot;
use burnburn_geometry::{Bounds, Path, Point, PointMap, ProductionTolerance, WarpError, warp_path};
use burnburn_types::{CancellationToken, GeometryId, RequestId};
use std::fmt;

use crate::arrange::ArrangeError;
use crate::selection::Selection;

/// Which selected object is the reference of a dock (it stays; the others move).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceChoice {
    FirstSelected,
    LastSelected,
    Object(GeometryId),
}

fn with_geometry(
    snapshot: &DocumentSnapshot,
    id: GeometryId,
) -> Result<&burnburn_document::GeometryObject, ArrangeError> {
    let obj = snapshot.object(id).ok_or(ArrangeError::UnknownObject(id))?;
    if obj.path().subpaths().is_empty() { Err(ArrangeError::EmptyGeometry(id)) } else { Ok(obj) }
}

/// Targets and reference of a dock, from at least two selected objects. The targets keep their
/// selection order.
pub fn dock_operands(
    snapshot: &DocumentSnapshot,
    selection: &Selection,
    choice: ReferenceChoice,
) -> Result<(Vec<GeometryId>, GeometryId), ArrangeError> {
    let ids = selection.ids();
    if ids.len() < 2 {
        return Err(ArrangeError::TooFewObjects { needed: 2 });
    }
    for id in ids {
        with_geometry(snapshot, *id)?;
    }
    let reference = match choice {
        ReferenceChoice::FirstSelected => ids[0],
        ReferenceChoice::LastSelected => ids[ids.len() - 1],
        ReferenceChoice::Object(id) if ids.contains(&id) => id,
        ReferenceChoice::Object(id) => return Err(ArrangeError::ReferenceNotSelected(id)),
    };
    Ok((ids.iter().copied().filter(|id| *id != reference).collect(), reference))
}

/// Objects for Move Together: at least two, each existing and with geometry.
pub fn move_together_operands(
    snapshot: &DocumentSnapshot,
    selection: &Selection,
) -> Result<Vec<GeometryId>, ArrangeError> {
    let ids = selection.ids();
    if ids.len() < 2 {
        return Err(ArrangeError::TooFewObjects { needed: 2 });
    }
    for id in ids {
        with_geometry(snapshot, *id)?;
    }
    Ok(ids.to_vec())
}

/// The selection plus every other member of the outermost group of each selected object, so a
/// grouped object docks or moves with its group. Selection order first, then the added members by id.
pub fn expand_to_groups(snapshot: &DocumentSnapshot, ids: &[GeometryId]) -> Vec<GeometryId> {
    let mut out = ids.to_vec();
    for id in ids {
        if let Some(group) = snapshot.object(*id).and_then(|o| o.groups().first().copied()) {
            for member in snapshot.group_members(group) {
                if !out.contains(&member) {
                    out.push(member);
                }
            }
        }
    }
    out
}

/// Objects and common bounding box for Warp 4 points and Deform 16 points. The box needs an area.
pub fn reshape_operands(
    snapshot: &DocumentSnapshot,
    selection: &Selection,
) -> Result<(Vec<GeometryId>, Bounds), ArrangeError> {
    let ids = selection.ids();
    if ids.is_empty() {
        return Err(ArrangeError::TooFewObjects { needed: 1 });
    }
    let mut union: Option<Bounds> = None;
    for id in ids {
        let b = with_geometry(snapshot, *id)?.bounds().ok_or(ArrangeError::EmptyGeometry(*id))?;
        union = Some(union.map_or(b, |u| u.union(b)));
    }
    let bounds = union.ok_or(ArrangeError::TooFewObjects { needed: 1 })?;
    if bounds.width() > 0.0 && bounds.height() > 0.0 {
        Ok((ids.to_vec(), bounds))
    } else {
        Err(ArrangeError::FlatSelection)
    }
}

/// The handles Warp 4 points starts from: the corners of the box, `[bottom-left, bottom-right,
/// top-right, top-left]`.
pub fn warp_start_corners(bounds: &Bounds) -> [Point; 4] {
    let (min, max) = (bounds.min(), bounds.max());
    [min, Point::new(max.x, min.y), max, Point::new(min.x, max.y)]
}

/// Why a preview could not be computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PreviewError {
    UnknownObject(GeometryId),
    Warp(WarpError),
}

impl fmt::Display for PreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreviewError::UnknownObject(id) => write!(f, "no object {id}"),
            PreviewError::Warp(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for PreviewError {}

/// A warp or deform preview for the objects named; `request` identifies the computation so the caller
/// can drop results that were overtaken by a newer drag.
#[derive(Debug, Clone, PartialEq)]
pub struct WarpPreview {
    pub request: RequestId,
    pub items: Vec<(GeometryId, Path)>,
}

/// Warps the objects without touching the document. Cancellable; an error returns nothing.
pub fn warp_preview(
    snapshot: &DocumentSnapshot,
    request: RequestId,
    ids: &[GeometryId],
    map: &impl PointMap,
    tolerance: &ProductionTolerance,
    cancel: &CancellationToken,
) -> Result<WarpPreview, PreviewError> {
    let mut items = Vec::with_capacity(ids.len());
    for id in ids {
        let obj = snapshot.object(*id).ok_or(PreviewError::UnknownObject(*id))?;
        let path = warp_path(obj.path(), map, tolerance, cancel).map_err(PreviewError::Warp)?;
        items.push((*id, path));
    }
    Ok(WarpPreview { request, items })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::SelectionChange;
    use crate::tool::fixture::{path_obj, rect};
    use burnburn_geometry::{QuadWarp, WarpKind};
    use burnburn_types::{CancellationSource, GroupId, StableId};

    fn id(raw: u64) -> GeometryId {
        GeometryId::from_raw(raw)
    }

    fn sel(ids: &[u64]) -> Selection {
        let mut s = Selection::default();
        s.apply(&SelectionChange::Replace(ids.iter().map(|r| id(*r)).collect()));
        s
    }

    fn snapshot() -> DocumentSnapshot {
        let flat = Path::line(Point::new(0.0, 0.0), Point::new(0.0, 9.0)).unwrap();
        let grouped = |raw, x| rect(raw, x, 0.0, 2.0, 2.0).with_groups(vec![GroupId::from_raw(50)]);
        DocumentSnapshot::from_parts(
            vec![
                rect(1, 0.0, 0.0, 10.0, 10.0),
                rect(2, 30.0, 0.0, 10.0, 10.0),
                path_obj(3, flat),
                path_obj(4, Path::default()),
                grouped(5, 60.0),
                grouped(6, 70.0),
                grouped(7, 80.0),
            ],
            60,
        )
        .unwrap()
    }

    #[test]
    fn the_reference_of_a_dock_is_chosen_explicitly_and_the_rest_move() {
        let snap = snapshot();
        let selected = sel(&[2, 1, 5]);
        assert_eq!(dock_operands(&snap, &selected, ReferenceChoice::FirstSelected), Ok((vec![id(1), id(5)], id(2))));
        assert_eq!(dock_operands(&snap, &selected, ReferenceChoice::LastSelected), Ok((vec![id(2), id(1)], id(5))));
        assert_eq!(dock_operands(&snap, &selected, ReferenceChoice::Object(id(1))), Ok((vec![id(2), id(5)], id(1))));
        assert_eq!(
            dock_operands(&snap, &selected, ReferenceChoice::Object(id(6))),
            Err(ArrangeError::ReferenceNotSelected(id(6)))
        );
        assert_eq!(
            dock_operands(&snap, &sel(&[1]), ReferenceChoice::FirstSelected),
            Err(ArrangeError::TooFewObjects { needed: 2 })
        );
        assert_eq!(
            dock_operands(&snap, &sel(&[1, 4]), ReferenceChoice::FirstSelected),
            Err(ArrangeError::EmptyGeometry(id(4)))
        );
        assert_eq!(
            dock_operands(&snap, &sel(&[1, 9]), ReferenceChoice::FirstSelected),
            Err(ArrangeError::UnknownObject(id(9)))
        );
    }

    #[test]
    fn move_together_needs_two_existing_objects_with_geometry() {
        let snap = snapshot();
        assert_eq!(move_together_operands(&snap, &sel(&[2, 1])), Ok(vec![id(2), id(1)]));
        assert_eq!(move_together_operands(&snap, &sel(&[1])), Err(ArrangeError::TooFewObjects { needed: 2 }));
        assert_eq!(move_together_operands(&snap, &sel(&[1, 4])), Err(ArrangeError::EmptyGeometry(id(4))));
        assert_eq!(move_together_operands(&snap, &sel(&[1, 9])), Err(ArrangeError::UnknownObject(id(9))));
    }

    #[test]
    fn grouped_objects_bring_their_group_along_when_asked() {
        let snap = snapshot();
        assert_eq!(expand_to_groups(&snap, &[id(6), id(1)]), vec![id(6), id(1), id(5), id(7)]);
        assert_eq!(expand_to_groups(&snap, &[id(1), id(2)]), vec![id(1), id(2)], "ungrouped objects stay as they are");
        assert_eq!(expand_to_groups(&snap, &[id(5), id(6), id(7)]), vec![id(5), id(6), id(7)]);
        assert_eq!(expand_to_groups(&snap, &[id(9)]), vec![id(9)], "unknown ids are left for the command to report");
    }

    #[test]
    fn warp_and_deform_need_a_selection_with_an_area() {
        let snap = snapshot();
        let (ids, b) = reshape_operands(&snap, &sel(&[2, 1])).unwrap();
        assert_eq!(ids, vec![id(2), id(1)]);
        assert_eq!((b.min(), b.max()), (Point::new(0.0, 0.0), Point::new(40.0, 10.0)));
        assert_eq!(reshape_operands(&snap, &sel(&[3])), Err(ArrangeError::FlatSelection));
        assert_eq!(reshape_operands(&snap, &sel(&[])), Err(ArrangeError::TooFewObjects { needed: 1 }));
        assert_eq!(reshape_operands(&snap, &sel(&[4])), Err(ArrangeError::EmptyGeometry(id(4))));
        assert_eq!(reshape_operands(&snap, &sel(&[9])), Err(ArrangeError::UnknownObject(id(9))));
        assert_eq!(
            warp_start_corners(&b),
            [Point::new(0.0, 0.0), Point::new(40.0, 0.0), Point::new(40.0, 10.0), Point::new(0.0, 10.0)]
        );
    }

    #[test]
    fn a_preview_warps_without_touching_the_document_and_carries_its_request() {
        let snap = snapshot();
        let (ids, b) = reshape_operands(&snap, &sel(&[1, 2])).unwrap();
        let mut corners = warp_start_corners(&b);
        corners[2] = Point::new(30.0, 25.0);
        let map = QuadWarp::new(b, corners, WarpKind::Projective).unwrap();
        let tol = ProductionTolerance::new(burnburn_types::Length::from_mm(0.001).unwrap()).unwrap();
        let request = RequestId::from_raw(41);
        let preview = warp_preview(&snap, request, &ids, &map, &tol, &CancellationToken::never()).unwrap();
        assert_eq!(preview.request, request);
        assert_eq!(preview.items.len(), 2);
        assert_eq!(preview.items[0].0, id(1));
        // The second rectangle's top-right corner is the corner of the common box: it lands on the handle.
        let second = &preview.items[1].1.subpaths()[0];
        let vertices: Vec<Point> = std::iter::once(second.start())
            .chain(second.segments().iter().map(burnburn_geometry::Segment::end))
            .collect();
        assert!(vertices.iter().any(|v| v.distance(Point::new(30.0, 25.0)) < 1e-9), "{vertices:?}");
        // The snapshot still has the original outline.
        assert_eq!(snap.object(id(1)).unwrap().bounds().unwrap().max(), Point::new(10.0, 10.0));

        let source = CancellationSource::new();
        source.cancel();
        assert_eq!(
            warp_preview(&snap, request, &ids, &map, &tol, &source.token()),
            Err(PreviewError::Warp(WarpError::Cancelled))
        );
        assert_eq!(
            warp_preview(&snap, request, &[id(9)], &map, &tol, &CancellationToken::never()),
            Err(PreviewError::UnknownObject(id(9))),
            "an unknown object has nothing to preview"
        );
    }
}
