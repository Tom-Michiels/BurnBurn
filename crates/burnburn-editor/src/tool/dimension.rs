//! The dimension tool: in a sketch a driving dimension through the sketch commands, outside a
//! sketch a measurement. A line gives its length, a circle its diameter, an arc its radius and
//! two points their distance.

use burnburn_document::sketch::{
    Along, Dimension, DimensionKind, DimensionRole, Entity, SketchCommand, SketchConstraint, SketchId,
};
use burnburn_geometry::Point;

use super::modify::{Action, Measurement, Rejection, line_preview, rejected};
use super::sketch_pick::{PickKind, SketchPick, pick_entity, point_of, radius_of};
use super::{CLICK_TOLERANCE_PT, EditCommit, ToolContext, ToolPreview, ToolStep};
use crate::input::PointerEvent;

#[derive(Debug, Clone, Copy, PartialEq)]
enum First {
    /// A point entity waiting for the second point.
    Entity(SketchPick),
    /// A free position waiting for the second position (a measurement).
    Free(Point),
}

#[derive(Debug, Clone, Default)]
pub(super) struct DimensionAction {
    first: Option<First>,
    hover: Option<Point>,
}

/// `12.5 mm`: the shortest text of the value at 1e-4 mm, with its unit (a bare number is
/// dimensionless in the sketch expressions).
pub(super) fn length_text(mm: f64) -> String {
    let fixed = format!("{mm:.4}");
    let trimmed = fixed.trim_end_matches('0').trim_end_matches('.');
    format!("{} mm", if trimmed.is_empty() || trimmed == "-0" { "0" } else { trimmed })
}

fn commit(sketch: SketchId, kind: DimensionKind, mm: f64) -> ToolStep {
    ToolStep::Commit(EditCommit::Sketch {
        sketch,
        commands: vec![SketchCommand::AddConstraint {
            sketch,
            constraint: SketchConstraint::Dimension(Dimension {
                kind,
                text: length_text(mm),
                role: DimensionRole::Driving,
            }),
        }],
    })
}

impl DimensionAction {
    /// The dimension a single click on a curve completes, or `None` for a point.
    fn single(ctx: &ToolContext<'_>, pick: SketchPick) -> Option<ToolStep> {
        let sketch = ctx.document.sketch(pick.sketch)?;
        let entity = sketch.entity(pick.entity)?.entity;
        let invalid = || Some(rejected(Rejection::InvalidSelection));
        match (pick.kind, entity) {
            (PickKind::Line, Entity::Line { start, end }) => {
                let Some((a, b)) = point_of(sketch, start).zip(point_of(sketch, end)) else { return invalid() };
                Some(commit(pick.sketch, DimensionKind::Length { line: pick.entity }, a.distance(b)))
            }
            (PickKind::Circle, _) => {
                let Some(r) = radius_of(sketch, pick.entity) else { return invalid() };
                Some(commit(pick.sketch, DimensionKind::Diameter { curve: pick.entity }, 2.0 * r))
            }
            (PickKind::Arc, Entity::Arc { center, start, .. }) => {
                let Some((c, s)) = point_of(sketch, center).zip(point_of(sketch, start)) else { return invalid() };
                Some(commit(pick.sketch, DimensionKind::Radius { curve: pick.entity }, c.distance(s)))
            }
            (PickKind::Ellipse, Entity::Ellipse { center, major, .. }) => {
                let Some((c, m)) = point_of(sketch, center).zip(point_of(sketch, major)) else { return invalid() };
                Some(commit(pick.sketch, DimensionKind::Diameter { curve: pick.entity }, 2.0 * c.distance(m)))
            }
            _ => None,
        }
    }
}

impl Action for DimensionAction {
    fn press(&mut self, ctx: &ToolContext<'_>, at: Point, ev: &PointerEvent) -> ToolStep {
        let pick = pick_entity(ctx.document, ev.position, ctx.tolerance_mm(CLICK_TOLERANCE_PT));
        match self.first.take() {
            None => {
                self.hover = None;
                match pick {
                    Some(p) => match Self::single(ctx, p) {
                        Some(step) => step,
                        None => {
                            self.first = Some(First::Entity(p));
                            ToolStep::PreviewChanged
                        }
                    },
                    None => {
                        self.first = Some(First::Free(at));
                        ToolStep::PreviewChanged
                    }
                }
            }
            Some(First::Free(from)) => ToolStep::Measured(Measurement { from, to: at }),
            Some(First::Entity(a)) => {
                let both = pick.filter(|b| b.kind == PickKind::Point && b.sketch == a.sketch && b.entity != a.entity);
                let Some(b) = both else { return rejected(Rejection::InvalidSelection) };
                let positions =
                    ctx.document.sketch(a.sketch).and_then(|s| point_of(s, a.entity).zip(point_of(s, b.entity)));
                let Some((pa, pb)) = positions else { return rejected(Rejection::InvalidSelection) };
                commit(
                    a.sketch,
                    DimensionKind::Distance { a: a.entity, b: b.entity, along: Along::Direct },
                    pa.distance(pb),
                )
            }
        }
    }

    fn hover(&mut self, _ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) {
        self.hover = Some(at);
    }

    fn reset(&mut self) -> bool {
        self.hover = None;
        self.first.take().is_some()
    }

    fn preview(&self) -> Option<ToolPreview> {
        match (self.first?, self.hover) {
            (First::Free(a), Some(h)) => line_preview(a, h),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use burnburn_document::sketch::{
        Along, Dimension, DimensionKind, DimensionRole, EntityId, Sketch, SketchCommand, SketchConstraint,
    };
    use burnburn_geometry::Point;

    use super::super::fixture::*;
    use super::super::modify::kit::*;
    use super::super::modify::{Measurement, Rejection};
    use super::super::{EditCommit, ToolKind, ToolStep};
    use crate::input::CanvasKey;

    /// a(0,0) b(10,0) line(2); circle at (30,0) r 5.
    fn sketch() -> Sketch {
        let mut s = Sketch::new("S");
        let a = s.point(0.0, 0.0).unwrap();
        let b = s.point(10.0, 0.0).unwrap();
        s.line(a, b).unwrap();
        let c = s.point(30.0, 0.0).unwrap();
        s.circle(c, 5.0).unwrap();
        s
    }

    fn dimension(kind: DimensionKind, text: &str) -> EditCommit {
        EditCommit::Sketch {
            sketch: sketch_id(),
            commands: vec![SketchCommand::AddConstraint {
                sketch: sketch_id(),
                constraint: SketchConstraint::Dimension(Dimension {
                    kind,
                    text: text.into(),
                    role: DimensionRole::Driving,
                }),
            }],
        }
    }

    #[test]
    fn clicking_a_line_adds_its_length_as_a_driving_dimension() {
        let f = with_sketch(vec![], sketch());
        let mut ed = editor(ToolKind::Dimension);
        let steps = click(&mut ed, &f, 5.0, 0.3);
        assert_eq!(only(&steps), dimension(DimensionKind::Length { line: EntityId::from_raw(2) }, "10 mm"));
    }

    #[test]
    fn clicking_a_circle_adds_its_diameter() {
        let f = with_sketch(vec![], sketch());
        let mut ed = editor(ToolKind::Dimension);
        let steps = click(&mut ed, &f, 35.0, 0.2);
        assert_eq!(only(&steps), dimension(DimensionKind::Diameter { curve: EntityId::from_raw(4) }, "10 mm"));
    }

    #[test]
    fn clicking_an_ellipse_dimensions_its_major_axis_as_a_diameter() {
        let mut s = sketch();
        let (c, m) = (s.point(0.0, 40.0).unwrap(), s.point(8.0, 40.0).unwrap());
        let ellipse = s.ellipse(c, m, 3.0).unwrap();
        let f = with_sketch(vec![], s);
        let mut ed = editor(ToolKind::Dimension);
        let steps = click(&mut ed, &f, 0.1, 43.0);
        assert_eq!(only(&steps), dimension(DimensionKind::Diameter { curve: ellipse }, "16 mm"));
    }

    #[test]
    fn two_points_give_the_distance_between_them() {
        let f = with_sketch(vec![], sketch());
        let mut ed = editor(ToolKind::Dimension);
        let steps = clicks(&mut ed, &f, &[(0.1, 0.0), (9.9, 0.0)]);
        assert_eq!(
            only(&steps),
            dimension(
                DimensionKind::Distance { a: EntityId::from_raw(0), b: EntityId::from_raw(1), along: Along::Direct },
                "10 mm"
            )
        );
    }

    #[test]
    fn a_point_then_a_line_is_not_a_dimension_and_changes_nothing() {
        let f = with_sketch(vec![], sketch());
        let mut ed = editor(ToolKind::Dimension);
        let steps = clicks(&mut ed, &f, &[(0.1, 0.0), (5.0, 0.3)]);
        assert!(commits(&steps).is_empty());
        assert_eq!(rejection(&steps), Some(Rejection::InvalidSelection));
    }

    #[test]
    fn outside_a_sketch_two_points_are_a_measurement_not_a_commit() {
        let (f, mut ed) = (with_objects(vec![rect(1, 0.0, 0.0, 10.0, 10.0)]), editor(ToolKind::Dimension));
        let steps = clicks(&mut ed, &f, &[(0.0, 0.0), (3.0, 4.0)]);
        assert!(commits(&steps).is_empty());
        let m = steps.iter().find_map(|s| if let ToolStep::Measured(m) = s { Some(*m) } else { None }).unwrap();
        assert_eq!(m, Measurement { from: Point::new(0.0, 0.0), to: Point::new(3.0, 4.0) });
        assert!((m.distance() - 5.0).abs() < 1e-12 && (m.dx() - 3.0).abs() < 1e-12 && (m.dy() - 4.0).abs() < 1e-12);
    }

    #[test]
    fn escape_forgets_the_first_pick() {
        let f = empty();
        let mut ed = editor(ToolKind::Dimension);
        click(&mut ed, &f, 1.0, 1.0);
        assert_eq!(press_key(&mut ed, &f, CanvasKey::Escape), ToolStep::Cancelled);
        let steps = click(&mut ed, &f, 5.0, 5.0);
        assert!(!steps.iter().any(|s| matches!(s, ToolStep::Measured(_))), "the earlier pick is gone");
    }
}
