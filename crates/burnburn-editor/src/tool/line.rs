//! Line variants: midpoint line and centerline (construction).

use burnburn_geometry::{Point, Shape};

use super::ShapeKind;
use super::click::{Out, Variant};

pub(super) fn build(variant: Variant, pts: &[Point]) -> Option<Out> {
    let (&a, &b) = (pts.first()?, pts.get(1)?);
    if a == b {
        return None;
    }
    match variant {
        // The first click is the middle: the line runs from the mirrored point to the second click.
        Variant::LineMidpoint => {
            let from = a + (a - b);
            Some(Out::Shape(Shape::Line { from, to: b }, ShapeKind::Line))
        }
        Variant::Centerline => Some(Out::Construction(Shape::Line { from: a, to: b })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::{Point, Shape};
    use burnburn_types::Length;

    use super::super::click::kit::*;
    use super::super::fixture::*;
    use super::super::{EditCommit, ToolKind};
    use crate::input::PointerPhase;

    fn line_of(c: &EditCommit) -> (Point, Point) {
        match c {
            EditCommit::CreateShape { shape: Shape::Line { from, to } } => (*from, *to),
            other => panic!("not a line: {other:?}"),
        }
    }

    #[test]
    fn midpoint_line_is_symmetric_about_the_first_click() {
        let (_, steps) = clicks(ToolKind::LineMidpoint, &[(10.0, 10.0), (14.0, 13.0)]);
        let (from, to) = line_of(&only(&steps));
        assert!(close(from, Point::new(6.0, 7.0)) && close(to, Point::new(14.0, 13.0)), "{from:?} {to:?}");
    }

    #[test]
    fn midpoint_line_by_dragging_from_the_middle() {
        let (f, mut ed) = (empty(), editor(ToolKind::LineMidpoint));
        let steps = drag(&mut ed, &f, (10.0, 10.0), (14.0, 13.0));
        let (from, to) = line_of(&only(&steps));
        assert!(close(from, Point::new(6.0, 7.0)) && close(to, Point::new(14.0, 13.0)));
    }

    #[test]
    fn typed_length_of_a_midpoint_line_is_the_full_length() {
        let (f, mut ed) = (empty(), editor(ToolKind::LineMidpoint));
        ed.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
        ed.input(&at(PointerPhase::Move, 3.0, 0.0, none()), &f.ctx());
        let mm = |v| Length::from_mm(v).unwrap();
        let (from, to) = line_of(&only(&[ed.exact_size(mm(10.0), mm(0.0))]));
        assert!(close(from, Point::new(-5.0, 0.0)) && close(to, Point::new(5.0, 0.0)), "{from:?} {to:?}");
    }

    #[test]
    fn centerline_is_construction_geometry_not_a_production_line() {
        let (f, mut ed) = (empty(), editor(ToolKind::Centerline));
        let steps = drag(&mut ed, &f, (0.0, 0.0), (20.0, 0.0));
        let EditCommit::CreateConstruction { shape: Shape::Line { from, to } } = only(&steps) else {
            panic!("a centerline must be a construction commit: {steps:?}")
        };
        assert!(close(from, Point::new(0.0, 0.0)) && close(to, Point::new(20.0, 0.0)));
        // Click-click works too and creates the same.
        let (_, steps) = clicks(ToolKind::Centerline, &[(0.0, 0.0), (0.0, 7.0)]);
        assert!(matches!(only(&steps), EditCommit::CreateConstruction { .. }));
    }
}
