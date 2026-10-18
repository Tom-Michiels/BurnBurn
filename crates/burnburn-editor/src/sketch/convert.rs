//! Drawn objects to a sketch: straight-sided contours and full circles become points, lines and
//! circles that share their end points. No relations are added: the user chooses them.

use burnburn_document::sketch::{EntityId, Sketch};
use burnburn_geometry::{Path, Point, Segment};

/// Two points closer than this (mm) are one corner.
const SAME_POINT_MM: f64 = 1e-9;

/// A sketch named `name` holding the geometry of `paths`, or `None` when a path has curves other
/// than a full circle or there is nothing to convert.
pub fn sketch_from_paths(name: &str, paths: &[&Path]) -> Option<Sketch> {
    let mut sketch = Sketch::new(name);
    for path in paths {
        for sub in path.subpaths() {
            if let [Segment::Arc(arc)] = sub.segments() {
                let (u, v) = arc.semi_diameters();
                let full = (arc.sweep_parameter().abs() - std::f64::consts::TAU).abs() < 1e-9;
                let round = (u.length() - v.length()).abs() < 1e-9 && u.dot(v).abs() < 1e-9;
                if !(full && round) {
                    return None;
                }
                let center = sketch.point(arc.center().x, arc.center().y).ok()?;
                sketch.circle(center, u.length()).ok()?;
                continue;
            }
            let mut corners: Vec<Point> = vec![sub.start()];
            for segment in sub.segments() {
                match segment {
                    Segment::Line { to } => corners.push(*to),
                    _ => return None,
                }
            }
            let closed = sub.is_closed();
            if closed && corners.len() > 1 && corners.last().is_some_and(|p| p.distance(sub.start()) < SAME_POINT_MM) {
                corners.pop();
            }
            if corners.len() < 2 {
                continue;
            }
            let ids: Vec<EntityId> = corners.iter().map(|p| sketch.point(p.x, p.y)).collect::<Result<_, _>>().ok()?;
            for pair in ids.windows(2) {
                sketch.line(pair[0], pair[1]).ok()?;
            }
            if closed && ids.len() >= 3 {
                sketch.line(ids[ids.len() - 1], ids[0]).ok()?;
            }
        }
    }
    let any = sketch.entities().len() > 0;
    any.then_some(sketch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use burnburn_document::sketch::Entity;
    use burnburn_geometry::{EllipticalArc, PathBuilder, Point, Rect};
    use burnburn_types::{Angle, Length};

    fn count(s: &Sketch, f: impl Fn(&Entity) -> bool) -> usize {
        s.entities().filter(|(_, e)| f(&e.entity)).count()
    }

    fn points(s: &Sketch) -> usize {
        count(s, |e| matches!(e, Entity::Point { .. }))
    }

    fn lines(s: &Sketch) -> usize {
        count(s, |e| matches!(e, Entity::Line { .. }))
    }

    #[test]
    fn a_rectangle_becomes_four_shared_corners_and_four_sides() {
        let rect =
            Rect::new(Point::new(0.0, 0.0), Length::from_mm(20.0).unwrap(), Length::from_mm(10.0).unwrap()).unwrap();
        let path = Path::rectangle(rect);
        let s = sketch_from_paths("Van rechthoek", &[&path]).unwrap();
        assert_eq!((points(&s), lines(&s)), (4, 4));
        assert_eq!(s.name(), "Van rechthoek");
        assert_eq!(s.constraints().len(), 0, "no relations are guessed");
    }

    #[test]
    fn an_open_polyline_keeps_its_ends_free() {
        let path = PathBuilder::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(10.0, 10.0))
            .build()
            .unwrap();
        let s = sketch_from_paths("Lijn", &[&path]).unwrap();
        assert_eq!((points(&s), lines(&s)), (3, 2));
    }

    #[test]
    fn a_closed_polyline_whose_last_point_repeats_the_first_has_no_double_corner() {
        let path = PathBuilder::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(10.0, 0.0))
            .line_to(Point::new(5.0, 8.0))
            .line_to(Point::new(0.0, 0.0))
            .close()
            .build()
            .unwrap();
        let s = sketch_from_paths("Driehoek", &[&path]).unwrap();
        assert_eq!((points(&s), lines(&s)), (3, 3));
    }

    #[test]
    fn a_full_circle_becomes_a_centre_and_a_circle() {
        let arc = EllipticalArc::circle(
            Point::new(5.0, 5.0),
            3.0,
            Angle::ZERO,
            Angle::from_radians(std::f64::consts::TAU).unwrap(),
        )
        .unwrap();
        let path = PathBuilder::new().move_to(arc.start_point()).arc(arc).close().build().unwrap();
        let s = sketch_from_paths("Cirkel", &[&path]).unwrap();
        assert_eq!(
            (points(&s), count(&s, |e| matches!(e, Entity::Circle { radius, .. } if (*radius - 3.0).abs() < 1e-9))),
            (1, 1)
        );
    }

    #[test]
    fn a_curve_cannot_be_converted() {
        let path = PathBuilder::new()
            .move_to(Point::new(0.0, 0.0))
            .cubic_to(Point::new(0.0, 10.0), Point::new(10.0, 10.0), Point::new(10.0, 0.0))
            .build()
            .unwrap();
        assert!(sketch_from_paths("Kromme", &[&path]).is_none());
        assert!(sketch_from_paths("Niets", &[]).is_none());
    }

    #[test]
    fn several_paths_make_one_sketch() {
        let a = Path::line(Point::new(0.0, 0.0), Point::new(5.0, 0.0)).unwrap();
        let b = Path::line(Point::new(0.0, 9.0), Point::new(5.0, 9.0)).unwrap();
        let s = sketch_from_paths("Twee", &[&a, &b]).unwrap();
        assert_eq!((points(&s), lines(&s)), (4, 2));
    }
}
