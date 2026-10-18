//! The point tool: one click, one point.

use burnburn_geometry::Point;

use super::click::{Out, Variant};

pub(super) fn build(variant: Variant, pts: &[Point]) -> Option<Out> {
    match (variant, pts.first()) {
        (Variant::Point, Some(&at)) => Some(Out::Point(at)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::Point;

    use super::super::click::kit::*;
    use super::super::fixture::*;
    use super::super::{EditCommit, ToolKind, ToolStep};
    use crate::input::{CanvasKey, PointerPhase};

    #[test]
    fn one_click_creates_one_point() {
        let (f, mut ed) = (empty(), editor(ToolKind::Point));
        let steps = click(&mut ed, &f, 3.0, 4.0);
        assert_eq!(only(&steps), EditCommit::CreatePoint { at: Point::new(3.0, 4.0) });
        assert_eq!(steps[1], ToolStep::Nothing, "the release commits nothing more");
    }

    #[test]
    fn points_snap_and_every_click_makes_another() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0)]);
        f.snap.objects = true;
        let mut ed = editor(ToolKind::Point);
        let steps = click(&mut ed, &f, 40.6, 20.4);
        assert_eq!(only(&steps), EditCommit::CreatePoint { at: Point::new(40.0, 20.0) });
        let steps = click(&mut ed, &f, 100.0, 100.0);
        assert_eq!(only(&steps), EditCommit::CreatePoint { at: Point::new(100.0, 100.0) });
    }

    #[test]
    fn a_point_tool_has_no_gesture_to_cancel_and_never_previews() {
        let (f, mut ed) = (empty(), editor(ToolKind::Point));
        ed.input(&at(PointerPhase::Move, 5.0, 5.0, none()), &f.ctx());
        assert_eq!(ed.preview(), None);
        assert_eq!(ed.input(&key(CanvasKey::Escape), &f.ctx()), ToolStep::Nothing);
    }
}
