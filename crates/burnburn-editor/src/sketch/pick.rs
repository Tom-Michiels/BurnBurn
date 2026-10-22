//! What a click on the canvas picks in a sketch.

use burnburn_document::sketch::{ConstraintId, Entity, EntityId};
use burnburn_geometry::Point;

use super::geometry::SketchGeometry;

/// The entity under `at` within `tol_mm`: points first (nearest), then lines and circles
/// (nearest). `None` when nothing is that close.
pub fn pick_entity(geometry: &SketchGeometry<'_>, at: Point, tol_mm: f64) -> Option<EntityId> {
    let sketch = geometry.sketch();
    let nearest = |candidates: Vec<(EntityId, f64)>| {
        candidates.into_iter().filter(|(_, d)| *d <= tol_mm).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(id, _)| id)
    };
    let points = sketch.entities().filter_map(|(id, _)| geometry.point(id).map(|p| (id, p.distance(at)))).collect();
    if let Some(id) = nearest(points) {
        return Some(id);
    }
    let curves = sketch
        .entities()
        .filter_map(|(id, e)| match e.entity {
            Entity::Line { .. } => geometry.line(id).map(|(a, b)| (id, distance_to_segment(at, a, b))),
            Entity::Circle { .. } => geometry.circle(id).map(|(c, r)| (id, (c.distance(at) - r).abs())),
            Entity::Ellipse { .. } => {
                let (c, m, minor) = geometry.ellipse(id)?;
                super::geometry::ellipse_distance(c, m, minor, at).map(|d| (id, d))
            }
            Entity::Arc { start, end, clockwise, .. } => {
                let (c, r) = geometry.circle(id)?;
                let on_arc = on_arc_range(c, geometry.point(start)?, geometry.point(end)?, clockwise, at);
                on_arc.then(|| (id, (c.distance(at) - r).abs()))
            }
            _ => None,
        })
        .collect();
    nearest(curves)
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (ab, ap) = (b - a, p - a);
    let len2 = ab.dot(ab);
    let t = if len2 > 0.0 { (ap.dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t).distance(p)
}

/// Whether the direction from `c` to `p` lies within the arc from `start` to `end`.
fn on_arc_range(c: Point, start: Point, end: Point, clockwise: bool, p: Point) -> bool {
    let angle = |q: Point| (q.y - c.y).atan2(q.x - c.x);
    let tau = std::f64::consts::TAU;
    let sweep = |from: f64, to: f64| {
        let d = if clockwise { from - to } else { to - from };
        d.rem_euclid(tau)
    };
    let (a0, a1, ap) = (angle(start), angle(end), angle(p));
    sweep(a0, ap) <= sweep(a0, a1)
}

/// The constraint whose glyph or label (`anchors`) is within `tol_mm` of `at`, nearest first.
pub fn pick_constraint(anchors: &[(ConstraintId, Point)], at: Point, tol_mm: f64) -> Option<ConstraintId> {
    anchors
        .iter()
        .map(|(id, p)| (*id, p.distance(at)))
        .filter(|(_, d)| *d <= tol_mm)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use burnburn_document::sketch::Sketch;

    /// p0 (0,0) — p1 (10,0) with line e2; circle e4 around p3 (30,0), radius 5.
    fn sketch() -> (Sketch, [EntityId; 5]) {
        let mut s = Sketch::new("t");
        let p0 = s.point(0.0, 0.0).unwrap();
        let p1 = s.point(10.0, 0.0).unwrap();
        let line = s.line(p0, p1).unwrap();
        let c = s.point(30.0, 0.0).unwrap();
        let circle = s.circle(c, 5.0).unwrap();
        (s, [p0, p1, line, c, circle])
    }

    #[test]
    fn a_point_wins_over_the_line_it_ends() {
        let (s, [p0, ..]) = sketch();
        let g = SketchGeometry::new(&s);
        assert_eq!(pick_entity(&g, Point::new(0.2, 0.1), 0.5), Some(p0));
    }

    #[test]
    fn a_line_is_picked_along_its_length_within_tolerance() {
        let (s, [_, _, line, ..]) = sketch();
        let g = SketchGeometry::new(&s);
        assert_eq!(pick_entity(&g, Point::new(5.0, 0.4), 0.5), Some(line));
        assert_eq!(pick_entity(&g, Point::new(5.0, 0.6), 0.5), None, "outside the tolerance");
        assert_eq!(pick_entity(&g, Point::new(12.0, 0.0), 0.5), None, "beyond the end is not on the segment");
    }

    #[test]
    fn a_circle_is_picked_on_its_rim_not_in_its_middle() {
        let (s, [_, _, _, c, circle]) = sketch();
        let g = SketchGeometry::new(&s);
        assert_eq!(pick_entity(&g, Point::new(35.0, 0.3), 0.5), Some(circle));
        assert_eq!(pick_entity(&g, Point::new(32.0, 0.0), 0.5), None, "inside, away from the rim and the centre");
        assert_eq!(pick_entity(&g, Point::new(30.1, 0.0), 0.5), Some(c), "the centre point wins");
    }

    #[test]
    fn an_ellipse_is_picked_on_its_outline_not_in_its_middle() {
        let mut s = Sketch::new("t");
        let (c, m) = (s.point(0.0, 0.0).unwrap(), s.point(10.0, 0.0).unwrap());
        let ellipse = s.ellipse(c, m, 4.0).unwrap();
        let g = SketchGeometry::new(&s);
        assert_eq!(pick_entity(&g, Point::new(0.0, 4.2), 0.5), Some(ellipse), "near the minor vertex");
        // (5, 4 * sqrt(3) / 2) lies exactly on x^2/100 + y^2/16 = 1.
        assert_eq!(pick_entity(&g, Point::new(5.0, 3.464), 0.5), Some(ellipse));
        assert_eq!(pick_entity(&g, Point::new(5.0, 0.0), 0.5), None, "inside, far from the outline");
        assert_eq!(pick_entity(&g, Point::new(0.0, 5.5), 0.5), None, "outside the tolerance");
        assert_eq!(pick_entity(&g, Point::new(0.1, 0.0), 0.5), Some(c), "the centre point wins");
    }

    #[test]
    fn the_solved_position_is_what_is_picked() {
        let (mut s, [p0, p1, ..]) = sketch();
        let mut sol = burnburn_document::sketch::SketchSolution::default();
        sol.points.insert(p0, Point::new(0.0, 4.0));
        sol.points.insert(p1, Point::new(10.0, 4.0));
        s = burnburn_document::sketch::Sketch::from_parts(
            "t".into(),
            s.entities().map(|(i, e)| (i, *e)).collect(),
            vec![],
            Default::default(),
            s.next_entity_id().raw(),
            0,
            Some(sol),
        )
        .unwrap();
        let g = SketchGeometry::new(&s);
        assert_eq!(pick_entity(&g, Point::new(0.0, 4.0), 0.5), Some(p0));
        assert_eq!(pick_entity(&g, Point::new(0.0, 0.0), 0.5), None);
    }

    #[test]
    fn the_nearest_constraint_anchor_within_tolerance_is_picked() {
        let anchors =
            [(ConstraintId::from_raw(1), Point::new(0.0, 0.0)), (ConstraintId::from_raw(2), Point::new(1.0, 0.0))];
        assert_eq!(pick_constraint(&anchors, Point::new(0.8, 0.0), 0.5), Some(ConstraintId::from_raw(2)));
        assert_eq!(pick_constraint(&anchors, Point::new(5.0, 0.0), 0.5), None);
    }
}
