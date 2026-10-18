//! The break tool: a click splits the object at that point.

use burnburn_geometry::Point;

use super::modify::{Action, Rejection, ensure_editable, outline_hit, rejected};
use super::{EditCommit, ToolContext, ToolStep};
use crate::input::PointerEvent;

#[derive(Debug, Clone, Default)]
pub(super) struct BreakAction;

impl Action for BreakAction {
    fn press(&mut self, ctx: &ToolContext<'_>, at: Point, ev: &PointerEvent) -> ToolStep {
        let Some(target) = outline_hit(ctx, ev.position) else { return rejected(Rejection::NothingPicked) };
        if let Err(r) = ensure_editable(ctx, target) {
            return rejected(r);
        }
        let Some(obj) = ctx.document.object(target) else { return rejected(Rejection::NothingPicked) };
        match obj.path().break_at(at) {
            Ok(path) => ToolStep::Commit(EditCommit::Break { target, pick: at, path }),
            Err(e) => rejected(Rejection::Break(e)),
        }
    }

    fn reset(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::{BreakError, Point};
    use burnburn_types::{GeometryId, StableId};

    use super::super::fixture::*;
    use super::super::modify::Rejection;
    use super::super::modify::kit::*;
    use super::super::{EditCommit, ToolKind};

    #[test]
    fn a_click_splits_a_line_into_two_pieces_that_meet_at_the_click() {
        let f = with_objects(vec![polyline(1, &[(0.0, 0.0), (10.0, 0.0)], false)]);
        let mut ed = editor(ToolKind::Break);
        let EditCommit::Break { target, pick, path } = only(&click(&mut ed, &f, 4.0, 0.2)) else { panic!("break") };
        assert_eq!((target, pick), (GeometryId::from_raw(1), Point::new(4.0, 0.2)));
        assert_eq!(path.subpaths().len(), 2);
        assert!(
            near(path.subpaths()[0].end(), Point::new(4.0, 0.0))
                && near(path.subpaths()[1].start(), Point::new(4.0, 0.0))
        );
    }

    #[test]
    fn a_closed_contour_becomes_one_open_ring() {
        let f = with_objects(vec![rect(1, 0.0, 0.0, 10.0, 10.0)]);
        let mut ed = editor(ToolKind::Break);
        let EditCommit::Break { path, .. } = only(&click(&mut ed, &f, 5.0, 0.2)) else { panic!("break") };
        assert_eq!(path.subpaths().len(), 1);
        assert!(!path.subpaths()[0].is_closed());
    }

    #[test]
    fn refusals_are_typed() {
        let f = with_objects(vec![polyline(1, &[(0.0, 0.0), (10.0, 0.0)], false)]);
        let mut ed = editor(ToolKind::Break);
        assert_eq!(rejection(&click(&mut ed, &f, 30.0, 30.0)), Some(Rejection::NothingPicked));
        let steps = click(&mut ed, &f, 0.0, 0.0);
        assert_eq!(rejection(&steps), Some(Rejection::Break(BreakError::AtEnd)));
        assert!(commits(&steps).is_empty());
    }
}
