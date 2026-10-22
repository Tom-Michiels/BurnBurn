//! Picking sketch entities (points, lines, circles, arcs) under the pointer, for the dimension,
//! construction and constraint tools. Positions come from the last valid solution when there is
//! one, else from the stored definition (sketch frame = document millimetres).

use std::f64::consts::TAU;

use burnburn_document::DocumentSnapshot;
use burnburn_document::sketch::{Entity, EntityId, Sketch, SketchId};
use burnburn_geometry::{Point, math};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PickKind {
    Point,
    Line,
    Circle,
    Arc,
    Ellipse,
}

impl PickKind {
    pub(super) fn is_curve(self) -> bool {
        matches!(self, PickKind::Circle | PickKind::Arc)
    }
}

/// One sketch entity under the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SketchPick {
    pub sketch: SketchId,
    pub entity: EntityId,
    pub kind: PickKind,
}

/// Where a point entity is: solved position first, stored position otherwise.
pub(super) fn point_of(sketch: &Sketch, id: EntityId) -> Option<Point> {
    if let Some(p) = sketch.solution().and_then(|s| s.points.get(&id)) {
        return Some(*p);
    }
    match sketch.entity(id)?.entity {
        Entity::Point { at } => Some(at),
        _ => None,
    }
}

/// The radius of a circle entity: solved first, stored otherwise.
pub(super) fn radius_of(sketch: &Sketch, id: EntityId) -> Option<f64> {
    if let Some(r) = sketch.solution().and_then(|s| s.radii.get(&id)) {
        return Some(*r);
    }
    match sketch.entity(id)?.entity {
        Entity::Circle { radius, .. } => Some(radius),
        _ => None,
    }
}

fn segment_distance(a: Point, b: Point, q: Point) -> f64 {
    let d = b - a;
    let len2 = d.dot(d);
    let t = if len2 > 0.0 { ((q - a).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
    a.lerp(b, t).distance(q)
}

/// Distance from `q` to the arc around `c` from `s` to `e`.
fn arc_distance(c: Point, s: Point, e: Point, clockwise: bool, q: Point) -> f64 {
    let angle = |p: Point| math::atan2(p.y - c.y, p.x - c.x);
    let (a0, a1, aq) = (angle(s), angle(e), angle(q));
    let (travelled, to_q) = if clockwise {
        ((a0 - a1).rem_euclid(TAU), (a0 - aq).rem_euclid(TAU))
    } else {
        ((a1 - a0).rem_euclid(TAU), (aq - a0).rem_euclid(TAU))
    };
    // A zero sweep is a full circle (start and end coincide).
    let sweep = if travelled == 0.0 { TAU } else { travelled };
    if to_q <= sweep { (c.distance(q) - c.distance(s)).abs() } else { s.distance(q).min(e.distance(q)) }
}

/// The sketch entity under `at` within `tolerance_mm`: a point beats a curve, else the nearest.
pub(super) fn pick_entity(snapshot: &DocumentSnapshot, at: Point, tolerance_mm: f64) -> Option<SketchPick> {
    let mut best_point: Option<(f64, SketchPick)> = None;
    let mut best_curve: Option<(f64, SketchPick)> = None;
    for (sketch_id, sketch) in snapshot.sketch_layer().sketches() {
        for (id, e) in sketch.entities() {
            let (distance, kind) = match e.entity {
                Entity::Point { .. } => (point_of(sketch, id).map(|p| p.distance(at)), PickKind::Point),
                Entity::Line { start, end } => (
                    point_of(sketch, start).zip(point_of(sketch, end)).map(|(a, b)| segment_distance(a, b, at)),
                    PickKind::Line,
                ),
                Entity::Circle { center, .. } => (
                    point_of(sketch, center).zip(radius_of(sketch, id)).map(|(c, r)| (c.distance(at) - r).abs()),
                    PickKind::Circle,
                ),
                Entity::Arc { center, start, end, clockwise } => (
                    point_of(sketch, center)
                        .zip(point_of(sketch, start))
                        .zip(point_of(sketch, end))
                        .map(|((c, s), e)| arc_distance(c, s, e, clockwise, at)),
                    PickKind::Arc,
                ),
                Entity::Ellipse { center, major, minor } => (
                    point_of(sketch, center)
                        .zip(point_of(sketch, major))
                        .and_then(|(c, m)| crate::sketch::ellipse_distance(c, m, minor, at)),
                    PickKind::Ellipse,
                ),
                _ => continue,
            };
            let Some(distance) = distance.filter(|d| *d <= tolerance_mm) else { continue };
            let slot = if kind == PickKind::Point { &mut best_point } else { &mut best_curve };
            if slot.is_none_or(|(d, _)| distance < d) {
                *slot = Some((distance, SketchPick { sketch: sketch_id, entity: id, kind }));
            }
        }
    }
    best_point.or(best_curve).map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use burnburn_document::sketch::{Entity, Sketch};
    use burnburn_geometry::Point;

    use super::super::modify::kit::*;
    use super::*;

    /// Line (0,0)-(10,0), circle at (30,0) r 5, quarter arc around (0,20) from (5,20) to (0,25).
    fn sketch() -> Sketch {
        let mut s = Sketch::new("S");
        let a = s.point(0.0, 0.0).unwrap();
        let b = s.point(10.0, 0.0).unwrap();
        s.line(a, b).unwrap();
        let c = s.point(30.0, 0.0).unwrap();
        s.circle(c, 5.0).unwrap();
        let (m, p, q) = (s.point(0.0, 20.0).unwrap(), s.point(5.0, 20.0).unwrap(), s.point(0.0, 25.0).unwrap());
        s.arc(m, p, q, false).unwrap();
        s
    }

    fn pick_at(x: f64, y: f64) -> Option<SketchPick> {
        let f = with_sketch(vec![], sketch());
        pick_entity(&f.snapshot, Point::new(x, y), 1.0)
    }

    #[test]
    fn a_point_beats_the_line_it_ends() {
        let p = pick_at(0.2, 0.1).unwrap();
        assert_eq!((p.kind, p.entity.raw()), (PickKind::Point, 0));
    }

    #[test]
    fn lines_circles_and_arcs_are_hit_on_their_curve_only() {
        assert_eq!(pick_at(5.0, 0.4).map(|p| (p.kind, p.entity.raw())), Some((PickKind::Line, 2)));
        assert_eq!(pick_at(35.0, 0.3).map(|p| (p.kind, p.entity.raw())), Some((PickKind::Circle, 4)));
        assert_eq!(pick_at(30.0, 0.0).map(|p| p.kind), Some(PickKind::Point), "the centre is a point, not the circle");
        let on_arc = Point::new(5.0 * std::f64::consts::FRAC_1_SQRT_2, 20.0 + 5.0 * std::f64::consts::FRAC_1_SQRT_2);
        assert_eq!(pick_at(on_arc.x, on_arc.y).map(|p| p.kind), Some(PickKind::Arc));
        assert_eq!(pick_at(-3.5, 16.5).map(|p| p.kind), None, "the other half of the circle is not the arc");
        assert!(pick_at(15.0, 15.0).is_none());
    }

    #[test]
    fn the_solved_positions_win_over_the_stored_ones() {
        use burnburn_document::sketch::SketchSolution;
        let mut s = Sketch::new("S");
        let a = s.point(0.0, 0.0).unwrap();
        let b = s.point(10.0, 0.0).unwrap();
        s.line(a, b).unwrap();
        let mut sol = SketchSolution::default();
        sol.points.insert(a, Point::new(0.0, 5.0));
        sol.points.insert(b, Point::new(10.0, 5.0));
        let s = Sketch::from_parts(
            "S".into(),
            s.entities().map(|(i, e)| (i, *e)).collect(),
            vec![],
            Default::default(),
            3,
            0,
            Some(sol),
        )
        .unwrap();
        let f = with_sketch(vec![], s);
        assert!(pick_entity(&f.snapshot, Point::new(5.0, 0.0), 1.0).is_none());
        assert_eq!(pick_entity(&f.snapshot, Point::new(5.0, 5.2), 1.0).map(|p| p.kind), Some(PickKind::Line));
        let line = pick_entity(&f.snapshot, Point::new(5.0, 5.2), 1.0).unwrap();
        let sketch = f.snapshot.sketch(line.sketch).unwrap();
        assert_eq!(point_of(sketch, a), Some(Point::new(0.0, 5.0)));
        let _ = Entity::Point { at: Point::ORIGIN };
    }
}
