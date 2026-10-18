//! The chamfer tools (equal distance, two distances, distance and angle) share the corner engine
//! of the fillet tool.

use super::ToolKind;
use super::fillet::{CornerAction, Cut};

/// The corner action of a chamfer tool kind.
pub(super) fn action(kind: ToolKind) -> CornerAction {
    CornerAction::new(match kind {
        ToolKind::ChamferTwo => Cut::ChamferTwo,
        ToolKind::ChamferAngle => Cut::ChamferAngle,
        _ => Cut::ChamferEqual,
    })
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::CornerCut;

    use super::super::fixture::*;
    use super::super::modify::ModifyParam;
    use super::super::modify::kit::*;
    use super::super::{EditCommit, ToolKind};

    fn square() -> Fixture {
        with_objects(vec![rect(1, 0.0, 0.0, 10.0, 10.0)])
    }

    fn cut(steps: &[super::super::ToolStep]) -> (CornerCut, f64) {
        let EditCommit::CutCorner { cut, path, .. } = only(steps) else { panic!("cut") };
        (cut, path.signed_area())
    }

    #[test]
    fn equal_distance_removes_the_analytic_triangle() {
        let (f, mut ed) = (square(), editor(ToolKind::ChamferEqual));
        param(&mut ed, ModifyParam::Distance(3.0));
        let (c, area) = cut(&click(&mut ed, &f, 0.3, 0.3));
        assert_eq!(c, CornerCut::ChamferEqual { distance: 3.0 });
        assert!((area - 95.5).abs() < 1e-9, "{area}");
    }

    #[test]
    fn two_distances_apply_to_the_side_nearer_to_the_click() {
        let (f, mut ed) = (square(), editor(ToolKind::ChamferTwo));
        param(&mut ed, ModifyParam::Distance(3.0));
        param(&mut ed, ModifyParam::Distance2(4.0));
        let (c, area) = cut(&click(&mut ed, &f, 2.0, 0.1));
        assert_eq!(c, CornerCut::ChamferTwo { first: 3.0, second: 4.0 });
        assert!((area - 94.0).abs() < 1e-9, "{area}");
    }

    #[test]
    fn distance_and_angle_follow_the_law_of_sines() {
        let (f, mut ed) = (square(), editor(ToolKind::ChamferAngle));
        param(&mut ed, ModifyParam::Distance(3.0));
        param(&mut ed, ModifyParam::AngleDeg(30.0));
        let (c, area) = cut(&click(&mut ed, &f, 2.0, 0.1));
        assert_eq!(c, CornerCut::ChamferAngle { distance: 3.0, angle_deg: 30.0 });
        let d2 = 3.0 * (std::f64::consts::PI / 6.0).sin() / (2.0 * std::f64::consts::PI / 3.0).sin();
        assert!((area - (100.0 - 1.5 * d2)).abs() < 1e-9, "{area}");
    }

    #[test]
    fn invalid_settings_are_refused_by_the_tool() {
        let mut ed = editor(ToolKind::ChamferAngle);
        assert!(!ed.set_param_ok(ModifyParam::AngleDeg(0.0)));
        assert!(!ed.set_param_ok(ModifyParam::AngleDeg(180.0)));
        assert!(!ed.set_param_ok(ModifyParam::Distance(-1.0)));
        assert!(!ed.set_param_ok(ModifyParam::Radius(1.0)), "radius belongs to the fillet");
    }
}
