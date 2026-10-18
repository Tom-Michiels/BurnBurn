//! The trim tool: a click on a piece of a line, arc or curve removes it between the crossings
//! with the other objects.

use burnburn_geometry::Point;
use burnburn_types::GeometryId;

use super::modify::{Action, Rejection, ensure_editable, outline_hit, rejected};
use super::{EditCommit, ToolContext, ToolStep};
use crate::input::PointerEvent;

#[derive(Debug, Clone, Default)]
pub(super) struct TrimAction;

impl Action for TrimAction {
    fn press(&mut self, ctx: &ToolContext<'_>, _at: Point, ev: &PointerEvent) -> ToolStep {
        let pick = ev.position;
        let Some(target) = outline_hit(ctx, pick) else { return rejected(Rejection::NothingPicked) };
        if let Err(r) = ensure_editable(ctx, target) {
            return rejected(r);
        }
        let Some(obj) = ctx.document.object(target) else { return rejected(Rejection::NothingPicked) };
        let cutters: Vec<GeometryId> = ctx.document.objects().map(|(id, _)| id).filter(|id| *id != target).collect();
        let paths: Vec<_> =
            cutters.iter().filter_map(|id| ctx.document.object(*id)).map(|o| o.path().clone()).collect();
        let refs: Vec<_> = paths.iter().map(|p| p.as_ref()).collect();
        match obj.path().trim_at(pick, &refs) {
            Ok(path) => ToolStep::Commit(EditCommit::Trim { target, pick, cutters, path }),
            Err(e) => rejected(Rejection::Trim(e)),
        }
    }

    fn reset(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use burnburn_document::sketch::SketchId;
    use burnburn_document::{DerivedKey, DerivedOwner, GeometryObject, GeometrySource, ObjectKind};
    use burnburn_geometry::{Path, Point, TrimError};
    use burnburn_types::{GeometryId, StableId};

    use super::super::fixture::*;
    use super::super::modify::Rejection;
    use super::super::modify::kit::*;
    use super::super::{EditCommit, ToolKind};

    fn line(raw: u64, a: (f64, f64), b: (f64, f64)) -> GeometryObject {
        polyline(raw, &[a, b], false)
    }

    #[test]
    fn a_click_removes_the_piece_between_the_two_nearest_crossings() {
        let f = with_objects(vec![
            line(1, (0.0, 0.0), (10.0, 0.0)),
            line(2, (3.0, -1.0), (3.0, 1.0)),
            line(3, (7.0, -1.0), (7.0, 1.0)),
        ]);
        let mut ed = editor(ToolKind::Trim);
        let EditCommit::Trim { target, pick, cutters, path } = only(&click(&mut ed, &f, 5.0, 0.1)) else {
            panic!("trim")
        };
        assert_eq!((target, pick), (GeometryId::from_raw(1), Point::new(5.0, 0.1)));
        assert_eq!(cutters, ids(&[2, 3]), "every other object cuts, the target does not cut itself");
        assert_eq!(path.subpaths().len(), 2);
        assert!(
            near(path.subpaths()[0].end(), Point::new(3.0, 0.0))
                && near(path.subpaths()[1].start(), Point::new(7.0, 0.0))
        );
    }

    #[test]
    fn a_circle_is_trimmed_to_an_open_arc() {
        let f = with_objects(vec![
            path_obj(1, Path::circle(Point::ORIGIN, 10.0).unwrap()),
            line(2, (0.0, -20.0), (0.0, 20.0)),
        ]);
        let mut ed = editor(ToolKind::Trim);
        let EditCommit::Trim { path, .. } = only(&click(&mut ed, &f, 10.0, 0.0)) else { panic!("trim") };
        let b = path.bounds().unwrap();
        assert!(!path.subpaths()[0].is_closed() && (b.min().x + 10.0).abs() < 1e-9 && b.max().x.abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn refusals_are_typed_and_commit_nothing() {
        let f = with_objects(vec![line(1, (0.0, 0.0), (10.0, 0.0))]);
        let mut ed = editor(ToolKind::Trim);
        let steps = click(&mut ed, &f, 50.0, 50.0);
        assert_eq!(rejection(&steps), Some(Rejection::NothingPicked));
        let steps = click(&mut ed, &f, 5.0, 0.0);
        assert!(commits(&steps).is_empty());
        assert_eq!(rejection(&steps), Some(Rejection::Trim(TrimError::NoIntersection)));
    }

    #[test]
    fn sketch_output_cannot_be_trimmed_directly() {
        let key = DerivedKey { owner: DerivedOwner::Sketch(SketchId::from_raw(100)), local: 0 };
        let derived = GeometryObject::new(
            GeometryId::from_raw(1),
            "o".into(),
            ObjectKind::Path,
            GeometrySource::Derived(key),
            Path::line(Point::ORIGIN, Point::new(10.0, 0.0)).unwrap(),
        );
        let f = with_objects(vec![derived, line(2, (5.0, -1.0), (5.0, 1.0))]);
        let mut ed = editor(ToolKind::Trim);
        let steps = click(&mut ed, &f, 8.0, 0.0);
        assert_eq!(rejection(&steps), Some(Rejection::DerivedGeometry(GeometryId::from_raw(1))));
        assert!(commits(&steps).is_empty());
    }
}
