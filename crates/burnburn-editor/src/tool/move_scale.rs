//! The move and scale tools: base point and target point(s), applied to the selection as one transform.

use burnburn_geometry::{Affine, Point};
use burnburn_types::GeometryId;

use super::modify::{Action, ModifyParam, Rejection, rejected};
use super::{EditCommit, ToolContext, ToolPreview, ToolStep, TransformKind};
use crate::input::PointerEvent;

/// Displacements and distances below this are none (float noise, not a user tolerance).
const MIN_MM: f64 = 1e-9;

fn translation(from: Point, to: Point) -> Option<Affine> {
    Affine::translate(to - from).ok()
}

fn preview_of(targets: &[GeometryId], transform: Affine, kind: TransformKind) -> ToolPreview {
    ToolPreview::Transform { targets: targets.to_vec(), transform, kind }
}

#[derive(Debug, Clone, Default)]
pub(super) struct MoveAction {
    base: Option<Point>,
    hover: Option<Point>,
    targets: Vec<GeometryId>,
}

impl Action for MoveAction {
    fn press(&mut self, ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) -> ToolStep {
        if ctx.selection.is_empty() {
            self.reset();
            return rejected(Rejection::NeedsSelection);
        }
        let Some(base) = self.base else {
            self.base = Some(at);
            self.targets = ctx.selection.ids().to_vec();
            return ToolStep::PreviewChanged;
        };
        let targets = std::mem::take(&mut self.targets);
        self.reset();
        if base.distance(at) <= MIN_MM {
            return ToolStep::Cancelled;
        }
        match translation(base, at) {
            Some(transform) => {
                ToolStep::Commit(EditCommit::Transform { targets, transform, kind: TransformKind::Move })
            }
            None => rejected(Rejection::InvalidValue),
        }
    }

    fn hover(&mut self, _ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) {
        self.hover = Some(at);
    }

    fn reset(&mut self) -> bool {
        self.hover = None;
        self.targets.clear();
        self.base.take().is_some()
    }

    fn preview(&self) -> Option<ToolPreview> {
        let transform = translation(self.base?, self.hover?)?;
        Some(preview_of(&self.targets, transform, TransformKind::Move))
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct ScaleAction {
    base: Option<Point>,
    reference: Option<Point>,
    hover: Option<Point>,
    targets: Vec<GeometryId>,
    /// A typed factor: the base click applies it at once.
    factor: Option<f64>,
}

fn scale_about(base: Point, factor: f64) -> Option<Affine> {
    Affine::scale_about(base, factor, factor).ok()
}

impl ScaleAction {
    fn finish(&mut self, base: Point, factor: f64) -> ToolStep {
        let targets = std::mem::take(&mut self.targets);
        self.reset();
        match scale_about(base, factor).filter(|_| factor.is_finite() && factor > MIN_MM) {
            Some(transform) => {
                ToolStep::Commit(EditCommit::Transform { targets, transform, kind: TransformKind::Scale })
            }
            None => rejected(Rejection::InvalidValue),
        }
    }
}

impl Action for ScaleAction {
    fn press(&mut self, ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) -> ToolStep {
        if ctx.selection.is_empty() {
            self.reset();
            return rejected(Rejection::NeedsSelection);
        }
        let Some(base) = self.base else {
            self.base = Some(at);
            self.targets = ctx.selection.ids().to_vec();
            return match self.factor {
                Some(f) => self.finish(at, f),
                None => ToolStep::PreviewChanged,
            };
        };
        let Some(reference) = self.reference else {
            if base.distance(at) <= MIN_MM {
                self.reset();
                return rejected(Rejection::InvalidValue);
            }
            self.reference = Some(at);
            return ToolStep::PreviewChanged;
        };
        let factor = base.distance(at) / base.distance(reference);
        self.finish(base, factor)
    }

    fn hover(&mut self, _ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) {
        self.hover = Some(at);
    }

    fn reset(&mut self) -> bool {
        self.hover = None;
        self.reference = None;
        self.targets.clear();
        self.base.take().is_some()
    }

    fn preview(&self) -> Option<ToolPreview> {
        let (base, reference, hover) = (self.base?, self.reference?, self.hover?);
        let transform = scale_about(base, base.distance(hover) / base.distance(reference))?;
        Some(preview_of(&self.targets, transform, TransformKind::Scale))
    }

    fn set_param(&mut self, param: ModifyParam) -> bool {
        match param {
            ModifyParam::Factor(f) if f.is_finite() && f > 0.0 => {
                self.factor = Some(f);
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::Point;

    use super::super::fixture::*;
    use super::super::modify::kit::*;
    use super::super::modify::{ModifyParam, Rejection};
    use super::super::{EditCommit, ToolKind, ToolPreview, ToolStep, TransformKind};
    use crate::input::CanvasKey;

    fn scene() -> Fixture {
        let mut f = with_objects(vec![rect(1, 0.0, 0.0, 10.0, 10.0)]);
        f.select(&[1]);
        f
    }

    fn transform(c: EditCommit) -> (burnburn_geometry::Affine, TransformKind) {
        let EditCommit::Transform { targets, transform, kind } = c else { panic!("transform, got {c:?}") };
        assert_eq!(targets, ids(&[1]));
        (transform, kind)
    }

    #[test]
    fn move_translates_the_selection_from_base_to_target() {
        let (f, mut ed) = (scene(), editor(ToolKind::Move));
        let (t, kind) = transform(only(&clicks(&mut ed, &f, &[(2.0, 2.0), (12.0, 7.0)])));
        assert_eq!(kind, TransformKind::Move);
        assert!(near(t.apply(Point::new(0.0, 0.0)), Point::new(10.0, 5.0)));
    }

    #[test]
    fn move_previews_the_displacement_and_escape_commits_nothing() {
        let (f, mut ed) = (scene(), editor(ToolKind::Move));
        click(&mut ed, &f, 2.0, 2.0);
        hover_to(&mut ed, &f, 6.0, 5.0);
        let Some(ToolPreview::Transform { transform, kind, .. }) = ed.preview() else { panic!("preview") };
        assert_eq!(kind, TransformKind::Move);
        assert!(near(transform.apply(Point::new(0.0, 0.0)), Point::new(4.0, 3.0)));
        assert_eq!(press_key(&mut ed, &f, CanvasKey::Escape), ToolStep::Cancelled);
        assert!(ed.preview().is_none());
    }

    #[test]
    fn move_needs_a_selection_and_a_real_displacement() {
        let mut f = scene();
        let mut ed = editor(ToolKind::Move);
        let steps = clicks(&mut ed, &f, &[(2.0, 2.0), (2.0, 2.0)]);
        assert!(commits(&steps).is_empty());
        assert!(steps.contains(&ToolStep::Cancelled), "no displacement: nothing to commit");
        f.select(&[]);
        assert_eq!(rejection(&click(&mut ed, &f, 2.0, 2.0)), Some(Rejection::NeedsSelection));
    }

    #[test]
    fn scale_uses_the_ratio_of_the_reference_and_target_distances_about_the_base() {
        let (f, mut ed) = (scene(), editor(ToolKind::Scale));
        let (t, kind) = transform(only(&clicks(&mut ed, &f, &[(5.0, 5.0), (15.0, 5.0), (25.0, 5.0)])));
        assert_eq!(kind, TransformKind::Scale);
        assert!(near(t.apply(Point::new(5.0, 5.0)), Point::new(5.0, 5.0)), "the base stays");
        assert!(near(t.apply(Point::new(7.0, 6.0)), Point::new(9.0, 7.0)), "factor 2");
    }

    #[test]
    fn a_typed_factor_scales_at_the_base_click_and_bad_input_is_refused() {
        let (f, mut ed) = (scene(), editor(ToolKind::Scale));
        param(&mut ed, ModifyParam::Factor(3.0));
        let (t, _) = transform(only(&click(&mut ed, &f, 0.0, 0.0)));
        assert!(near(t.apply(Point::new(1.0, 2.0)), Point::new(3.0, 6.0)));
        assert!(!ed.set_param_ok(ModifyParam::Factor(0.0)));

        let mut ed = editor(ToolKind::Scale);
        let steps = clicks(&mut ed, &f, &[(5.0, 5.0), (5.0, 5.0)]);
        assert_eq!(rejection(&steps), Some(Rejection::InvalidValue), "the reference point equals the base");
        assert!(commits(&steps).is_empty());
    }
}
