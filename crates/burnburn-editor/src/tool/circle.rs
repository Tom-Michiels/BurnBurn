//! Circle variants: two-point, three-point and the tangent circles.

use burnburn_document::DocumentSnapshot;
use burnburn_geometry::{
    Line, Path, PathBuilder, Point, Segment, Shape, circle_tangent_three_lines, circle_tangent_two_lines,
    circle_tangent_two_lines_radius, circle_through_three,
};
use burnburn_types::{GeometryId, Length};

use super::click::{Out, Variant};
use super::{
    CLICK_TOLERANCE_PT, EditCommit, ShapeKind, Tool, ToolContext, ToolPreview, ToolStep, is_escape, tool_pointer,
};
use crate::input::{CanvasInput, PointerPhase};
use crate::snap::SnapCandidate;

fn circle_out(center: Point, radius: f64) -> Option<Out> {
    let radius = Length::from_mm(radius).ok().filter(|r| r.mm() > 0.0)?;
    Some(Out::Shape(Shape::Circle { center, radius }, ShapeKind::Circle))
}

pub(super) fn build(variant: Variant, pts: &[Point]) -> Option<Out> {
    match variant {
        Variant::CircleTwoPoint => {
            let (&a, &b) = (pts.first()?, pts.get(1)?);
            circle_out(a.lerp(b, 0.5), a.distance(b) / 2.0)
        }
        Variant::CircleThreePoint => {
            let c = circle_through_three(*pts.first()?, *pts.get(1)?, *pts.get(2)?).ok()?;
            circle_out(c.center, c.radius)
        }
        _ => None,
    }
}

/// A straight segment of an object, picked by a click.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pick {
    /// Object, subpath and segment index (the implicit closing segment of a closed subpath comes last).
    key: (GeometryId, usize, usize),
    line: Line,
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let d = b - a;
    let len2 = d.dot(d);
    let t = if len2 > 0.0 { ((p - a).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + d * t)
}

/// The straight segment of the document nearest to `p` within `tolerance` (curves are not tangent targets).
fn pick_line(doc: &DocumentSnapshot, p: Point, tolerance: f64) -> Option<Pick> {
    let mut best: Option<(f64, Pick)> = None;
    let mut consider = |key: (GeometryId, usize, usize), a: Point, b: Point| {
        if a == b {
            return;
        }
        let d = distance_to_segment(p, a, b);
        if d <= tolerance && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, Pick { key, line: (a, b) }));
        }
    };
    for (id, object) in doc.objects() {
        for (si, sub) in object.path().subpaths().iter().enumerate() {
            let mut from = sub.start();
            for (gi, seg) in sub.segments().iter().enumerate() {
                if let Segment::Line { to } = seg {
                    consider((id, si, gi), from, *to);
                }
                from = seg.end();
            }
            if sub.is_closed() {
                consider((id, si, sub.segments().len()), from, sub.start());
            }
        }
    }
    best.map(|(_, pick)| pick)
}

/// Circle tangent to two or three picked straight segments. The circle follows the pointer (it
/// goes to the sector, or for three lines the incircle/excircle, nearest to it); the next click
/// commits it. Type a diameter to fix the radius of a two-line circle.
#[derive(Debug, Clone)]
pub struct TangentCircleTool {
    lines: usize,
    picks: Vec<Pick>,
    hover: Option<Point>,
    snap: Option<SnapCandidate>,
}

impl TangentCircleTool {
    pub(super) fn new(lines: usize) -> Self {
        TangentCircleTool { lines, picks: Vec::new(), hover: None, snap: None }
    }

    fn reset(&mut self) {
        self.picks.clear();
        self.hover = None;
    }

    fn circle_near(&self, near: Point) -> Option<Out> {
        let l = |i: usize| self.picks.get(i).map(|p| p.line);
        let c = match self.lines {
            2 => circle_tangent_two_lines(l(0)?, l(1)?, near),
            _ => circle_tangent_three_lines(l(0)?, l(1)?, l(2)?, near),
        }
        .ok()?;
        circle_out(c.center, c.radius)
    }

    fn complete(&self) -> bool {
        self.picks.len() >= self.lines
    }

    fn commit(&mut self, out: Option<Out>) -> ToolStep {
        self.reset();
        match out {
            Some(Out::Shape(shape, _)) => ToolStep::Commit(EditCommit::CreateShape { shape }),
            _ => ToolStep::Cancelled,
        }
    }
}

impl Tool for TangentCircleTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase == PointerPhase::Cancel {
            return self.cancel();
        }
        let (world, snap) = ctx.snapped(ev, &[]);
        let snap_changed = snap != self.snap;
        self.snap = snap;
        match ev.phase {
            PointerPhase::Down if self.complete() => {
                let out = self.circle_near(world);
                self.commit(out)
            }
            PointerPhase::Down => match pick_line(ctx.document, ev.position, ctx.tolerance_mm(CLICK_TOLERANCE_PT)) {
                Some(pick) if !self.picks.iter().any(|p| p.key == pick.key) => {
                    self.picks.push(pick);
                    ToolStep::PreviewChanged
                }
                _ => ToolStep::Nothing,
            },
            PointerPhase::Move => {
                self.hover = Some(world);
                if !self.picks.is_empty() || snap_changed { ToolStep::PreviewChanged } else { ToolStep::Nothing }
            }
            _ => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        let active = !self.picks.is_empty();
        self.reset();
        if active { ToolStep::Cancelled } else { ToolStep::Nothing }
    }

    fn preview(&self) -> Option<ToolPreview> {
        if self.picks.is_empty() {
            return None;
        }
        if self.complete()
            && let Some(Out::Shape(shape, kind)) = self.hover.and_then(|h| self.circle_near(h))
        {
            return Some(ToolPreview::Shape { path: std::sync::Arc::new(shape.to_path().ok()?), shape: kind });
        }
        // Until the circle exists, the picked segments are highlighted.
        let mut b = PathBuilder::new();
        for pick in &self.picks {
            b = b.move_to(pick.line.0).line_to(pick.line.1);
        }
        let path: Path = b.build().ok()?;
        Some(ToolPreview::Shape { path: std::sync::Arc::new(path), shape: ShapeKind::Line })
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }

    fn is_active(&self) -> bool {
        !self.picks.is_empty()
    }

    /// The diameter of a circle tangent to two lines, on the side of the pointer.
    fn exact_size(&mut self, width: Length, _height: Length) -> ToolStep {
        if self.lines != 2 || !self.complete() {
            return ToolStep::Nothing;
        }
        let near = self.hover.unwrap_or_default();
        let out = (|| {
            let (a, b) = (self.picks.first()?.line, self.picks.get(1)?.line);
            let c = circle_tangent_two_lines_radius(a, b, width.mm() / 2.0, near).ok()?;
            circle_out(c.center, c.radius)
        })();
        self.commit(out)
    }
}

#[cfg(test)]
mod tests {
    use burnburn_document::GeometryObject;
    use burnburn_geometry::{Path, Point, Shape};
    use burnburn_types::Length;

    use super::super::click::kit::*;
    use super::super::fixture::*;
    use super::super::{EditCommit, ToolKind, ToolStep};
    use crate::input::{CanvasKey, PointerPhase};

    fn circle_of(c: &EditCommit) -> (Point, f64) {
        match c {
            EditCommit::CreateShape { shape: Shape::Circle { center, radius } } => (*center, radius.mm()),
            other => panic!("not a circle: {other:?}"),
        }
    }

    fn line_obj(raw: u64, a: (f64, f64), b: (f64, f64)) -> GeometryObject {
        path_obj(raw, Path::line(Point::new(a.0, a.1), Point::new(b.0, b.1)).unwrap())
    }

    #[test]
    fn two_point_circle_has_the_clicks_as_diameter_ends() {
        let (f, mut ed) = (empty(), editor(ToolKind::CircleTwoPoint));
        let steps = drag(&mut ed, &f, (0.0, 0.0), (10.0, 0.0));
        let (c, r) = circle_of(&only(&steps));
        assert!(close(c, Point::new(5.0, 0.0)) && near(r, 5.0), "{c:?} {r}");
        let (_, steps) = clicks(ToolKind::CircleTwoPoint, &[(0.0, 0.0), (6.0, 8.0)]);
        let (c, r) = circle_of(&only(&steps));
        assert!(close(c, Point::new(3.0, 4.0)) && near(r, 5.0));
    }

    #[test]
    fn typed_diameter_of_a_two_point_circle() {
        let (f, mut ed) = (empty(), editor(ToolKind::CircleTwoPoint));
        ed.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
        ed.input(&at(PointerPhase::Move, 0.0, 3.0, none()), &f.ctx());
        let mm = |v| Length::from_mm(v).unwrap();
        let (c, r) = circle_of(&only(&[ed.exact_size(mm(10.0), mm(0.0))]));
        assert!(close(c, Point::new(0.0, 5.0)) && near(r, 5.0), "{c:?} {r}");
    }

    #[test]
    fn three_point_circle_passes_through_all_clicks() {
        let (_, steps) = clicks(ToolKind::CircleThreePoint, &[(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)]);
        let (c, r) = circle_of(&only(&steps));
        assert!(close(c, Point::new(5.0, 5.0)) && near(r, 50.0_f64.sqrt()), "{c:?} {r}");
    }

    #[test]
    fn two_tangent_circle_touches_both_picked_lines_and_follows_the_pointer() {
        let f = Fixture::new(vec![line_obj(1, (0.0, 0.0), (100.0, 0.0)), line_obj(2, (0.0, 0.0), (0.0, 100.0))]);
        let mut ed = editor(ToolKind::CircleTwoTangent);
        assert!(commits(&click(&mut ed, &f, 50.0, 0.4)).is_empty());
        assert!(commits(&click(&mut ed, &f, 0.4, 60.0)).is_empty());
        ed.input(&at(PointerPhase::Move, 30.0, 35.0, none()), &f.ctx());
        assert!(matches!(ed.preview(), Some(super::super::ToolPreview::Shape { .. })));
        let steps = click(&mut ed, &f, 30.0, 35.0);
        let (c, r) = circle_of(&only(&steps));
        assert!(close(c, Point::new(32.5, 32.5)) && near(r, 32.5), "{c:?} {r}");
        assert_eq!(ed.preview(), None);
    }

    #[test]
    fn two_tangent_circle_with_a_typed_diameter() {
        let f = Fixture::new(vec![line_obj(1, (0.0, 0.0), (100.0, 0.0)), line_obj(2, (0.0, 0.0), (0.0, 100.0))]);
        let mut ed = editor(ToolKind::CircleTwoTangent);
        click(&mut ed, &f, 50.0, 0.4);
        click(&mut ed, &f, 0.4, 60.0);
        ed.input(&at(PointerPhase::Move, 30.0, 35.0, none()), &f.ctx());
        let mm = |v| Length::from_mm(v).unwrap();
        let (c, r) = circle_of(&only(&[ed.exact_size(mm(8.0), mm(0.0))]));
        assert!(close(c, Point::new(4.0, 4.0)) && near(r, 4.0), "{c:?} {r}");
    }

    #[test]
    fn three_tangent_circle_is_the_incircle_when_the_pointer_is_inside() {
        let f = Fixture::new(vec![
            line_obj(1, (0.0, 0.0), (40.0, 0.0)),
            line_obj(2, (40.0, 0.0), (0.0, 30.0)),
            line_obj(3, (0.0, 30.0), (0.0, 0.0)),
        ]);
        let mut ed = editor(ToolKind::CircleThreeTangent);
        click(&mut ed, &f, 20.0, 0.3);
        click(&mut ed, &f, 20.0, 15.0);
        click(&mut ed, &f, 0.3, 15.0);
        let steps = click(&mut ed, &f, 11.0, 9.0);
        let (c, r) = circle_of(&only(&steps));
        assert!(close(c, Point::new(10.0, 10.0)) && near(r, 10.0), "{c:?} {r}");
    }

    #[test]
    fn tangent_picks_ignore_empty_space_and_repeated_lines() {
        let f = Fixture::new(vec![line_obj(1, (0.0, 0.0), (100.0, 0.0)), line_obj(2, (0.0, 0.0), (0.0, 100.0))]);
        let mut ed = editor(ToolKind::CircleTwoTangent);
        assert_eq!(click(&mut ed, &f, 50.0, 50.0), vec![ToolStep::Nothing, ToolStep::Nothing]);
        assert!(!ed.is_gesture_active());
        click(&mut ed, &f, 50.0, 0.2);
        click(&mut ed, &f, 70.0, 0.2);
        assert!(ed.is_gesture_active(), "one line picked");
        ed.input(&at(PointerPhase::Move, 30.0, 35.0, none()), &f.ctx());
        assert!(commits(&click(&mut ed, &f, 30.0, 35.0)).is_empty(), "the same line twice is still one pick");
    }

    #[test]
    fn parallel_tangent_pair_fills_the_strip() {
        let f = Fixture::new(vec![line_obj(1, (0.0, 0.0), (100.0, 0.0)), line_obj(2, (0.0, 10.0), (100.0, 10.0))]);
        let mut ed = editor(ToolKind::CircleTwoTangent);
        click(&mut ed, &f, 20.0, 0.2);
        click(&mut ed, &f, 20.0, 10.2);
        let (c, r) = circle_of(&only(&click(&mut ed, &f, 40.0, 7.0)));
        assert!(close(c, Point::new(40.0, 5.0)) && near(r, 5.0), "{c:?} {r}");
    }

    #[test]
    fn cancelling_a_tangent_circle_keeps_the_document_and_state_clean() {
        let f = Fixture::new(vec![line_obj(1, (0.0, 0.0), (100.0, 0.0)), line_obj(2, (0.0, 0.0), (0.0, 100.0))]);
        let before = f.snapshot.revision();
        let mut ed = editor(ToolKind::CircleTwoTangent);
        click(&mut ed, &f, 50.0, 0.2);
        click(&mut ed, &f, 0.2, 50.0);
        assert_eq!(ed.input(&key(CanvasKey::Escape), &f.ctx()), ToolStep::Cancelled);
        assert_eq!(ed.preview(), None);
        assert_eq!(f.snapshot.revision(), before);
        assert!(commits(&click(&mut ed, &f, 30.0, 30.0)).is_empty(), "the picks are gone");
    }
}
