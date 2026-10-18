//! Ellipse from centre, major-axis end and a minor-axis point.

use burnburn_geometry::{Point, Shape, ellipse_from_axes};
use burnburn_types::Length;

use super::ShapeKind;
use super::click::{Out, Variant};

pub(super) fn build(variant: Variant, pts: &[Point]) -> Option<Out> {
    if variant != Variant::EllipseAxes {
        return None;
    }
    let spec = ellipse_from_axes(*pts.first()?, *pts.get(1)?, *pts.get(2)?).ok()?;
    let shape = Shape::Ellipse {
        center: spec.center,
        rx: Length::from_mm(spec.rx).ok()?,
        ry: Length::from_mm(spec.ry).ok()?,
        rotation: spec.rotation,
    };
    Some(Out::Shape(shape, ShapeKind::Ellipse))
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use burnburn_geometry::{Point, Shape};

    use super::super::click::kit::*;
    use super::super::fixture::close;
    use super::super::{EditCommit, ToolKind};

    #[test]
    fn ellipse_from_centre_major_end_and_minor_point() {
        let (_, steps) = clicks(ToolKind::EllipseAxes, &[(1.0, 1.0), (5.0, 1.0), (2.0, 3.0)]);
        let EditCommit::CreateShape { shape: Shape::Ellipse { center, rx, ry, rotation } } = only(&steps) else {
            panic!("{steps:?}")
        };
        assert!(close(center, Point::new(1.0, 1.0)) && near(rx.mm(), 4.0) && near(ry.mm(), 2.0));
        assert!(near(rotation.radians(), 0.0));
    }

    #[test]
    fn ellipse_with_a_tilted_major_axis_keeps_its_rotation_and_area() {
        let (_, steps) = clicks(ToolKind::EllipseAxes, &[(1.0, 1.0), (1.0, 5.0), (-1.0, 2.0)]);
        let EditCommit::CreateShape { shape: Shape::Ellipse { rx, ry, rotation, .. } } = only(&steps) else {
            panic!("{steps:?}")
        };
        assert!(near(rx.mm(), 4.0) && near(ry.mm(), 2.0) && near(rotation.radians(), FRAC_PI_2));
        let area = outline(&only(&steps)).signed_area();
        assert!(near(area, std::f64::consts::PI * 8.0), "{area}");
    }

    #[test]
    fn ellipse_with_the_minor_point_on_the_axis_cancels() {
        let (_, steps) = clicks(ToolKind::EllipseAxes, &[(0.0, 0.0), (4.0, 0.0), (2.0, 0.0)]);
        assert!(commits(&steps).is_empty(), "{steps:?}");
    }
}
