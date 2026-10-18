//! Operand checks for the selection-wide geometry commands (W1-L2a): Cut Shapes, Join/Close and
//! the path operations (round corners, optimize, direction). They decide enablement and which
//! objects the `OpsCommand` names; the geometry itself is a domain operation.

use burnburn_document::DocumentSnapshot;
use burnburn_types::GeometryId;

use crate::arrange::ArrangeError;
use crate::selection::Selection;

fn existing_with_geometry(
    snapshot: &DocumentSnapshot,
    id: GeometryId,
) -> Result<&burnburn_document::GeometryObject, ArrangeError> {
    let obj = snapshot.object(id).ok_or(ArrangeError::UnknownObject(id))?;
    if obj.path().subpaths().is_empty() { Err(ArrangeError::EmptyGeometry(id)) } else { Ok(obj) }
}

/// Subjects and cutter for Cut Shapes: the **last selected** object is the cutter (it must be
/// closed), every other selected object is cut by it.
pub fn cut_operands(
    snapshot: &DocumentSnapshot,
    selection: &Selection,
) -> Result<(Vec<GeometryId>, GeometryId), ArrangeError> {
    let ids = selection.ids();
    if ids.len() < 2 {
        return Err(ArrangeError::TooFewObjects { needed: 2 });
    }
    for id in ids {
        existing_with_geometry(snapshot, *id)?;
    }
    let (cutter, subjects) = ids.split_last().ok_or(ArrangeError::TooFewObjects { needed: 2 })?;
    if snapshot.object(*cutter).is_some_and(|o| o.path().subpaths().iter().any(|s| !s.is_closed())) {
        return Err(ArrangeError::OpenContour(*cutter));
    }
    Ok((subjects.to_vec(), *cutter))
}

/// Objects to join or close: at least one, at least one open path among them.
pub fn join_operands(snapshot: &DocumentSnapshot, selection: &Selection) -> Result<Vec<GeometryId>, ArrangeError> {
    let ids = selection.ids();
    if ids.is_empty() {
        return Err(ArrangeError::TooFewObjects { needed: 1 });
    }
    let mut any_open = false;
    for id in ids {
        let obj = existing_with_geometry(snapshot, *id)?;
        any_open |= obj.path().subpaths().iter().any(|s| !s.is_closed());
    }
    if any_open { Ok(ids.to_vec()) } else { Err(ArrangeError::NoOpenPath) }
}

/// Objects for a path operation: at least one, each existing and with geometry.
pub fn path_op_operands(snapshot: &DocumentSnapshot, selection: &Selection) -> Result<Vec<GeometryId>, ArrangeError> {
    let ids = selection.ids();
    if ids.is_empty() {
        return Err(ArrangeError::TooFewObjects { needed: 1 });
    }
    for id in ids {
        existing_with_geometry(snapshot, *id)?;
    }
    Ok(ids.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::SelectionChange;
    use crate::tool::fixture::{path_obj, rect};
    use burnburn_geometry::{Path, Point};
    use burnburn_types::StableId;

    fn id(raw: u64) -> GeometryId {
        GeometryId::from_raw(raw)
    }

    fn sel(ids: &[u64]) -> Selection {
        let mut s = Selection::default();
        s.apply(&SelectionChange::Replace(ids.iter().map(|r| id(*r)).collect()));
        s
    }

    fn snapshot() -> DocumentSnapshot {
        let open = Path::line(Point::new(0.0, 0.0), Point::new(5.0, 5.0)).unwrap();
        DocumentSnapshot::from_parts(
            vec![
                rect(1, 0.0, 0.0, 10.0, 10.0),
                rect(2, 5.0, 5.0, 10.0, 10.0),
                path_obj(3, open),
                path_obj(4, Path::default()),
            ],
            5,
        )
        .unwrap()
    }

    #[test]
    fn the_last_selected_closed_object_is_the_cutter() {
        let snap = snapshot();
        assert_eq!(cut_operands(&snap, &sel(&[3, 1, 2])), Ok((vec![id(3), id(1)], id(2))));
        assert_eq!(cut_operands(&snap, &sel(&[1])), Err(ArrangeError::TooFewObjects { needed: 2 }));
        assert_eq!(
            cut_operands(&snap, &sel(&[1, 3])),
            Err(ArrangeError::OpenContour(id(3))),
            "an open cutter cannot cut"
        );
        assert_eq!(cut_operands(&snap, &sel(&[1, 9])), Err(ArrangeError::UnknownObject(id(9))));
        assert_eq!(cut_operands(&snap, &sel(&[1, 4])), Err(ArrangeError::EmptyGeometry(id(4))));
    }

    #[test]
    fn join_needs_at_least_one_open_path() {
        let snap = snapshot();
        assert_eq!(join_operands(&snap, &sel(&[3])), Ok(vec![id(3)]));
        assert_eq!(join_operands(&snap, &sel(&[1, 3])), Ok(vec![id(1), id(3)]));
        assert_eq!(join_operands(&snap, &sel(&[1, 2])), Err(ArrangeError::NoOpenPath));
        assert_eq!(join_operands(&snap, &sel(&[])), Err(ArrangeError::TooFewObjects { needed: 1 }));
        assert_eq!(join_operands(&snap, &sel(&[9])), Err(ArrangeError::UnknownObject(id(9))));
    }

    #[test]
    fn path_operations_need_existing_objects_with_geometry() {
        let snap = snapshot();
        assert_eq!(path_op_operands(&snap, &sel(&[2, 1])), Ok(vec![id(2), id(1)]));
        assert_eq!(path_op_operands(&snap, &sel(&[])), Err(ArrangeError::TooFewObjects { needed: 1 }));
        assert_eq!(path_op_operands(&snap, &sel(&[4])), Err(ArrangeError::EmptyGeometry(id(4))));
        assert_eq!(path_op_operands(&snap, &sel(&[9])), Err(ArrangeError::UnknownObject(id(9))));
    }
}
