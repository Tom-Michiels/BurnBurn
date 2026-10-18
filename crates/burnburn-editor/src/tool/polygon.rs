//! Regular polygons: inscribed, circumscribed and by edge.

use burnburn_geometry::{Point, Shape, polygon_circumscribed, polygon_from_edge, polygon_inscribed};
use burnburn_types::Length;

use super::ShapeKind;
use super::click::{Out, Params, Variant};

pub(super) fn build(variant: Variant, pts: &[Point], params: &Params) -> Option<Out> {
    let (&a, &b) = (pts.first()?, pts.get(1)?);
    let spec = match variant {
        Variant::PolygonInscribed => polygon_inscribed(a, b, params.sides),
        Variant::PolygonCircumscribed => polygon_circumscribed(a, b, params.sides),
        Variant::PolygonEdge => polygon_from_edge(a, b, params.sides),
        _ => return None,
    }
    .ok()?;
    let shape = Shape::Polygon {
        center: spec.center,
        radius: Length::from_mm(spec.radius).ok()?,
        sides: spec.sides,
        rotation: spec.rotation,
    };
    Some(Out::Shape(shape, ShapeKind::Polyline))
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::{Point, Shape};

    use super::super::click::kit::*;
    use super::super::fixture::*;
    use super::super::{EditCommit, ToolKind, ToolParam};

    #[test]
    fn inscribed_polygon_has_its_first_vertex_on_the_second_click() {
        let (f, mut ed) = (empty(), editor(ToolKind::PolygonInscribed));
        let steps = drag(&mut ed, &f, (0.0, 0.0), (10.0, 0.0));
        let EditCommit::CreateShape { shape: Shape::Polygon { center, radius, sides, rotation } } = only(&steps) else {
            panic!("{steps:?}")
        };
        assert!(
            close(center, Point::new(0.0, 0.0))
                && near(radius.mm(), 10.0)
                && sides == 6
                && near(rotation.radians(), 0.0)
        );
    }

    #[test]
    fn the_side_count_is_a_tool_parameter() {
        let (f, mut ed) = (empty(), editor(ToolKind::PolygonInscribed));
        assert!(ed.set_param(ToolParam::Sides(5)));
        assert!(!ed.set_param(ToolParam::Sides(2)), "fewer than three sides is rejected");
        let steps = drag(&mut ed, &f, (0.0, 0.0), (10.0, 0.0));
        let EditCommit::CreateShape { shape: Shape::Polygon { sides, .. } } = only(&steps) else { panic!("{steps:?}") };
        assert_eq!(sides, 5);
        // Regular pentagon of circumradius 10: area = 5/2 r^2 sin(72 degrees).
        let area = outline(&only(&steps)).signed_area();
        assert!(near(area, 2.5 * 100.0 * (72.0_f64.to_radians()).sin()), "{area}");
    }

    #[test]
    fn circumscribed_polygon_touches_the_click_with_an_edge_midpoint() {
        let (f, mut ed) = (empty(), editor(ToolKind::PolygonCircumscribed));
        ed.set_param(ToolParam::Sides(4));
        let steps = drag(&mut ed, &f, (0.0, 0.0), (5.0, 0.0));
        let path = outline(&only(&steps));
        assert!(near(path.signed_area(), 100.0), "square with apothem 5: {}", path.signed_area());
        let b = path.bounds().unwrap();
        assert!(near(b.max().x, 5.0) && near(b.min().x, -5.0), "{b:?}");
    }

    #[test]
    fn edge_polygon_uses_both_clicks_as_one_side() {
        let (_, steps) = clicks(ToolKind::PolygonEdge, &[(0.0, 0.0), (2.0, 0.0)]);
        let EditCommit::CreateShape { shape: Shape::Polygon { center, radius, .. } } = only(&steps) else { panic!() };
        // Default hexagon: side = circumradius, apothem = side * sqrt(3) / 2.
        assert!(near(radius.mm(), 2.0) && close(center, Point::new(1.0, 3.0_f64.sqrt())), "{center:?} {radius:?}");
        let path = outline(&only(&steps));
        assert!(close(path.subpaths()[0].start(), Point::new(0.0, 0.0)));
        assert!(close(path.subpaths()[0].segments()[0].end(), Point::new(2.0, 0.0)));
    }

    #[test]
    fn typed_circumscribed_diameter_of_an_inscribed_polygon() {
        use burnburn_types::Length;
        let (f, mut ed) = (empty(), editor(ToolKind::PolygonInscribed));
        ed.input(&at(crate::input::PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
        ed.input(&at(crate::input::PointerPhase::Move, 4.0, 0.0, none()), &f.ctx());
        let mm = |v| Length::from_mm(v).unwrap();
        let EditCommit::CreateShape { shape: Shape::Polygon { radius, .. } } =
            only(&[ed.exact_size(mm(30.0), mm(0.0))])
        else {
            panic!()
        };
        assert!(near(radius.mm(), 15.0));
    }
}
