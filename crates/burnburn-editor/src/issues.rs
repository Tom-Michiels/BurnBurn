//! Geometry problem locations shown as overlays (E09). Open contours are read from the path
//! structure; self-intersections come from the domain (`Path::self_intersections`, Kern IM09).

use burnburn_document::DocumentSnapshot;
use burnburn_geometry::Point;
use burnburn_types::GeometryId;

/// An open contour whose ends are closer than this is almost certainly meant to be closed.
pub const NEARLY_CLOSED_MM: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IssueKind {
    /// Ends of an open subpath meet or nearly meet (gap ≤ [`NEARLY_CLOSED_MM`]).
    OpenContour,
    SelfIntersection,
}

/// One located problem with its physical size.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryIssue {
    pub kind: IssueKind,
    /// Where to zoom to: the middle of the gap or the crossing point.
    pub at: Point,
    pub objects: Vec<GeometryId>,
    /// Gap between the two ends (open contour), in mm.
    pub size_mm: Option<f64>,
}

/// Open subpaths whose two ends are within [`NEARLY_CLOSED_MM`], in object order. A deliberately
/// open line or curve (ends far apart) is not a problem.
pub fn open_contours(snapshot: &DocumentSnapshot) -> Vec<GeometryIssue> {
    snapshot
        .objects()
        .flat_map(|(id, obj)| {
            obj.path()
                .subpaths()
                .iter()
                .filter(|sp| !sp.is_closed() && !sp.segments().is_empty())
                .filter_map(move |sp| {
                    let gap = sp.start().distance(sp.end());
                    (gap <= NEARLY_CLOSED_MM).then(|| GeometryIssue {
                        kind: IssueKind::OpenContour,
                        at: sp.start().lerp(sp.end(), 0.5),
                        objects: vec![id],
                        size_mm: Some(gap),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Every crossing of an object's outline with itself, located at the crossing point.
pub fn self_intersections(snapshot: &DocumentSnapshot) -> Vec<GeometryIssue> {
    snapshot
        .objects()
        .flat_map(|(id, obj)| {
            obj.path().self_intersections().into_iter().map(move |x| GeometryIssue {
                kind: IssueKind::SelfIntersection,
                at: x.point,
                objects: vec![id],
                size_mm: None,
            })
        })
        .collect()
}

/// All located problems of a snapshot, in object order (open contours first per object list).
pub fn geometry_issues(snapshot: &DocumentSnapshot) -> Vec<GeometryIssue> {
    let mut out = open_contours(snapshot);
    out.extend(self_intersections(snapshot));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::fixture::{path_obj, rect};
    use burnburn_geometry::PathBuilder;
    use burnburn_types::StableId;

    fn tri(gap: f64) -> burnburn_geometry::Path {
        PathBuilder::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(5.0, 8.0))
            .line_to(Point::new(gap, 0.0))
            .build()
            .unwrap()
    }

    #[test]
    fn nearly_closed_contour_is_located_with_its_gap() {
        let line = PathBuilder::new().move_to(Point::new(0.0, 50.0)).line_to(Point::new(30.0, 50.0)).build().unwrap();
        let snap = DocumentSnapshot::from_parts(
            vec![path_obj(1, tri(0.08)), path_obj(2, line), rect(3, 50.0, 0.0, 5.0, 5.0), path_obj(4, tri(0.0))],
            5,
        )
        .unwrap();
        let issues = open_contours(&snap);
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert_eq!(issues[0].kind, IssueKind::OpenContour);
        assert_eq!(issues[0].objects, vec![GeometryId::from_raw(1)]);
        assert!((issues[0].size_mm.unwrap() - 0.08).abs() < 1e-12);
        assert!(issues[0].at.distance(Point::new(0.04, 0.0)) < 1e-12);
        assert_eq!(
            (issues[1].objects.clone(), issues[1].size_mm),
            (vec![GeometryId::from_raw(4)], Some(0.0)),
            "touching but not closed"
        );
    }

    #[test]
    fn self_intersection_is_located_at_the_crossing() {
        let bow = burnburn_geometry::Path::polyline(
            &[Point::new(0.0, 0.0), Point::new(10.0, 10.0), Point::new(10.0, 0.0), Point::new(0.0, 10.0)],
            true,
        )
        .unwrap();
        let snap = DocumentSnapshot::from_parts(vec![path_obj(1, bow), rect(2, 50.0, 0.0, 5.0, 5.0)], 3).unwrap();
        let issues = self_intersections(&snap);
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(
            (issues[0].kind, issues[0].objects.clone()),
            (IssueKind::SelfIntersection, vec![GeometryId::from_raw(1)])
        );
        assert!(issues[0].at.distance(Point::new(5.0, 5.0)) < 1e-9);
        assert_eq!(geometry_issues(&snap).len(), 1);
    }
}
