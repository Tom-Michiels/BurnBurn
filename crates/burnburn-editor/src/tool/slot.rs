//! Slot variants: straight (centre-to-centre, overall, centre-point) and along an arc.

use burnburn_geometry::{
    EllipticalArc, Point, arc_center_start_end, arc_through_three, slot_along_arc, slot_center_to_center, slot_overall,
};
use burnburn_types::Angle;

use super::arc::arc_path;
use super::click::{Out, Variant};

/// Twice the distance of `p` from the line through `a` and `b`: the width a click sets.
fn width_from_axis(a: Point, b: Point, p: Point) -> Option<f64> {
    let d = b - a;
    let len = d.length();
    (len > 0.0).then(|| 2.0 * (d.cross(p - a) / len).abs())
}

/// Centreline arc of the two arc-slot variants from its first three points.
fn centerline(variant: Variant, pts: &[Point]) -> Option<EllipticalArc> {
    match variant {
        Variant::SlotArcThreePoint => arc_through_three(*pts.first()?, *pts.get(1)?, *pts.get(2)?).ok(),
        Variant::SlotArcCenter => arc_center_start_end(*pts.first()?, *pts.get(1)?, *pts.get(2)?, true).ok(),
        _ => None,
    }
}

pub(super) fn build(variant: Variant, pts: &[Point]) -> Option<Out> {
    let path = match variant {
        Variant::SlotCenterToCenter => {
            let (a, b, w) = (*pts.first()?, *pts.get(1)?, *pts.get(2)?);
            slot_center_to_center(a, b, width_from_axis(a, b, w)?)
        }
        Variant::SlotOverall => {
            let (a, b, w) = (*pts.first()?, *pts.get(1)?, *pts.get(2)?);
            slot_overall(a, b, width_from_axis(a, b, w)?)
        }
        Variant::SlotCenterPoint => {
            let (c, e, w) = (*pts.first()?, *pts.get(1)?, *pts.get(2)?);
            slot_center_to_center(c + (c - e), e, width_from_axis(c, e, w)?)
        }
        Variant::SlotArcThreePoint | Variant::SlotArcCenter => {
            let arc = centerline(variant, pts)?;
            let (center, w) = (arc.center(), *pts.get(3)?);
            let radius = center.distance(arc.start_point());
            let width = 2.0 * (w.distance(center) - radius).abs();
            let start = Angle::from_radians(arc.start_parameter()).ok()?;
            let sweep = Angle::from_radians(arc.sweep_parameter()).ok()?;
            slot_along_arc(center, radius, start, sweep, width)
        }
        _ => return None,
    };
    path.ok().map(Out::Path)
}

/// The centreline arc while the width of an arc slot is still being chosen.
pub(super) fn guide(variant: Variant, pts: &[Point]) -> Option<Out> {
    if pts.len() != 3 {
        return None;
    }
    arc_path(&centerline(variant, pts)?).map(Out::Path)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::super::click::kit::*;
    use super::super::fixture::*;
    use super::super::{EditCommit, ToolKind};

    fn slot(kind: ToolKind, pts: &[(f64, f64)]) -> burnburn_geometry::Path {
        let (_, steps) = clicks(kind, pts);
        let c = only(&steps);
        assert!(matches!(c, EditCommit::CreatePath { .. }), "{c:?}");
        outline(&c)
    }

    #[test]
    fn centre_to_centre_slot_takes_its_width_from_the_third_click() {
        let path = slot(ToolKind::SlotCenterToCenter, &[(0.0, 0.0), (10.0, 0.0), (5.0, 2.0)]);
        assert!(near(path.signed_area(), 10.0 * 4.0 + PI * 4.0), "{}", path.signed_area());
        let b = path.bounds().unwrap();
        assert!(near(b.min().x, -2.0) && near(b.max().x, 12.0) && near(b.max().y, 2.0), "{b:?}");
    }

    #[test]
    fn centre_to_centre_slot_by_dragging_the_axis_then_clicking_the_width() {
        let (f, mut ed) = (empty(), editor(ToolKind::SlotCenterToCenter));
        let mut steps = drag(&mut ed, &f, (0.0, 0.0), (10.0, 0.0));
        assert!(commits(&steps).is_empty());
        steps.extend(click(&mut ed, &f, 5.0, -2.0));
        let path = outline(&only(&steps));
        assert!(near(path.signed_area(), 10.0 * 4.0 + PI * 4.0));
    }

    #[test]
    fn overall_slot_spans_exactly_the_first_two_clicks() {
        let path = slot(ToolKind::SlotOverall, &[(0.0, 0.0), (14.0, 0.0), (7.0, 2.0)]);
        let b = path.bounds().unwrap();
        assert!(
            near(b.min().x, 0.0) && near(b.max().x, 14.0) && near(b.max().y, 2.0) && near(b.min().y, -2.0),
            "{b:?}"
        );
    }

    #[test]
    fn overall_slot_shorter_than_its_width_cancels() {
        let (_, steps) = clicks(ToolKind::SlotOverall, &[(0.0, 0.0), (3.0, 0.0), (1.5, 2.0)]);
        assert!(commits(&steps).is_empty(), "{steps:?}");
    }

    #[test]
    fn centre_point_slot_is_mirrored_about_the_centre_click() {
        let path = slot(ToolKind::SlotCenterPoint, &[(0.0, 0.0), (5.0, 0.0), (0.0, 1.0)]);
        let b = path.bounds().unwrap();
        assert!(
            near(b.min().x, -6.0) && near(b.max().x, 6.0) && near(b.min().y, -1.0) && near(b.max().y, 1.0),
            "{b:?}"
        );
    }

    #[test]
    fn three_point_arc_slot_follows_the_arc_through_the_clicks() {
        let path = slot(ToolKind::SlotArcThreePoint, &[(10.0, 0.0), (0.0, 10.0), (-10.0, 0.0), (0.0, 11.0)]);
        // Half turn, centreline radius 10, width 2: pi R w + pi h^2 = 21 pi.
        assert!(near(path.signed_area(), 21.0 * PI), "{}", path.signed_area());
    }

    #[test]
    fn centre_start_end_arc_slot() {
        let path = slot(ToolKind::SlotArcCenter, &[(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (12.0, 0.0)]);
        // Quarter turn, radius 10, width 4: (pi/2) * 10 * 4 + pi * 2^2 = 24 pi.
        assert!(near(path.signed_area(), 24.0 * PI), "{}", path.signed_area());
    }

    #[test]
    fn arc_slot_wider_than_its_radius_cancels() {
        let (_, steps) = clicks(ToolKind::SlotArcCenter, &[(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (35.0, 0.0)]);
        assert!(commits(&steps).is_empty(), "{steps:?}");
    }

    #[test]
    fn slot_preview_shows_the_stadium_while_choosing_the_width() {
        let (f, mut ed) = (empty(), editor(ToolKind::SlotCenterToCenter));
        click(&mut ed, &f, 0.0, 0.0);
        click(&mut ed, &f, 10.0, 0.0);
        ed.input(&at(crate::input::PointerPhase::Move, 5.0, 2.0, none()), &f.ctx());
        let Some(crate::tool::ToolPreview::Shape { path, .. }) = ed.preview() else { panic!("no preview") };
        assert!(near(path.signed_area(), 10.0 * 4.0 + PI * 4.0));
    }

    #[test]
    fn typed_length_and_width_of_a_centre_to_centre_slot() {
        use burnburn_types::Length;
        let (f, mut ed) = (empty(), editor(ToolKind::SlotCenterToCenter));
        ed.input(&at(crate::input::PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
        ed.input(&at(crate::input::PointerPhase::Move, 3.0, 0.0, none()), &f.ctx());
        let mm = |v| Length::from_mm(v).unwrap();
        let path = outline(&only(&[ed.exact_size(mm(10.0), mm(4.0))]));
        assert!(near(path.signed_area(), 10.0 * 4.0 + PI * 4.0), "{}", path.signed_area());
    }
}
