//! The fillet and chamfer tools: a click near a corner between two straight segments cuts it.
//! The result is computed by `Path::cut_corner`; the document re-applies the same edit.

use burnburn_geometry::{CornerCut, CornerError, Point};
use burnburn_types::{GeometryId, Length};

use super::modify::{Action, ModifyParam, REACH_PT, Rejection, ensure_editable, path_preview, rejected};
use super::{EditCommit, ToolContext, ToolPreview, ToolStep};
use crate::input::PointerEvent;

/// Which corner cut the tool makes and with which values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Cut {
    Fillet,
    ChamferEqual,
    ChamferTwo,
    ChamferAngle,
}

#[derive(Debug, Clone)]
pub(super) struct CornerAction {
    cut: Cut,
    radius: f64,
    distance: f64,
    distance2: f64,
    angle_deg: f64,
    preview: Option<ToolPreview>,
}

fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

impl CornerAction {
    pub(super) fn new(cut: Cut) -> Self {
        CornerAction { cut, radius: 2.0, distance: 2.0, distance2: 2.0, angle_deg: 45.0, preview: None }
    }

    pub(super) fn fillet() -> Self {
        CornerAction::new(Cut::Fillet)
    }

    fn corner_cut(&self) -> CornerCut {
        match self.cut {
            Cut::Fillet => CornerCut::Fillet { radius: self.radius },
            Cut::ChamferEqual => CornerCut::ChamferEqual { distance: self.distance },
            Cut::ChamferTwo => CornerCut::ChamferTwo { first: self.distance, second: self.distance2 },
            Cut::ChamferAngle => CornerCut::ChamferAngle { distance: self.distance, angle_deg: self.angle_deg },
        }
    }

    /// The topmost object with a cuttable corner at `pick`, or why none could be cut.
    fn find(&self, ctx: &ToolContext<'_>, pick: Point) -> Result<EditCommit, Rejection> {
        let reach = ctx.tolerance_mm(REACH_PT);
        let cut = self.corner_cut();
        let mut failure: Option<CornerError> = None;
        let mut ids: Vec<GeometryId> = ctx.document.objects().map(|(id, _)| id).collect();
        ids.reverse();
        for id in ids {
            let Some(obj) = ctx.document.object(id) else { continue };
            match obj.path().cut_corner(pick, reach, cut) {
                Ok(path) => {
                    ensure_editable(ctx, id)?;
                    let reach = Length::from_mm(reach).map_err(|_| Rejection::InvalidValue)?;
                    return Ok(EditCommit::CutCorner { target: id, pick, reach, cut, path });
                }
                Err(CornerError::NoCorner) => {}
                Err(e) => {
                    failure.get_or_insert(e);
                }
            }
        }
        Err(failure.map_or(Rejection::Corner(CornerError::NoCorner), Rejection::from))
    }
}

impl Action for CornerAction {
    fn press(&mut self, ctx: &ToolContext<'_>, _at: Point, ev: &PointerEvent) -> ToolStep {
        self.preview = None;
        match self.find(ctx, ev.position) {
            Ok(commit) => ToolStep::Commit(commit),
            Err(r) => rejected(r),
        }
    }

    fn hover(&mut self, ctx: &ToolContext<'_>, _at: Point, ev: &PointerEvent) {
        self.preview = match self.find(ctx, ev.position) {
            Ok(EditCommit::CutCorner { path, .. }) => Some(path_preview(path)),
            _ => None,
        };
    }

    fn reset(&mut self) -> bool {
        self.preview = None;
        false
    }

    fn preview(&self) -> Option<ToolPreview> {
        self.preview.clone()
    }

    fn set_param(&mut self, param: ModifyParam) -> bool {
        match (self.cut, param) {
            (Cut::Fillet, ModifyParam::Radius(v)) if positive(v) => self.radius = v,
            (Cut::ChamferEqual | Cut::ChamferTwo | Cut::ChamferAngle, ModifyParam::Distance(v)) if positive(v) => {
                self.distance = v;
            }
            (Cut::ChamferTwo, ModifyParam::Distance2(v)) if positive(v) => self.distance2 = v,
            (Cut::ChamferAngle, ModifyParam::AngleDeg(v)) if positive(v) && v < 180.0 => self.angle_deg = v,
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use burnburn_document::sketch::SketchId;
    use burnburn_document::{DerivedKey, DerivedOwner, GeometryObject, GeometrySource, ObjectKind};
    use burnburn_geometry::{CornerCut, CornerError, Path, Point, Segment};
    use burnburn_types::{GeometryId, Length, StableId};

    use super::super::fixture::*;
    use super::super::modify::kit::*;
    use super::super::modify::{ModifyParam, Rejection, Unavailable};
    use super::super::{EditCommit, ToolKind, ToolPreview, ToolStep};
    use crate::input::CanvasKey;

    fn corner() -> Fixture {
        with_objects(vec![polyline(1, &[(10.0, 0.0), (0.0, 0.0), (0.0, 10.0)], false)])
    }

    #[test]
    fn a_click_on_the_corner_commits_the_tangent_arc_five_mm_from_the_corner() {
        let f = corner();
        let mut ed = editor(ToolKind::Fillet);
        param(&mut ed, ModifyParam::Radius(5.0));
        let steps = click(&mut ed, &f, 0.5, 0.5);
        let EditCommit::CutCorner { target, pick, reach, cut, path } = only(&steps) else { panic!("cut") };
        assert_eq!(
            (target, pick, cut),
            (GeometryId::from_raw(1), Point::new(0.5, 0.5), CornerCut::Fillet { radius: 5.0 })
        );
        assert!(reach.mm() > 0.0);
        let segs = path.subpaths()[0].segments();
        let Segment::Arc(arc) = segs[1] else { panic!("arc between the two sides: {segs:?}") };
        assert!(near(arc.center(), Point::new(5.0, 5.0)));
        assert!(near(arc.start_point(), Point::new(5.0, 0.0)) && near(arc.end_point(), Point::new(0.0, 5.0)));
    }

    #[test]
    fn the_default_radius_is_used_until_one_is_set() {
        let f = corner();
        let mut ed = editor(ToolKind::Fillet);
        let EditCommit::CutCorner { cut: CornerCut::Fillet { radius }, .. } = only(&click(&mut ed, &f, 0.5, 0.5))
        else {
            panic!("fillet")
        };
        assert!(radius > 0.0);
    }

    #[test]
    fn hovering_a_corner_previews_the_result_and_hovering_away_clears_it() {
        let f = corner();
        let mut ed = editor(ToolKind::Fillet);
        param(&mut ed, ModifyParam::Radius(5.0));
        hover_to(&mut ed, &f, 0.4, 0.4);
        let Some(ToolPreview::Shape { path, .. }) = ed.preview() else { panic!("preview") };
        assert!(path.subpaths()[0].segments().iter().any(|s| matches!(s, Segment::Arc(_))));
        hover_to(&mut ed, &f, 5.0, 5.0);
        assert!(ed.preview().is_none());
        assert!(commits(&[]).is_empty());
    }

    #[test]
    fn refusals_are_typed_and_commit_nothing() {
        let f = corner();
        let mut ed = editor(ToolKind::Fillet);
        param(&mut ed, ModifyParam::Radius(50.0));
        let steps = click(&mut ed, &f, 0.5, 0.5);
        assert!(commits(&steps).is_empty());
        assert_eq!(rejection(&steps), Some(Rejection::Corner(CornerError::TooLarge)));
        let steps = click(&mut ed, &f, 5.0, 5.0);
        assert_eq!(rejection(&steps), Some(Rejection::Corner(CornerError::NoCorner)));
    }

    #[test]
    fn a_circle_seam_is_smooth_so_it_is_no_corner() {
        let circle = path_obj(2, Path::circle(Point::new(0.0, 0.0), 5.0).unwrap());
        let f = with_objects(vec![circle]);
        let mut ed = editor(ToolKind::Fillet);
        let steps = click(&mut ed, &f, 5.0, 0.0);
        assert!(commits(&steps).is_empty());
        assert_eq!(rejection(&steps), Some(Rejection::Corner(CornerError::Collinear)));
    }

    #[test]
    fn a_corner_between_a_line_and_an_arc_is_rounded_tangent_to_both() {
        // G8 E05: the line comes down the y axis into the lowest point of a circle of radius 10.
        let arc = burnburn_geometry::EllipticalArc::circle(
            Point::new(0.0, 10.0),
            10.0,
            burnburn_types::Angle::from_radians(-std::f64::consts::FRAC_PI_2).unwrap(),
            burnburn_types::Angle::from_radians(std::f64::consts::FRAC_PI_2).unwrap(),
        )
        .unwrap();
        let path = burnburn_geometry::PathBuilder::new()
            .move_to(Point::new(0.0, 20.0))
            .line_to(Point::new(0.0, 0.0))
            .arc(arc)
            .build()
            .unwrap();
        let f = with_objects(vec![path_obj(2, path)]);
        let mut ed = editor(ToolKind::Fillet);
        param(&mut ed, ModifyParam::Radius(2.0));
        let steps = click(&mut ed, &f, 0.1, 0.1);
        let EditCommit::CutCorner { path, .. } = only(&steps) else { panic!("cut") };
        let segs = path.subpaths()[0].segments();
        assert_eq!(segs.len(), 3, "{segs:?}");
        let Segment::Arc(fillet) = segs[1] else { panic!("fillet arc: {segs:?}") };
        assert!((fillet.semi_diameters().0.length() - 2.0).abs() < 1e-9);
        assert!((fillet.center().x - 2.0).abs() < 1e-6, "2 mm from the line: {:?}", fillet.center());
    }

    #[test]
    fn sketch_output_is_edited_through_its_sketch_not_here() {
        let path =
            Path::polyline(&[Point::new(10.0, 0.0), Point::new(0.0, 0.0), Point::new(0.0, 10.0)], false).unwrap();
        let key = DerivedKey { owner: DerivedOwner::Sketch(SketchId::from_raw(100)), local: 0 };
        let obj = GeometryObject::new(
            GeometryId::from_raw(3),
            "outline".into(),
            ObjectKind::Path,
            GeometrySource::Derived(key),
            path,
        );
        let f = with_objects(vec![obj]);
        let mut ed = editor(ToolKind::Fillet);
        let steps = click(&mut ed, &f, 0.5, 0.5);
        assert!(commits(&steps).is_empty());
        assert_eq!(rejection(&steps), Some(Rejection::DerivedGeometry(GeometryId::from_raw(3))));
    }

    #[test]
    fn escape_changes_nothing_and_a_bad_radius_is_not_accepted() {
        let f = corner();
        let mut ed = editor(ToolKind::Fillet);
        assert_eq!(press_key(&mut ed, &f, CanvasKey::Escape), ToolStep::Nothing);
        assert!(!ed.set_param_ok(ModifyParam::Radius(0.0)));
        assert!(!ed.set_param_ok(ModifyParam::Radius(f64::NAN)));
        assert!(!ed.set_param_ok(ModifyParam::Rho(0.5)), "not a fillet setting");
        let _ = Length::from_mm(1.0);
    }
}
