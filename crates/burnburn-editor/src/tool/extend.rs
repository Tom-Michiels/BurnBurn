//! The extend tool: a click near an open end of a line or arc lengthens it to the nearest boundary.

use burnburn_geometry::{EndpointRef, Point};
use burnburn_types::GeometryId;

use super::modify::{Action, REACH_PT, Rejection, ensure_editable, rejected};
use super::{EditCommit, ToolContext, ToolStep};
use crate::input::PointerEvent;

#[derive(Debug, Clone, Default)]
pub(super) struct ExtendAction;

/// The open end nearest to `pick` within `reach`: object, end and distance.
fn nearest_end(ctx: &ToolContext<'_>, pick: Point, reach: f64) -> Option<(GeometryId, EndpointRef, f64)> {
    let mut best: Option<(GeometryId, EndpointRef, f64)> = None;
    for (id, obj) in ctx.document.objects() {
        for (si, sp) in obj.path().subpaths().iter().enumerate() {
            if sp.is_closed() || sp.segments().is_empty() {
                continue;
            }
            for (end, p) in [(EndpointRef::Start(si), sp.start()), (EndpointRef::End(si), sp.end())] {
                let d = p.distance(pick);
                if d <= reach && best.is_none_or(|(_, _, b)| d < b) {
                    best = Some((id, end, d));
                }
            }
        }
    }
    best
}

impl Action for ExtendAction {
    fn press(&mut self, ctx: &ToolContext<'_>, _at: Point, ev: &PointerEvent) -> ToolStep {
        let Some((target, end, _)) = nearest_end(ctx, ev.position, ctx.tolerance_mm(REACH_PT)) else {
            return rejected(Rejection::NothingPicked);
        };
        if let Err(r) = ensure_editable(ctx, target) {
            return rejected(r);
        }
        let Some(obj) = ctx.document.object(target) else { return rejected(Rejection::NothingPicked) };
        let boundaries: Vec<GeometryId> = ctx.document.objects().map(|(id, _)| id).filter(|id| *id != target).collect();
        let paths: Vec<_> =
            boundaries.iter().filter_map(|id| ctx.document.object(*id)).map(|o| o.path().clone()).collect();
        let refs: Vec<_> = paths.iter().map(|p| p.as_ref()).collect();
        match obj.path().extend_end(end, &refs) {
            Ok(path) => ToolStep::Commit(EditCommit::Extend { target, end, boundaries, path }),
            Err(e) => rejected(e),
        }
    }

    fn reset(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::{EndpointRef, ExtendError, Path, Point};
    use burnburn_types::{GeometryId, StableId};

    use super::super::fixture::*;
    use super::super::modify::Rejection;
    use super::super::modify::kit::*;
    use super::super::{EditCommit, ToolKind};

    fn scene() -> Fixture {
        with_objects(vec![
            polyline(1, &[(0.0, 0.0), (4.0, 0.0)], false),
            polyline(2, &[(10.0, -5.0), (10.0, 5.0)], false),
        ])
    }

    #[test]
    fn a_click_near_the_end_moves_it_to_the_boundary() {
        let (f, mut ed) = (scene(), editor(ToolKind::Extend));
        let EditCommit::Extend { target, end, boundaries, path } = only(&click(&mut ed, &f, 3.8, 0.1)) else {
            panic!("extend")
        };
        assert_eq!((target, end, boundaries), (GeometryId::from_raw(1), EndpointRef::End(0), ids(&[2])));
        assert!(near(path.subpaths()[0].end(), Point::new(10.0, 0.0)));
    }

    #[test]
    fn the_start_end_is_chosen_when_the_click_is_there() {
        let f = with_objects(vec![
            polyline(1, &[(4.0, 0.0), (8.0, 0.0)], false),
            polyline(2, &[(0.0, -5.0), (0.0, 5.0)], false),
        ]);
        let mut ed = editor(ToolKind::Extend);
        let EditCommit::Extend { end, path, .. } = only(&click(&mut ed, &f, 4.2, 0.1)) else { panic!("extend") };
        assert_eq!(end, EndpointRef::Start(0));
        assert!(near(path.subpaths()[0].start(), Point::new(0.0, 0.0)));
    }

    #[test]
    fn nothing_ahead_or_nothing_near_is_a_typed_refusal() {
        let (f, mut ed) = (scene(), editor(ToolKind::Extend));
        let steps = click(&mut ed, &f, 0.2, 0.1);
        assert_eq!(rejection(&steps), Some(Rejection::Extend(ExtendError::NoBoundaryAhead)));
        assert!(commits(&steps).is_empty());
        let long = with_objects(vec![polyline(1, &[(0.0, 0.0), (40.0, 0.0)], false)]);
        assert_eq!(rejection(&click(&mut ed, &long, 20.0, 0.0)), Some(Rejection::NothingPicked), "mid-line is no end");
    }

    #[test]
    fn a_bezier_end_extends_along_its_own_curve() {
        // y = x² / 10 as one cubic: the end runs on to the wall at x = 15, y = 22.5.
        let curve = burnburn_geometry::PathBuilder::new()
            .move_to(Point::new(0.0, 0.0))
            .cubic_to(Point::new(10.0 / 3.0, 0.0), Point::new(20.0 / 3.0, 10.0 / 3.0), Point::new(10.0, 10.0))
            .build()
            .unwrap();
        let f = with_objects(vec![path_obj(1, curve), polyline(2, &[(15.0, -5.0), (15.0, 40.0)], false)]);
        let mut ed = editor(ToolKind::Extend);
        let EditCommit::Extend { target, end, path, .. } = only(&click(&mut ed, &f, 9.9, 9.9)) else {
            panic!("extend")
        };
        assert_eq!((target, end), (GeometryId::from_raw(1), EndpointRef::End(0)));
        assert!(near(path.subpaths()[0].end(), Point::new(15.0, 22.5)), "{:?}", path.subpaths()[0].end());
    }

    #[test]
    fn a_bezier_end_with_nothing_ahead_is_a_typed_refusal() {
        let spline =
            Path::spline_fit(&[Point::new(0.0, 0.0), Point::new(3.0, 2.0), Point::new(6.0, 0.0)], false).unwrap();
        let f = with_objects(vec![path_obj(1, spline), polyline(2, &[(-10.0, -5.0), (-10.0, 5.0)], false)]);
        let mut ed = editor(ToolKind::Extend);
        let steps = click(&mut ed, &f, 5.9, 0.1);
        assert_eq!(rejection(&steps), Some(Rejection::Extend(ExtendError::NoBoundaryAhead)));
    }
}
