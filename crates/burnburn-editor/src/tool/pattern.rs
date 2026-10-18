//! The pattern tools: rectangular and circular arrays of the selection, as array modifiers.

use burnburn_document::structure::Modifier;
use burnburn_geometry::Point;
use burnburn_geometry::array::{CircularArray, GridArray};
use burnburn_types::{GeometryId, Length};

use super::modify::{Action, ModifyParam, Rejection, line_preview, rejected};
use super::{DRAG_THRESHOLD_PT, EditCommit, ToolContext, ToolPreview, ToolStep};
use crate::input::PointerEvent;

/// Pitches below this are none (float noise, not a user tolerance).
const MIN_PITCH_MM: f64 = 1e-9;

/// Base point, then a drag or a second click that sets the pitch of one column and one row step.
#[derive(Debug, Clone)]
pub(super) struct GridAction {
    columns: u32,
    rows: u32,
    base: Option<Point>,
    pressed: bool,
    hover: Option<Point>,
    targets: Vec<GeometryId>,
}

impl Default for GridAction {
    fn default() -> Self {
        GridAction { columns: 3, rows: 3, base: None, pressed: false, hover: None, targets: Vec::new() }
    }
}

impl GridAction {
    fn finish(&mut self, at: Point) -> ToolStep {
        let (base, targets) = (self.base, std::mem::take(&mut self.targets));
        self.reset();
        let Some(base) = base else { return ToolStep::Nothing };
        let (dx, dy) = (at.x - base.x, at.y - base.y);
        if dx.abs() <= MIN_PITCH_MM && dy.abs() <= MIN_PITCH_MM {
            return rejected(Rejection::InvalidValue);
        }
        let (Ok(pitch_x), Ok(pitch_y)) = (Length::from_mm(dx), Length::from_mm(dy)) else {
            return rejected(Rejection::InvalidValue);
        };
        let grid = GridArray { columns: self.columns, rows: self.rows, pitch_x, pitch_y };
        ToolStep::Commit(EditCommit::AddArray { targets, modifier: Modifier::GridArray(grid) })
    }
}

impl Action for GridAction {
    fn press(&mut self, ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) -> ToolStep {
        if ctx.selection.is_empty() {
            self.reset();
            return rejected(Rejection::NeedsSelection);
        }
        if self.base.is_some() {
            return self.finish(at);
        }
        self.base = Some(at);
        self.pressed = true;
        self.targets = ctx.selection.ids().to_vec();
        ToolStep::PreviewChanged
    }

    fn hover(&mut self, _ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) {
        self.hover = Some(at);
    }

    fn release(&mut self, ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) -> ToolStep {
        let Some(base) = self.base.filter(|_| self.pressed) else { return ToolStep::Nothing };
        self.pressed = false;
        if base.distance(at) > ctx.tolerance_mm(DRAG_THRESHOLD_PT) { self.finish(at) } else { ToolStep::Nothing }
    }

    fn reset(&mut self) -> bool {
        self.hover = None;
        self.pressed = false;
        self.targets.clear();
        self.base.take().is_some()
    }

    fn preview(&self) -> Option<ToolPreview> {
        line_preview(self.base?, self.hover?)
    }

    fn set_param(&mut self, param: ModifyParam) -> bool {
        match param {
            ModifyParam::Columns(n) if n >= 1 => self.columns = n,
            ModifyParam::Rows(n) if n >= 1 => self.rows = n,
            _ => return false,
        }
        true
    }
}

/// One click on the centre of rotation.
#[derive(Debug, Clone)]
pub(super) struct RadialAction {
    count: u32,
    sweep_deg: f64,
}

impl Default for RadialAction {
    fn default() -> Self {
        RadialAction { count: 6, sweep_deg: 360.0 }
    }
}

impl Action for RadialAction {
    fn press(&mut self, ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) -> ToolStep {
        let Some(bounds) = ctx.selection.bounds(ctx.document).filter(|_| !ctx.selection.is_empty()) else {
            return rejected(Rejection::NeedsSelection);
        };
        let array = CircularArray {
            count: self.count,
            center: at,
            sweep_deg: self.sweep_deg,
            rotate_items: true,
            reference: bounds.center(),
        };
        ToolStep::Commit(EditCommit::AddArray {
            targets: ctx.selection.ids().to_vec(),
            modifier: Modifier::CircularArray(array),
        })
    }

    fn reset(&mut self) -> bool {
        false
    }

    fn set_param(&mut self, param: ModifyParam) -> bool {
        match param {
            ModifyParam::Count(n) if n >= 2 => self.count = n,
            ModifyParam::SweepDeg(d) if d.is_finite() && d != 0.0 && d.abs() <= 360.0 => self.sweep_deg = d,
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use burnburn_document::structure::Modifier;
    use burnburn_geometry::Point;
    use burnburn_geometry::array::{CircularArray, GridArray};
    use burnburn_types::Length;

    use super::super::fixture::*;
    use super::super::modify::kit::*;
    use super::super::modify::{ModifyParam, Rejection};
    use super::super::{EditCommit, ToolKind, ToolStep};
    use crate::input::{CanvasInput, PointerPhase};

    fn mm(v: f64) -> Length {
        Length::from_mm(v).unwrap()
    }

    fn scene() -> Fixture {
        let mut f = with_objects(vec![rect(1, 0.0, 0.0, 10.0, 10.0)]);
        f.select(&[1]);
        f
    }

    fn drag(ed: &mut super::super::Editor, f: &Fixture, a: (f64, f64), b: (f64, f64)) -> Vec<ToolStep> {
        let mut steps = vec![ed.input(&at(PointerPhase::Down, a.0, a.1, none()), &f.ctx())];
        steps.push(ed.input(&at(PointerPhase::Move, b.0, b.1, none()), &f.ctx()));
        steps.push(ed.input(&at(PointerPhase::Up, b.0, b.1, none()), &f.ctx()));
        let _: Option<&CanvasInput> = None;
        steps
    }

    #[test]
    fn dragging_the_pitch_makes_a_grid_array_with_the_chosen_counts() {
        let (f, mut ed) = (scene(), editor(ToolKind::PatternRectangular));
        param(&mut ed, ModifyParam::Columns(3));
        param(&mut ed, ModifyParam::Rows(2));
        let steps = drag(&mut ed, &f, (0.0, 0.0), (15.0, 12.0));
        assert_eq!(
            only(&steps),
            EditCommit::AddArray {
                targets: ids(&[1]),
                modifier: Modifier::GridArray(GridArray { columns: 3, rows: 2, pitch_x: mm(15.0), pitch_y: mm(12.0) }),
            }
        );
    }

    #[test]
    fn two_clicks_define_the_pitch_too_and_a_backwards_drag_gives_negative_pitch() {
        let (f, mut ed) = (scene(), editor(ToolKind::PatternRectangular));
        let steps = clicks(&mut ed, &f, &[(20.0, 20.0), (14.0, 11.0)]);
        let EditCommit::AddArray { modifier: Modifier::GridArray(g), .. } = only(&steps) else { panic!("grid") };
        assert_eq!((g.pitch_x, g.pitch_y), (mm(-6.0), mm(-9.0)));
        assert_eq!((g.columns, g.rows), (3, 3), "defaults");
    }

    #[test]
    fn a_circular_pattern_is_one_click_on_the_centre() {
        let (f, mut ed) = (scene(), editor(ToolKind::PatternCircular));
        param(&mut ed, ModifyParam::Count(4));
        let steps = click(&mut ed, &f, 50.0, 40.0);
        assert_eq!(
            only(&steps),
            EditCommit::AddArray {
                targets: ids(&[1]),
                modifier: Modifier::CircularArray(CircularArray {
                    count: 4,
                    center: Point::new(50.0, 40.0),
                    sweep_deg: 360.0,
                    rotate_items: true,
                    reference: Point::new(5.0, 5.0),
                }),
            }
        );
        param(&mut ed, ModifyParam::SweepDeg(90.0));
        let EditCommit::AddArray { modifier: Modifier::CircularArray(c), .. } = only(&click(&mut ed, &f, 1.0, 1.0))
        else {
            panic!("circular")
        };
        assert_eq!(c.sweep_deg, 90.0);
    }

    #[test]
    fn counts_out_of_range_and_a_missing_selection_are_refused() {
        let mut ed = editor(ToolKind::PatternCircular);
        assert!(!ed.set_param_ok(ModifyParam::Count(1)));
        assert!(!ed.set_param_ok(ModifyParam::SweepDeg(0.0)));
        assert!(!ed.set_param_ok(ModifyParam::SweepDeg(400.0)));
        let mut ed = editor(ToolKind::PatternRectangular);
        assert!(!ed.set_param_ok(ModifyParam::Columns(0)));
        assert!(!ed.set_param_ok(ModifyParam::Count(3)), "circular setting");
        let mut f = scene();
        f.select(&[]);
        assert_eq!(rejection(&click(&mut ed, &f, 0.0, 0.0)), Some(Rejection::NeedsSelection));
        let mut ed = editor(ToolKind::PatternCircular);
        assert_eq!(rejection(&click(&mut ed, &f, 0.0, 0.0)), Some(Rejection::NeedsSelection));
    }

    #[test]
    fn a_zero_pitch_is_refused() {
        let (f, mut ed) = (scene(), editor(ToolKind::PatternRectangular));
        let steps = clicks(&mut ed, &f, &[(5.0, 5.0), (5.0, 5.0)]);
        assert!(commits(&steps).is_empty());
        assert_eq!(rejection(&steps), Some(Rejection::InvalidValue));
    }
}
