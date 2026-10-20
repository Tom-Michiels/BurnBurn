//! Tangent Circle with every solution (W3-UI-schikken): pick up to three targets — a straight
//! segment, a circle or arc, or a point in empty space — and see all circles tangent to them. The
//! pointer chooses the solution nearest to it; a click creates it. Two targets need a diameter,
//! typed in the size field.

use std::sync::Arc;

use burnburn_geometry::{
    Circle, Path, PathBuilder, Point, Segment, Shape, TangentError, TangentTarget, nearest_circle, tangent_circles,
    tangent_circles_radius,
};
use burnburn_types::Length;

use super::{CLICK_TOLERANCE_PT, EditCommit, Tool, ToolContext, ToolPreview, ToolStep, is_escape, tool_pointer};
use crate::input::{CanvasInput, PointerPhase};
use crate::snap::SnapCandidate;

/// A target needs this much distance from another to count as another one (mm).
const SAME_TARGET_MM: f64 = 1e-6;

/// What a scene draws while the tool runs.
#[derive(Debug, Clone, PartialEq)]
pub struct TangentPreview {
    /// The picked targets, drawn as highlighted lines, circles and crosses.
    pub picks: Arc<Path>,
    pub picked: usize,
    /// Every solution as a circle outline.
    pub candidates: Vec<Arc<Path>>,
    /// The solution a click would create: the one whose centre is nearest to the pointer.
    pub chosen: Option<usize>,
    /// Where the chosen circle touches each target.
    pub contacts: Vec<Point>,
    /// The diameter typed for a two-target circle, as a radius (mm).
    pub radius_mm: Option<f64>,
    /// Why there is no solution to show.
    pub error: Option<TangentError>,
}

#[derive(Debug, Clone)]
struct Pick {
    target: TangentTarget,
    highlight: Path,
}

#[derive(Debug, Clone)]
pub struct TangentCirclesTool {
    picks: Vec<Pick>,
    radius_mm: Option<f64>,
    hover: Option<Point>,
    snap: Option<SnapCandidate>,
    mm_per_point: f64,
}

impl TangentCirclesTool {
    pub(super) fn new() -> Self {
        TangentCirclesTool { picks: Vec::new(), radius_mm: None, hover: None, snap: None, mm_per_point: 0.5 }
    }

    fn reset(&mut self) {
        self.picks.clear();
        self.radius_mm = None;
        self.hover = None;
    }

    /// All circles tangent to the picks, or why there are none to show.
    fn solve(&self) -> (Vec<Circle>, Option<TangentError>) {
        let t = |i: usize| self.picks.get(i).map(|p| p.target);
        let result = match (self.picks.len(), self.radius_mm) {
            (3, _) => match (t(0), t(1), t(2)) {
                (Some(a), Some(b), Some(c)) => tangent_circles([a, b, c]),
                _ => return (Vec::new(), None),
            },
            (2, Some(r)) => match (t(0), t(1)) {
                (Some(a), Some(b)) => tangent_circles_radius([a, b], r),
                _ => return (Vec::new(), None),
            },
            _ => return (Vec::new(), None),
        };
        match result {
            Ok(circles) => (circles, None),
            Err(e) => (Vec::new(), Some(e)),
        }
    }

    /// The target a click at `world` picks: a straight segment or circular arc of the drawing near
    /// it, else the point itself.
    fn pick_at(&self, ctx: &ToolContext<'_>, world: Point) -> Pick {
        let tolerance = ctx.tolerance_mm(CLICK_TOLERANCE_PT);
        let mut best: Option<(f64, TangentTarget)> = None;
        for id in ctx.document.draw_order() {
            if !ctx.document.is_selectable(id) {
                continue;
            }
            let Some(obj) = ctx.document.object(id) else { continue };
            if obj.bounds().is_none_or(|b| b.distance_to(world) > tolerance) {
                continue;
            }
            let Some(near) = obj.path().nearest(world).filter(|n| n.distance <= tolerance) else { continue };
            if best.as_ref().is_some_and(|(d, _)| *d <= near.distance) {
                continue;
            }
            let Some((from, segment)) = segment_at(obj.path(), near.at) else { continue };
            if let Some(target) = TangentTarget::from_segment(from, &segment) {
                best = Some((near.distance, target));
            }
        }
        match best {
            Some((_, target)) => Pick { target, highlight: highlight_of(&target, self.mm_per_point) },
            None => {
                let target = TangentTarget::Point(world);
                Pick { target, highlight: highlight_of(&target, self.mm_per_point) }
            }
        }
    }

    fn already_picked(&self, target: &TangentTarget) -> bool {
        self.picks.iter().any(|p| same_target(&p.target, target))
    }

    fn commit(&mut self, circle: Option<Circle>) -> ToolStep {
        self.reset();
        match circle.and_then(|c| Length::from_mm(c.radius).ok().filter(|r| r.mm() > 0.0).map(|r| (c.center, r))) {
            Some((center, radius)) => {
                ToolStep::Commit(EditCommit::CreateShape { shape: Shape::Circle { center, radius } })
            }
            None => ToolStep::Cancelled,
        }
    }
}

/// The segment of `path` a `SegmentRef` names, with its start point.
fn segment_at(path: &Path, at: burnburn_geometry::SegmentRef) -> Option<(Point, Segment)> {
    let sp = path.subpaths().get(at.subpath)?;
    let mut from = sp.start();
    for (i, seg) in sp.segments().iter().enumerate() {
        if i == at.segment {
            return Some((from, *seg));
        }
        from = seg.end();
    }
    (sp.is_closed() && at.segment == sp.segments().len()).then(|| (from, Segment::Line { to: sp.start() }))
}

fn same_target(a: &TangentTarget, b: &TangentTarget) -> bool {
    match (a, b) {
        (TangentTarget::Point(p), TangentTarget::Point(q)) => p.distance(*q) < SAME_TARGET_MM,
        (TangentTarget::Line((a0, a1)), TangentTarget::Line((b0, b1))) => {
            (a0.distance(*b0) < SAME_TARGET_MM && a1.distance(*b1) < SAME_TARGET_MM)
                || (a0.distance(*b1) < SAME_TARGET_MM && a1.distance(*b0) < SAME_TARGET_MM)
        }
        (TangentTarget::Circle(c), TangentTarget::Circle(d)) => {
            c.center.distance(d.center) < SAME_TARGET_MM && (c.radius - d.radius).abs() < SAME_TARGET_MM
        }
        _ => false,
    }
}

/// How a pick is drawn: the line segment, the circle, or a small cross on the point.
fn highlight_of(target: &TangentTarget, mm_per_point: f64) -> Path {
    match *target {
        TangentTarget::Line((a, b)) => Path::line(a, b).unwrap_or_default(),
        TangentTarget::Circle(c) => Length::from_mm(c.radius)
            .ok()
            .and_then(|radius| Shape::Circle { center: c.center, radius }.to_path().ok())
            .unwrap_or_default(),
        TangentTarget::Point(p) => {
            let d = 4.0 * mm_per_point;
            PathBuilder::new()
                .move_to(Point::new(p.x - d, p.y))
                .line_to(Point::new(p.x + d, p.y))
                .move_to(Point::new(p.x, p.y - d))
                .line_to(Point::new(p.x, p.y + d))
                .build()
                .unwrap_or_default()
        }
    }
}

fn circle_path(c: &Circle) -> Option<Arc<Path>> {
    let radius = Length::from_mm(c.radius).ok().filter(|r| r.mm() > 0.0)?;
    Shape::Circle { center: c.center, radius }.to_path().ok().map(Arc::new)
}

impl Tool for TangentCirclesTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase == PointerPhase::Cancel {
            return self.cancel();
        }
        self.mm_per_point = ctx.mm_per_point;
        let (world, snap) = ctx.snapped(ev, &[]);
        let snap_changed = snap != self.snap;
        self.snap = snap;
        match ev.phase {
            PointerPhase::Move => {
                self.hover = Some(world);
                if !self.picks.is_empty() || snap_changed { ToolStep::PreviewChanged } else { ToolStep::Nothing }
            }
            PointerPhase::Down => {
                let (circles, _) = self.solve();
                let complete = self.picks.len() == 3 || (self.picks.len() == 2 && self.radius_mm.is_some());
                if complete {
                    // A click creates the solution nearest to it; with none to choose, nothing.
                    return match nearest_circle(&circles, world) {
                        Some(circle) => self.commit(Some(circle)),
                        None => ToolStep::Nothing,
                    };
                }
                let pick = self.pick_at(ctx, world);
                if self.already_picked(&pick.target) {
                    return ToolStep::Nothing;
                }
                self.picks.push(pick);
                ToolStep::PreviewChanged
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
        let (circles, error) = self.solve();
        let chosen_circle = self.hover.and_then(|h| nearest_circle(&circles, h));
        let chosen = chosen_circle.and_then(|c| circles.iter().position(|x| *x == c));
        let contacts =
            chosen_circle.map(|c| self.picks.iter().map(|p| p.target.contact(&c)).collect()).unwrap_or_default();
        let mut picks = Vec::new();
        for p in &self.picks {
            picks.extend(p.highlight.subpaths().iter().cloned());
        }
        Some(ToolPreview::Tangent(TangentPreview {
            picks: Arc::new(Path::from_subpaths(picks)),
            picked: self.picks.len(),
            candidates: circles.iter().filter_map(circle_path).collect(),
            chosen,
            contacts,
            radius_mm: self.radius_mm,
            error,
        }))
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }

    fn is_active(&self) -> bool {
        !self.picks.is_empty()
    }

    /// The diameter of a circle tangent to two targets; the solutions appear and a click chooses.
    fn exact_size(&mut self, width: Length, _height: Length) -> ToolStep {
        if self.picks.len() != 2 || !(width.mm() > 0.0) {
            return ToolStep::Nothing;
        }
        self.radius_mm = Some(width.mm() / 2.0);
        ToolStep::PreviewChanged
    }
}

#[cfg(test)]
mod tests {
    use burnburn_document::GeometryObject;
    use burnburn_geometry::{Path, Point, Shape};
    use burnburn_types::Length;

    use super::super::click::kit::*;
    use super::super::fixture::*;
    use super::super::{EditCommit, ToolKind, ToolPreview, ToolStep};
    use super::*;
    use crate::input::{CanvasKey, PointerPhase};

    fn line_obj(raw: u64, a: (f64, f64), b: (f64, f64)) -> GeometryObject {
        path_obj(raw, Path::line(Point::new(a.0, a.1), Point::new(b.0, b.1)).unwrap())
    }

    fn circle_obj(raw: u64, c: (f64, f64), r: f64) -> GeometryObject {
        let shape = Shape::Circle { center: Point::new(c.0, c.1), radius: Length::from_mm(r).unwrap() };
        path_obj(raw, shape.to_path().unwrap())
    }

    fn triangle() -> Fixture {
        Fixture::new(vec![
            line_obj(1, (0.0, 0.0), (40.0, 0.0)),
            line_obj(2, (40.0, 0.0), (0.0, 30.0)),
            line_obj(3, (0.0, 30.0), (0.0, 0.0)),
        ])
    }

    fn pick_three_lines(ed: &mut crate::tool::Editor, f: &Fixture) {
        click(ed, f, 20.0, 0.3);
        click(ed, f, 20.0, 15.0);
        click(ed, f, 0.3, 15.0);
    }

    fn preview(ed: &crate::tool::Editor) -> TangentPreview {
        match ed.preview() {
            Some(ToolPreview::Tangent(p)) => p,
            other => panic!("expected a tangent preview, got {other:?}"),
        }
    }

    fn move_to(ed: &mut crate::tool::Editor, f: &Fixture, x: f64, y: f64) {
        ed.input(&at(PointerPhase::Move, x, y, none()), &f.ctx());
    }

    fn circle_of(c: &EditCommit) -> (Point, f64) {
        match c {
            EditCommit::CreateShape { shape: Shape::Circle { center, radius } } => (*center, radius.mm()),
            other => panic!("not a circle: {other:?}"),
        }
    }

    fn centre_of(path: &Path) -> Point {
        let b = path.bounds().unwrap();
        Point::new((b.min().x + b.max().x) / 2.0, (b.min().y + b.max().y) / 2.0)
    }

    #[test]
    fn three_lines_show_every_solution_and_the_pointer_chooses_one() {
        let f = triangle();
        let mut ed = editor(ToolKind::TangentCircle);
        pick_three_lines(&mut ed, &f);
        move_to(&mut ed, &f, 11.0, 9.0);
        let p = preview(&ed);
        assert_eq!((p.picked, p.candidates.len()), (3, 4), "the incircle and three excircles");
        let chosen = p.chosen.expect("a chosen solution");
        let c = centre_of(&p.candidates[chosen]);
        assert!(close(c, Point::new(10.0, 10.0)), "{c:?}");
        // Pointing beyond the hypotenuse picks the excircle there.
        move_to(&mut ed, &f, 55.0, 58.0);
        let p = preview(&ed);
        let c = centre_of(&p.candidates[p.chosen.unwrap()]);
        assert!(close(c, Point::new(60.0, 60.0)), "{c:?}");
        let steps = click(&mut ed, &f, 55.0, 58.0);
        let (c, r) = circle_of(&only(&steps));
        assert!(close(c, Point::new(60.0, 60.0)) && near(r, 60.0), "{c:?} {r}");
        assert!(ed.preview().is_none(), "the picks are done after the commit");
    }

    #[test]
    fn the_contacts_are_where_the_chosen_circle_touches_the_targets() {
        let f = triangle();
        let mut ed = editor(ToolKind::TangentCircle);
        pick_three_lines(&mut ed, &f);
        move_to(&mut ed, &f, 11.0, 9.0);
        let contacts = preview(&ed).contacts;
        assert_eq!(contacts.len(), 3);
        assert!(contacts.iter().any(|c| close(*c, Point::new(10.0, 0.0))), "{contacts:?}");
        assert!(contacts.iter().any(|c| close(*c, Point::new(0.0, 10.0))), "{contacts:?}");
        // The foot of the incentre on the hypotenuse 3x + 4y = 120.
        let foot = Point::new(10.0 + 0.6 * 10.0, 10.0 + 0.8 * 10.0);
        assert!(contacts.iter().any(|c| close(*c, foot)), "{contacts:?}");
    }

    #[test]
    fn a_point_in_empty_space_is_a_target_too() {
        let f = Fixture::new(vec![line_obj(1, (0.0, 0.0), (100.0, 0.0)), line_obj(2, (0.0, 0.0), (0.0, 100.0))]);
        let mut ed = editor(ToolKind::TangentCircle);
        click(&mut ed, &f, 50.0, 0.3);
        click(&mut ed, &f, 0.3, 50.0);
        click(&mut ed, &f, 6.0, 3.0); // empty space: the point (6, 3)
        move_to(&mut ed, &f, 4.0, 4.0);
        let p = preview(&ed);
        assert_eq!(p.candidates.len(), 2, "radius 3 and radius 15");
        let (c, r) = circle_of(&only(&click(&mut ed, &f, 4.0, 4.0)));
        assert!(close(c, Point::new(3.0, 3.0)) && near(r, 3.0), "{c:?} {r}");
    }

    #[test]
    fn a_circle_is_a_target_and_two_targets_take_a_typed_diameter() {
        let f = Fixture::new(vec![circle_obj(1, (0.0, 0.0), 5.0), line_obj(2, (10.0, -50.0), (10.0, 50.0))]);
        let mut ed = editor(ToolKind::TangentCircle);
        click(&mut ed, &f, 5.0, 0.0); // on the circle's edge
        click(&mut ed, &f, 10.0, 20.0);
        move_to(&mut ed, &f, 7.0, 3.0);
        let p = preview(&ed);
        assert!(p.candidates.is_empty(), "two targets need a diameter first");
        let mm = |v| Length::from_mm(v).unwrap();
        assert_eq!(ed.exact_size(mm(6.0), mm(0.0)), ToolStep::PreviewChanged);
        let p = preview(&ed);
        assert_eq!((p.candidates.len(), p.radius_mm), (2, Some(3.0)));
        for c in &p.candidates {
            let centre = centre_of(c);
            assert!((centre.distance(Point::new(0.0, 0.0)) - 8.0).abs() < 1e-6, "touches the circle: {centre:?}");
            assert!((10.0 - centre.x - 3.0).abs() < 1e-6, "touches the line: {centre:?}");
        }
        let (c, r) = circle_of(&only(&click(&mut ed, &f, 7.0, 3.0)));
        assert!(c.y > 0.0 && near(r, 3.0), "the solution nearest to the pointer: {c:?}");
    }

    #[test]
    fn a_problem_with_the_targets_is_named_and_nothing_is_created() {
        let f = Fixture::new(vec![line_obj(1, (0.0, 0.0), (100.0, 0.0)), line_obj(2, (0.0, 10.0), (100.0, 10.0))]);
        let mut ed = editor(ToolKind::TangentCircle);
        click(&mut ed, &f, 20.0, 0.2);
        click(&mut ed, &f, 20.0, 10.2);
        let mm = |v| Length::from_mm(v).unwrap();
        ed.exact_size(mm(10.0), mm(0.0)); // the strip's own half-width: infinitely many circles
        let p = preview(&ed);
        assert_eq!(p.error, Some(burnburn_geometry::TangentError::Degenerate));
        assert!(p.candidates.is_empty());
        assert!(commits(&click(&mut ed, &f, 40.0, 5.0)).is_empty(), "no circle for an undetermined problem");
    }

    #[test]
    fn the_same_target_twice_is_one_pick() {
        let f = Fixture::new(vec![circle_obj(1, (0.0, 0.0), 5.0), line_obj(2, (0.0, 10.0), (100.0, 10.0))]);
        let mut ed = editor(ToolKind::TangentCircle);
        click(&mut ed, &f, 5.0, 0.0);
        click(&mut ed, &f, 0.0, 5.0); // another part of the same circle
        click(&mut ed, &f, 50.0, 10.2);
        click(&mut ed, &f, 50.0, 10.4); // the same line again
        assert_eq!(preview(&ed).picked, 2);
    }

    #[test]
    fn escape_drops_the_picks_and_creates_nothing() {
        let f = triangle();
        let mut ed = editor(ToolKind::TangentCircle);
        pick_three_lines(&mut ed, &f);
        assert!(ed.is_gesture_active());
        assert_eq!(ed.input(&key(CanvasKey::Escape), &f.ctx()), ToolStep::Cancelled);
        assert!(ed.preview().is_none() && !ed.is_gesture_active());
        assert!(commits(&click(&mut ed, &f, 11.0, 9.0)).is_empty(), "the picks are gone");
    }
}
