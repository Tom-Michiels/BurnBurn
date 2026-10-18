//! The mirror tool: mirrors the selection about an axis given by two clicks or by a line object.

use burnburn_geometry::{Affine, Path, Point, Segment};
use burnburn_types::GeometryId;

use super::modify::{Action, ModifyParam, Rejection, line_preview, outline_hit, rejected};
use super::{EditCommit, ToolContext, ToolPreview, ToolStep};
use crate::input::PointerEvent;

/// Axes shorter than this are no axis (float noise, not a user tolerance).
const MIN_AXIS_MM: f64 = 1e-9;

#[derive(Debug, Clone, Default)]
pub(super) struct MirrorAction {
    first: Option<Point>,
    hover: Option<Point>,
    keep_original: bool,
}

/// The reflection about the line through `a` and `b`, or `None` for coincident points.
pub(super) fn reflection(a: Point, b: Point) -> Option<Affine> {
    let d = b - a;
    let len2 = d.dot(d);
    if !(len2.sqrt() > MIN_AXIS_MM) {
        return None;
    }
    // cos 2θ and sin 2θ of the axis direction θ, without trigonometry.
    let (c, s) = ((d.x * d.x - d.y * d.y) / len2, 2.0 * d.x * d.y / len2);
    let e = a.x - (c * a.x + s * a.y);
    let f = a.y - (s * a.x - c * a.y);
    Affine::new([c, s, s, -c, e, f]).ok()
}

/// The two ends of an object that is exactly one straight line.
fn line_axis(path: &Path) -> Option<(Point, Point)> {
    let [sp] = path.subpaths() else { return None };
    match sp.segments() {
        [Segment::Line { to }] if !sp.is_closed() => Some((sp.start(), *to)),
        _ => None,
    }
}

impl MirrorAction {
    fn commit(&mut self, targets: Vec<GeometryId>, a: Point, b: Point) -> ToolStep {
        self.first = None;
        self.hover = None;
        match reflection(a, b) {
            Some(transform) => {
                ToolStep::Commit(EditCommit::Mirror { targets, transform, keep_original: self.keep_original })
            }
            None => rejected(Rejection::InvalidValue),
        }
    }
}

impl Action for MirrorAction {
    fn press(&mut self, ctx: &ToolContext<'_>, at: Point, ev: &PointerEvent) -> ToolStep {
        if ctx.selection.is_empty() {
            self.first = None;
            return rejected(Rejection::NeedsSelection);
        }
        let targets = ctx.selection.ids().to_vec();
        if let Some(a) = self.first {
            return self.commit(targets, a, at);
        }
        let axis_object = outline_hit(ctx, ev.position)
            .filter(|id| !ctx.selection.contains(*id))
            .and_then(|id| ctx.document.object(id))
            .and_then(|o| line_axis(o.path()));
        match axis_object {
            Some((a, b)) => self.commit(targets, a, b),
            None => {
                self.first = Some(at);
                ToolStep::PreviewChanged
            }
        }
    }

    fn hover(&mut self, _ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) {
        self.hover = Some(at);
    }

    fn reset(&mut self) -> bool {
        self.hover = None;
        self.first.take().is_some()
    }

    fn preview(&self) -> Option<ToolPreview> {
        line_preview(self.first?, self.hover?)
    }

    fn set_param(&mut self, param: ModifyParam) -> bool {
        match param {
            ModifyParam::KeepOriginal(keep) => {
                self.keep_original = keep;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::Point;

    use super::super::fixture::*;
    use super::super::modify::kit::*;
    use super::super::modify::{ModifyParam, Rejection};
    use super::super::{EditCommit, ToolKind, ToolPreview, ToolStep, TransformKind};
    use crate::input::CanvasKey;

    fn scene() -> Fixture {
        let mut f = with_objects(vec![rect(1, 0.0, 0.0, 10.0, 10.0), polyline(2, &[(20.0, -5.0), (20.0, 5.0)], false)]);
        f.select(&[1]);
        f
    }

    fn transform(c: EditCommit) -> (Vec<burnburn_types::GeometryId>, burnburn_geometry::Affine, bool) {
        let EditCommit::Mirror { targets, transform, keep_original } = c else { panic!("mirror, got {c:?}") };
        (targets, transform, keep_original)
    }

    #[test]
    fn two_axis_points_mirror_the_selection_about_that_line() {
        let (f, mut ed) = (scene(), editor(ToolKind::Mirror));
        let (targets, t, keep) = transform(only(&clicks(&mut ed, &f, &[(30.0, 0.0), (30.0, 10.0)])));
        assert_eq!(targets, ids(&[1]));
        assert!(!keep);
        assert!(near(t.apply(Point::new(5.0, 5.0)), Point::new(55.0, 5.0)), "about x = 30");
        assert!(near(t.apply(Point::new(30.0, 7.0)), Point::new(30.0, 7.0)), "points on the axis stay");
        assert!(t.is_mirroring());
    }

    #[test]
    fn a_diagonal_axis_swaps_the_coordinates() {
        let (f, mut ed) = (scene(), editor(ToolKind::Mirror));
        let (_, t, _) = transform(only(&clicks(&mut ed, &f, &[(0.0, 0.0), (7.0, 7.0)])));
        assert!(near(t.apply(Point::new(3.0, 1.0)), Point::new(1.0, 3.0)));
    }

    #[test]
    fn clicking_a_line_object_uses_it_as_the_axis() {
        let (f, mut ed) = (scene(), editor(ToolKind::Mirror));
        let (targets, t, _) = transform(only(&click(&mut ed, &f, 20.0, 1.0)));
        assert_eq!(targets, ids(&[1]));
        assert!(near(t.apply(Point::new(5.0, 5.0)), Point::new(35.0, 5.0)), "about x = 20");
    }

    #[test]
    fn keeping_the_originals_is_a_setting_of_the_commit() {
        let (f, mut ed) = (scene(), editor(ToolKind::Mirror));
        param(&mut ed, ModifyParam::KeepOriginal(true));
        let (_, _, keep) = transform(only(&clicks(&mut ed, &f, &[(30.0, 0.0), (30.0, 10.0)])));
        assert!(keep);
    }

    #[test]
    fn without_a_selection_a_degenerate_axis_or_escape_nothing_is_committed() {
        let mut f = scene();
        let mut ed = editor(ToolKind::Mirror);
        click(&mut ed, &f, 30.0, 0.0);
        hover_to(&mut ed, &f, 30.0, 10.0);
        assert!(matches!(ed.preview(), Some(ToolPreview::Shape { .. })), "the axis follows the pointer");
        assert_eq!(press_key(&mut ed, &f, CanvasKey::Escape), ToolStep::Cancelled);
        assert!(ed.preview().is_none());
        let steps = clicks(&mut ed, &f, &[(30.0, 0.0), (30.0, 0.0)]);
        assert_eq!(rejection(&steps), Some(Rejection::InvalidValue), "both axis points coincide");
        f.select(&[]);
        let steps = click(&mut ed, &f, 30.0, 0.0);
        assert_eq!(rejection(&steps), Some(Rejection::NeedsSelection));
        let _ = TransformKind::Move;
    }
}
