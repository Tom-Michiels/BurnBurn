//! The text tool: a click places the anchor; the text itself arrives through the host's text input.

use burnburn_geometry::Point;

use super::modify::Action;
use super::{EditCommit, ToolContext, ToolStep};
use crate::input::PointerEvent;

#[derive(Debug, Clone, Default)]
pub(super) struct TextAction;

impl Action for TextAction {
    fn press(&mut self, _ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) -> ToolStep {
        ToolStep::Commit(EditCommit::PlaceText { at })
    }

    fn reset(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::Point;

    use super::super::fixture::*;
    use super::super::modify::kit::*;
    use super::super::{EditCommit, ToolKind, ToolStep};
    use crate::input::CanvasKey;
    use crate::snap::SnapKind;

    #[test]
    fn a_click_places_the_text_anchor_exactly_once() {
        let (f, mut ed) = (empty(), editor(ToolKind::Text));
        let steps = click(&mut ed, &f, 3.0, 4.0);
        assert_eq!(only(&steps), EditCommit::PlaceText { at: Point::new(3.0, 4.0) });
        assert_eq!(steps[1], ToolStep::Nothing, "the release adds nothing");
    }

    #[test]
    fn the_anchor_snaps_to_object_corners() {
        let mut f = with_objects(vec![rect(1, 0.0, 0.0, 40.0, 20.0)]);
        f.snap.objects = true;
        let mut ed = editor(ToolKind::Text);
        hover_to(&mut ed, &f, 39.6, 19.7);
        assert_eq!(ed.snap().map(|s| s.kind), Some(SnapKind::Endpoint));
        let steps = click(&mut ed, &f, 39.6, 19.7);
        assert_eq!(only(&steps), EditCommit::PlaceText { at: Point::new(40.0, 20.0) });
    }

    #[test]
    fn escape_places_nothing() {
        let (f, mut ed) = (empty(), editor(ToolKind::Text));
        assert_eq!(press_key(&mut ed, &f, CanvasKey::Escape), ToolStep::Nothing);
    }
}
