//! The constraint tools: pick the entities a relation needs and send exactly the matching sketch
//! commands; auto-constrain adds the obvious horizontal, vertical and coincident relations.

use burnburn_document::sketch::{Entity, EntityId, Relation, Sketch, SketchCommand, SketchConstraint, SketchId};
use burnburn_geometry::Point;

use super::modify::{Action, Rejection, rejected};
use super::sketch_pick::{PickKind, SketchPick, pick_entity, point_of};
use super::{CLICK_TOLERANCE_PT, EditCommit, ToolContext, ToolKind, ToolStep};
use crate::input::PointerEvent;

/// A line within this angle (degrees) of an axis counts as horizontal or vertical for
/// auto-constrain; two distinct points within [`COINCIDENT_MM`] count as one.
const AXIS_ANGLE_DEG: f64 = 1.0;
const COINCIDENT_MM: f64 = 0.05;

fn commit(sketch: SketchId, relations: Vec<Relation>) -> ToolStep {
    let commands = relations
        .into_iter()
        .map(|r| SketchCommand::AddConstraint { sketch, constraint: SketchConstraint::Relation(r) })
        .collect();
    ToolStep::Commit(EditCommit::Sketch { sketch, commands })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Outcome {
    NeedMore,
    Invalid,
    Done,
}

#[derive(Debug, Clone)]
pub(super) struct ConstraintAction {
    kind: ToolKind,
    picks: Vec<SketchPick>,
}

impl ConstraintAction {
    pub(super) fn new(kind: ToolKind) -> Self {
        ConstraintAction { kind, picks: Vec::new() }
    }

    fn needed(&self) -> usize {
        match self.kind {
            ToolKind::ConstraintFix | ToolKind::ConstraintHorizontalVertical => 1,
            ToolKind::ConstraintSymmetry => 3,
            _ => 2,
        }
    }

    /// The relations for the complete pick list, or why it does not fit.
    fn relations(&self, sketch: &Sketch) -> Option<Vec<Relation>> {
        use PickKind::{Line, Point as Pt};
        let kinds: Vec<PickKind> = self.picks.iter().map(|p| p.kind).collect();
        let e: Vec<EntityId> = self.picks.iter().map(|p| p.entity).collect();
        let is_curve = |k: PickKind| k.is_curve();
        Some(match (self.kind, kinds.as_slice()) {
            (ToolKind::ConstraintCoincident, [Pt, Pt]) => vec![Relation::Coincident(e[0], e[1])],
            (ToolKind::ConstraintCoincident, [Pt, k]) if *k == Line || is_curve(*k) => {
                vec![Relation::PointOn { point: e[0], curve: e[1] }]
            }
            (ToolKind::ConstraintCoincident, [k, Pt]) if *k == Line || is_curve(*k) => {
                vec![Relation::PointOn { point: e[1], curve: e[0] }]
            }
            (ToolKind::ConstraintCollinear, [Line, Line]) => {
                let Entity::Line { start, .. } = sketch.entity(e[1])?.entity else { return None };
                vec![Relation::Parallel(e[0], e[1]), Relation::PointOn { point: start, curve: e[0] }]
            }
            (ToolKind::ConstraintConcentric, [a, b]) if is_curve(*a) && is_curve(*b) => {
                vec![Relation::Concentric(e[0], e[1])]
            }
            (ToolKind::ConstraintEqual, [Line, Line]) => vec![Relation::Equal(e[0], e[1])],
            (ToolKind::ConstraintEqual, [a, b]) if is_curve(*a) && is_curve(*b) => vec![Relation::Equal(e[0], e[1])],
            (ToolKind::ConstraintFix, [Pt]) => vec![Relation::Fix { point: e[0], at: point_of(sketch, e[0])? }],
            (ToolKind::ConstraintHorizontalVertical, [Line]) => {
                let Entity::Line { start, end } = sketch.entity(e[0])?.entity else { return None };
                let (a, b) = (point_of(sketch, start)?, point_of(sketch, end)?);
                vec![if (b.x - a.x).abs() >= (b.y - a.y).abs() {
                    Relation::Horizontal(e[0])
                } else {
                    Relation::Vertical(e[0])
                }]
            }
            (ToolKind::ConstraintMidpoint, [Pt, Line]) => vec![Relation::Midpoint { point: e[0], line: e[1] }],
            (ToolKind::ConstraintMidpoint, [Line, Pt]) => vec![Relation::Midpoint { point: e[1], line: e[0] }],
            (ToolKind::ConstraintParallel, [Line, Line]) => vec![Relation::Parallel(e[0], e[1])],
            (ToolKind::ConstraintPerpendicular, [Line, Line]) => vec![Relation::Perpendicular(e[0], e[1])],
            (ToolKind::ConstraintSymmetry, [Pt, Pt, Line]) => {
                vec![Relation::Symmetric { a: e[0], b: e[1], about: e[2] }]
            }
            (ToolKind::ConstraintTangent, [Line, k]) | (ToolKind::ConstraintTangent, [k, Line]) if is_curve(*k) => {
                vec![Relation::Tangent(e[0], e[1])]
            }
            (ToolKind::ConstraintTangent, [a, b]) if is_curve(*a) && is_curve(*b) => {
                vec![Relation::Tangent(e[0], e[1])]
            }
            _ => return None,
        })
    }

    fn outcome(&self, ctx: &ToolContext<'_>) -> (Outcome, Option<(SketchId, Vec<Relation>)>) {
        if self.picks.len() < self.needed() {
            return (Outcome::NeedMore, None);
        }
        let Some(first) = self.picks.first() else { return (Outcome::Invalid, None) };
        let relations = ctx.document.sketch(first.sketch).and_then(|s| self.relations(s));
        match relations {
            Some(r) => (Outcome::Done, Some((first.sketch, r))),
            None => (Outcome::Invalid, None),
        }
    }
}

impl Action for ConstraintAction {
    fn press(&mut self, ctx: &ToolContext<'_>, _at: Point, ev: &PointerEvent) -> ToolStep {
        let Some(pick) = pick_entity(ctx.document, ev.position, ctx.tolerance_mm(CLICK_TOLERANCE_PT)) else {
            return rejected(Rejection::NothingPicked);
        };
        let clash = self.picks.iter().any(|p| p.entity == pick.entity && p.sketch == pick.sketch)
            || self.picks.iter().any(|p| p.sketch != pick.sketch);
        if clash {
            self.reset();
            return rejected(Rejection::InvalidSelection);
        }
        self.picks.push(pick);
        match self.outcome(ctx) {
            (Outcome::NeedMore, _) => ToolStep::PreviewChanged,
            (Outcome::Done, Some((sketch, relations))) => {
                self.reset();
                commit(sketch, relations)
            }
            _ => {
                self.reset();
                rejected(Rejection::InvalidSelection)
            }
        }
    }

    fn reset(&mut self) -> bool {
        !std::mem::take(&mut self.picks).is_empty()
    }
}

/// One click on any entity of a sketch adds the obvious relations of that whole sketch.
#[derive(Debug, Clone, Default)]
pub(super) struct AutoAction;

fn already(sketch: &Sketch, wanted: Relation) -> bool {
    sketch.constraints().any(|(_, c)| match (c, wanted) {
        (SketchConstraint::Relation(Relation::Coincident(a, b)), Relation::Coincident(x, y)) => {
            (a, b) == (&x, &y) || (a, b) == (&y, &x)
        }
        (SketchConstraint::Relation(r), w) => *r == w,
        _ => false,
    })
}

/// Horizontal/vertical for near-axis lines, coincident for distinct points that sit together,
/// skipping what the sketch already states.
fn obvious_relations(sketch: &Sketch) -> Vec<Relation> {
    let tolerance = AXIS_ANGLE_DEG.to_radians().sin();
    let mut out = Vec::new();
    for (id, e) in sketch.entities() {
        let Entity::Line { start, end } = e.entity else { continue };
        let Some((a, b)) = point_of(sketch, start).zip(point_of(sketch, end)) else { continue };
        let len = a.distance(b);
        if !(len > 0.0) || already(sketch, Relation::Horizontal(id)) || already(sketch, Relation::Vertical(id)) {
            continue;
        }
        if (b.y - a.y).abs() / len <= tolerance {
            out.push(Relation::Horizontal(id));
        } else if (b.x - a.x).abs() / len <= tolerance {
            out.push(Relation::Vertical(id));
        }
    }
    let points: Vec<(EntityId, Point)> = sketch
        .entities()
        .filter(|(_, e)| matches!(e.entity, Entity::Point { .. }))
        .filter_map(|(id, _)| point_of(sketch, id).map(|p| (id, p)))
        .collect();
    for (i, (a, pa)) in points.iter().enumerate() {
        for (b, pb) in &points[i + 1..] {
            let wanted = Relation::Coincident(*a, *b);
            if pa.distance(*pb) <= COINCIDENT_MM && !already(sketch, wanted) {
                out.push(wanted);
            }
        }
    }
    out
}

impl Action for AutoAction {
    fn press(&mut self, ctx: &ToolContext<'_>, _at: Point, ev: &PointerEvent) -> ToolStep {
        let Some(pick) = pick_entity(ctx.document, ev.position, ctx.tolerance_mm(CLICK_TOLERANCE_PT)) else {
            return rejected(Rejection::NothingPicked);
        };
        let Some(sketch) = ctx.document.sketch(pick.sketch) else { return rejected(Rejection::NothingPicked) };
        let relations = obvious_relations(sketch);
        if relations.is_empty() {
            return rejected(Rejection::NothingToConstrain);
        }
        commit(pick.sketch, relations)
    }

    fn reset(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use burnburn_document::sketch::{EntityId, Relation, Sketch, SketchCommand, SketchConstraint};
    use burnburn_geometry::Point;

    use super::super::modify::Rejection;
    use super::super::modify::kit::*;
    use super::super::{EditCommit, ToolKind, ToolStep};
    use crate::input::CanvasKey;

    // Entity ids by construction order.
    const A: u32 = 0; // (0,0)
    const B: u32 = 1; // (10,0)
    const L1: u32 = 2; // A-B
    const C: u32 = 3; // (0,10)
    #[allow(dead_code)] // documents the fixture
    const D: u32 = 4; // (10,10)
    const L2: u32 = 5; // C-D
    const E: u32 = 6; // (20,0)
    #[allow(dead_code)] // documents the fixture
    const F: u32 = 7; // (20,10)
    const L3: u32 = 8; // E-F
    const G: u32 = 9; // (50,50)
    const C1: u32 = 10; // circle r 5 at G
    #[allow(dead_code)] // documents the fixture
    const H: u32 = 11; // (70,50)
    const C2: u32 = 12; // circle r 5 at H

    fn sketch() -> Sketch {
        let mut s = Sketch::new("S");
        let id = EntityId::from_raw;
        let pts: Vec<_> = [(0.0, 0.0), (10.0, 0.0)].iter().map(|(x, y)| s.point(*x, *y).unwrap()).collect();
        s.line(pts[0], pts[1]).unwrap();
        let (c, d) = (s.point(0.0, 10.0).unwrap(), s.point(10.0, 10.0).unwrap());
        s.line(c, d).unwrap();
        let (e, f) = (s.point(20.0, 0.0).unwrap(), s.point(20.0, 10.0).unwrap());
        s.line(e, f).unwrap();
        let g = s.point(50.0, 50.0).unwrap();
        s.circle(g, 5.0).unwrap();
        let h = s.point(70.0, 50.0).unwrap();
        s.circle(h, 5.0).unwrap();
        assert_eq!((id(A), id(C1)), (pts[0], EntityId::from_raw(10)));
        s
    }

    /// A click position that picks each entity (indexed by entity id).
    const CLICKS: [(f64, f64); 13] = [
        (0.0, 0.0),
        (10.0, 0.0),
        (5.0, 0.2),
        (0.0, 10.0),
        (10.0, 10.0),
        (5.0, 10.2),
        (20.0, 0.0),
        (20.0, 10.0),
        (20.2, 5.0),
        (50.0, 50.0),
        (55.0, 50.2),
        (70.0, 50.0),
        (75.0, 50.2),
    ];

    fn at_entity(raw: u32) -> (f64, f64) {
        CLICKS[raw as usize]
    }

    fn id(raw: u32) -> EntityId {
        EntityId::from_raw(raw)
    }

    fn rel(r: Relation) -> SketchCommand {
        SketchCommand::AddConstraint { sketch: sketch_id(), constraint: SketchConstraint::Relation(r) }
    }

    fn pick(kind: ToolKind, entities: &[u32]) -> Vec<ToolStep> {
        let (f, mut ed) = (with_sketch(vec![], sketch()), editor(kind));
        let pts: Vec<(f64, f64)> = entities.iter().map(|e| at_entity(*e)).collect();
        clicks(&mut ed, &f, &pts)
    }

    fn sent(kind: ToolKind, entities: &[u32]) -> Vec<SketchCommand> {
        let EditCommit::Sketch { sketch, commands } = only(&pick(kind, entities)) else { panic!("sketch commit") };
        assert_eq!(sketch, sketch_id());
        commands
    }

    #[test]
    fn coincident_joins_two_points_or_puts_a_point_on_a_curve() {
        assert_eq!(sent(ToolKind::ConstraintCoincident, &[A, C]), vec![rel(Relation::Coincident(id(A), id(C)))]);
        assert_eq!(
            sent(ToolKind::ConstraintCoincident, &[E, L1]),
            vec![rel(Relation::PointOn { point: id(E), curve: id(L1) })]
        );
        assert_eq!(
            sent(ToolKind::ConstraintCoincident, &[C1, G]),
            vec![rel(Relation::PointOn { point: id(G), curve: id(C1) })],
            "order does not matter"
        );
    }

    #[test]
    fn collinear_is_parallel_plus_a_point_on_the_other_line() {
        assert_eq!(
            sent(ToolKind::ConstraintCollinear, &[L1, L2]),
            vec![rel(Relation::Parallel(id(L1), id(L2))), rel(Relation::PointOn { point: id(C), curve: id(L1) })]
        );
    }

    #[test]
    fn concentric_equal_parallel_perpendicular_and_tangent_take_two_entities() {
        assert_eq!(sent(ToolKind::ConstraintConcentric, &[C1, C2]), vec![rel(Relation::Concentric(id(C1), id(C2)))]);
        assert_eq!(sent(ToolKind::ConstraintEqual, &[L1, L2]), vec![rel(Relation::Equal(id(L1), id(L2)))]);
        assert_eq!(sent(ToolKind::ConstraintEqual, &[C1, C2]), vec![rel(Relation::Equal(id(C1), id(C2)))]);
        assert_eq!(sent(ToolKind::ConstraintParallel, &[L1, L2]), vec![rel(Relation::Parallel(id(L1), id(L2)))]);
        assert_eq!(
            sent(ToolKind::ConstraintPerpendicular, &[L1, L3]),
            vec![rel(Relation::Perpendicular(id(L1), id(L3)))]
        );
        assert_eq!(sent(ToolKind::ConstraintTangent, &[L1, C1]), vec![rel(Relation::Tangent(id(L1), id(C1)))]);
        assert_eq!(sent(ToolKind::ConstraintTangent, &[C1, C2]), vec![rel(Relation::Tangent(id(C1), id(C2)))]);
    }

    #[test]
    fn fix_pins_a_point_where_it_is_and_horizontal_vertical_follows_the_line() {
        assert_eq!(
            sent(ToolKind::ConstraintFix, &[B]),
            vec![rel(Relation::Fix { point: id(B), at: Point::new(10.0, 0.0) })]
        );
        assert_eq!(sent(ToolKind::ConstraintHorizontalVertical, &[L1]), vec![rel(Relation::Horizontal(id(L1)))]);
        assert_eq!(sent(ToolKind::ConstraintHorizontalVertical, &[L3]), vec![rel(Relation::Vertical(id(L3)))]);
    }

    #[test]
    fn midpoint_takes_a_point_and_a_line_in_either_order_and_symmetry_takes_two_points_and_an_axis() {
        let expected = vec![rel(Relation::Midpoint { point: id(C), line: id(L1) })];
        assert_eq!(sent(ToolKind::ConstraintMidpoint, &[C, L1]), expected);
        assert_eq!(sent(ToolKind::ConstraintMidpoint, &[L1, C]), expected);
        assert_eq!(
            sent(ToolKind::ConstraintSymmetry, &[A, B, L3]),
            vec![rel(Relation::Symmetric { a: id(A), b: id(B), about: id(L3) })]
        );
    }

    #[test]
    fn a_wrong_combination_is_refused_commits_nothing_and_starts_over() {
        let (f, mut ed) = (with_sketch(vec![], sketch()), editor(ToolKind::ConstraintParallel));
        let (l1, a) = (at_entity(L1), at_entity(A));
        let steps = clicks(&mut ed, &f, &[l1, a]);
        assert!(commits(&steps).is_empty());
        assert_eq!(rejection(&steps), Some(Rejection::InvalidSelection));
        let steps = clicks(&mut ed, &f, &[at_entity(L1), at_entity(L2)]);
        assert_eq!(
            only(&steps),
            EditCommit::Sketch { sketch: sketch_id(), commands: vec![rel(Relation::Parallel(id(L1), id(L2)))] }
        );
    }

    #[test]
    fn the_same_entity_twice_empty_space_and_escape_never_commit() {
        let (f, mut ed) = (with_sketch(vec![], sketch()), editor(ToolKind::ConstraintParallel));
        let steps = clicks(&mut ed, &f, &[at_entity(L1), at_entity(L1)]);
        assert!(commits(&steps).is_empty());
        assert_eq!(rejection(&steps), Some(Rejection::InvalidSelection));
        assert_eq!(rejection(&click(&mut ed, &f, 200.0, 200.0)), Some(Rejection::NothingPicked));
        click(&mut ed, &f, at_entity(L1).0, at_entity(L1).1);
        assert_eq!(press_key(&mut ed, &f, CanvasKey::Escape), ToolStep::Cancelled);
        let steps = click(&mut ed, &f, at_entity(L2).0, at_entity(L2).1);
        assert!(commits(&steps).is_empty(), "the first pick was forgotten");
    }

    #[test]
    fn auto_constrain_adds_horizontal_vertical_and_coincident_but_not_what_exists() {
        let mut s = Sketch::new("S");
        let p0 = s.point(0.0, 0.0).unwrap();
        let p1 = s.point(10.0, 0.05).unwrap();
        let p2 = s.point(10.0, 10.0).unwrap();
        let p3 = s.point(10.01, 10.0).unwrap();
        let p4 = s.point(0.0, 10.0).unwrap();
        let l1 = s.line(p0, p1).unwrap();
        let l2 = s.line(p1, p2).unwrap();
        let l3 = s.line(p3, p4).unwrap();
        s.relate(Relation::Horizontal(l3)).unwrap();
        let f = with_sketch(vec![], s);
        let mut ed = editor(ToolKind::AutoConstrain);
        let steps = click(&mut ed, &f, 5.0, 0.02);
        let EditCommit::Sketch { commands, .. } = only(&steps) else { panic!("sketch") };
        assert_eq!(
            commands,
            vec![rel(Relation::Horizontal(l1)), rel(Relation::Vertical(l2)), rel(Relation::Coincident(p2, p3))]
        );
    }

    #[test]
    fn auto_constrain_with_nothing_to_add_says_so() {
        let mut s = Sketch::new("S");
        let a = s.point(0.0, 0.0).unwrap();
        let b = s.point(10.0, 3.0).unwrap();
        s.line(a, b).unwrap();
        let f = with_sketch(vec![], s);
        let mut ed = editor(ToolKind::AutoConstrain);
        let steps = click(&mut ed, &f, 5.0, 1.5);
        assert!(commits(&steps).is_empty());
        assert_eq!(rejection(&steps), Some(Rejection::NothingToConstrain));
        assert_eq!(rejection(&click(&mut ed, &f, 100.0, 100.0)), Some(Rejection::NothingPicked));
    }
}
