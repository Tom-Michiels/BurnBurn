//! Rectangle variants: centre, three-point and rounded.

use burnburn_geometry::{Bounds, Point, Shape, rectangle_three_point, rounded_rectangle};

use super::click::{Out, Params, Variant};
use super::{ShapeKind, rect_from_bounds};

pub(super) fn build(variant: Variant, pts: &[Point], params: &Params) -> Option<Out> {
    match variant {
        Variant::RectCenter => {
            let (&c, &p) = (pts.first()?, pts.get(1)?);
            let rect = rect_from_bounds(Bounds::from_corners(c + (c - p), p))?;
            Some(Out::Rect(rect))
        }
        Variant::RectThreePoint => {
            let corners = rectangle_three_point(*pts.first()?, *pts.get(1)?, *pts.get(2)?).ok()?;
            Some(Out::Shape(Shape::Polyline { points: corners.to_vec(), closed: true }, ShapeKind::Polyline))
        }
        Variant::RectRounded => {
            let bounds = Bounds::from_corners(*pts.first()?, *pts.get(1)?);
            let rect = rect_from_bounds(bounds)?;
            // A radius beyond half the short side would not fit: the corners round as far as they can.
            let radius = params.corner_radius.min(bounds.width().min(bounds.height()) / 2.0);
            Some(Out::Path(rounded_rectangle(rect, radius).ok()?))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use burnburn_geometry::{Point, Shape};
    use burnburn_types::Length;

    use super::super::click::kit::*;
    use super::super::fixture::*;
    use super::super::{EditCommit, ToolKind, ToolParam};
    use crate::input::PointerPhase;

    #[test]
    fn centre_rectangle_grows_symmetrically_from_the_first_click() {
        let (f, mut ed) = (empty(), editor(ToolKind::RectangleCenter));
        let steps = drag(&mut ed, &f, (10.0, 10.0), (15.0, 13.0));
        let EditCommit::CreateRectangle { rect } = only(&steps) else { panic!("{steps:?}") };
        assert!(close(rect.origin(), Point::new(5.0, 7.0)), "{:?}", rect.origin());
        assert!(near(rect.width().mm(), 10.0) && near(rect.height().mm(), 6.0));
    }

    #[test]
    fn centre_rectangle_without_area_cancels() {
        let (_, steps) = clicks(ToolKind::RectangleCenter, &[(0.0, 0.0), (5.0, 0.0)]);
        assert!(commits(&steps).is_empty(), "{steps:?}");
    }

    #[test]
    fn typed_size_of_a_centre_rectangle_is_the_full_width_and_height() {
        let (f, mut ed) = (empty(), editor(ToolKind::RectangleCenter));
        ed.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
        ed.input(&at(PointerPhase::Move, -3.0, 3.0, none()), &f.ctx());
        let mm = |v| Length::from_mm(v).unwrap();
        let EditCommit::CreateRectangle { rect } = only(&[ed.exact_size(mm(20.0), mm(10.0))]) else { panic!() };
        assert!(close(rect.origin(), Point::new(-10.0, -5.0)) && near(rect.width().mm(), 20.0));
    }

    #[test]
    fn three_point_rectangle_is_a_rotated_closed_polygon_through_the_third_click() {
        let (_, steps) = clicks(ToolKind::RectangleThreePoint, &[(0.0, 0.0), (4.0, 0.0), (1.0, 3.0)]);
        let EditCommit::CreateShape { shape: Shape::Polyline { points, closed } } = only(&steps) else {
            panic!("{steps:?}")
        };
        assert!(closed);
        let expect = [Point::new(0.0, 0.0), Point::new(4.0, 0.0), Point::new(4.0, 3.0), Point::new(0.0, 3.0)];
        assert!(points.len() == 4 && points.iter().zip(expect).all(|(a, b)| close(*a, b)), "{points:?}");
        // Rotated by 45 degrees: the area is |edge| * height.
        let (_, steps) = clicks(ToolKind::RectangleThreePoint, &[(0.0, 0.0), (2.0, 2.0), (2.0, 0.0)]);
        let area = outline(&only(&steps)).signed_area().abs();
        assert!(near(area, 8.0_f64.sqrt() * 2.0_f64.sqrt()), "{area}");
    }

    #[test]
    fn three_point_rectangle_with_the_third_click_on_the_edge_cancels() {
        let (_, steps) = clicks(ToolKind::RectangleThreePoint, &[(0.0, 0.0), (4.0, 0.0), (9.0, 0.0)]);
        assert!(commits(&steps).is_empty(), "{steps:?}");
    }

    #[test]
    fn rounded_rectangle_uses_the_tool_radius() {
        let (f, mut ed) = (empty(), editor(ToolKind::RectangleRounded));
        assert!(ed.set_param(ToolParam::CornerRadius(2.0)));
        let steps = drag(&mut ed, &f, (0.0, 0.0), (20.0, 10.0));
        let EditCommit::CreatePath { path } = only(&steps) else { panic!("{steps:?}") };
        assert!(near(path.signed_area(), 200.0 - 16.0 * (1.0 - PI / 4.0)), "{}", path.signed_area());
    }

    #[test]
    fn rounded_rectangle_radius_is_clamped_to_half_the_short_side() {
        let (f, mut ed) = (empty(), editor(ToolKind::RectangleRounded));
        assert!(ed.set_param(ToolParam::CornerRadius(50.0)));
        let steps = drag(&mut ed, &f, (0.0, 0.0), (20.0, 10.0));
        let area = outline(&only(&steps)).signed_area();
        assert!(near(area, 200.0 - (4.0 - PI) * 25.0), "stadium: {area}");
    }

    #[test]
    fn rounded_rectangle_from_a_leftward_drag_is_normalised() {
        let (f, mut ed) = (empty(), editor(ToolKind::RectangleRounded));
        ed.set_param(ToolParam::CornerRadius(1.0));
        let steps = drag(&mut ed, &f, (20.0, 10.0), (0.0, 0.0));
        let b = outline(&only(&steps)).bounds().unwrap();
        assert!(near(b.min().x, 0.0) && near(b.max().x, 20.0) && near(b.max().y, 10.0), "{b:?}");
    }
}
