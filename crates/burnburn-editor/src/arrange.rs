//! Boolean operand checks for command enablement (E05). Align/distribute and the boolean itself
//! are domain operations (`burnburn_geometry`, `GeometryCommand`, Kern IM09).

use std::fmt;

use burnburn_document::DocumentSnapshot;
use burnburn_types::GeometryId;

use crate::selection::Selection;

/// Why an arrange or boolean command is disabled for the current selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ArrangeError {
    TooFewObjects {
        needed: usize,
    },
    /// A selected object has no geometry (no bounds).
    EmptyGeometry(GeometryId),
    /// Booleans need closed contours; this object has an open subpath.
    OpenContour(GeometryId),
    UnknownObject(GeometryId),
    /// Join needs at least one open path among the selected objects.
    NoOpenPath,
    /// The selection has no width or height, so there is nothing to map onto a quadrilateral.
    FlatSelection,
    /// The reference chosen for a dock is not part of the selection.
    ReferenceNotSelected(GeometryId),
}

impl fmt::Display for ArrangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArrangeError::TooFewObjects { needed } => write!(f, "select at least {needed} objects"),
            ArrangeError::EmptyGeometry(id) => write!(f, "object {id} has no geometry"),
            ArrangeError::OpenContour(id) => write!(f, "object {id} is not closed"),
            ArrangeError::UnknownObject(id) => write!(f, "no object {id}"),
            ArrangeError::NoOpenPath => f.write_str("none of the selected objects is an open path"),
            ArrangeError::FlatSelection => f.write_str("the selection has no width or height"),
            ArrangeError::ReferenceNotSelected(id) => write!(f, "reference {id} is not selected"),
        }
    }
}

impl std::error::Error for ArrangeError {}

/// The operands of a boolean, in selection order (difference: first minus the rest).
pub fn boolean_operands(snapshot: &DocumentSnapshot, selection: &Selection) -> Result<Vec<GeometryId>, ArrangeError> {
    if selection.len() < 2 {
        return Err(ArrangeError::TooFewObjects { needed: 2 });
    }
    for id in selection.ids() {
        let obj = snapshot.object(*id).ok_or(ArrangeError::UnknownObject(*id))?;
        let subpaths = obj.path().subpaths();
        if subpaths.is_empty() {
            return Err(ArrangeError::EmptyGeometry(*id));
        }
        if subpaths.iter().any(|sp| !sp.is_closed()) {
            return Err(ArrangeError::OpenContour(*id));
        }
    }
    Ok(selection.ids().to_vec())
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

    #[test]
    fn booleans_need_two_closed_objects() {
        let open = Path::line(Point::new(0.0, 0.0), Point::new(5.0, 5.0)).unwrap();
        let snap = DocumentSnapshot::from_parts(
            vec![rect(1, 0.0, 0.0, 10.0, 10.0), rect(2, 5.0, 5.0, 10.0, 10.0), path_obj(3, open)],
            4,
        )
        .unwrap();
        assert_eq!(boolean_operands(&snap, &sel(&[2, 1])), Ok(vec![id(2), id(1)]));
        assert_eq!(boolean_operands(&snap, &sel(&[1])), Err(ArrangeError::TooFewObjects { needed: 2 }));
        assert_eq!(boolean_operands(&snap, &sel(&[1, 3])), Err(ArrangeError::OpenContour(id(3))));
    }
}
