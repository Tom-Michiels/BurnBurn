//! Drawing into a sketch: which entities a finished gesture adds and which relations it suggests
//! (horizontal, vertical, coincident). A relation becomes a constraint only when the gesture ends
//! without the suppress modifier (S01); an imported or hand-converted contour never gets any.

use burnburn_document::sketch::{Entity, EntityId, Relation, Sketch, SketchConstraint, SketchEntity, SketchStep};
use burnburn_geometry::{Point, Segment, Shape};

use super::geometry::SketchGeometry;
use crate::tool::{EditCommit, ShapeKind, ToolPreview};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    Horizontal,
    Vertical,
    /// The new end point sits on an existing sketch point and shares it.
    Coincident,
}

/// A suggested relation: shown as a dashed symbol at `at` while drawing and applied on release.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Suggestion {
    pub at: Point,
    pub kind: SuggestionKind,
}

/// What a finished gesture adds to the sketch: one `SketchCommand::Batch` worth of steps.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapePlan {
    pub steps: Vec<SketchStep>,
    pub suggestions: Vec<Suggestion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    /// Only lines, polylines, rectangles and circles can be drawn into a sketch for now.
    Unsupported,
}

/// The steps and suggestions for `commit` in `sketch`. `tol_mm` is how close counts as horizontal,
/// vertical or on an existing point; `suppress` drops every suggestion (and so every relation).
pub fn plan_commit(sketch: &Sketch, commit: &EditCommit, tol_mm: f64, suppress: bool) -> Result<ShapePlan, PlanError> {
    let mut plan = Planner {
        geometry: SketchGeometry::new(sketch),
        tol: tol_mm,
        suppress,
        next: sketch.next_entity_id().raw(),
        points: Vec::new(),
        lines: Vec::new(),
        circle: None,
        axis_aligned: false,
        suggestions: Vec::new(),
    };
    match commit {
        EditCommit::CreateRectangle { rect } => {
            let (o, w, h) = (rect.origin(), rect.width().mm(), rect.height().mm());
            let corners = [o, Point::new(o.x + w, o.y), Point::new(o.x + w, o.y + h), Point::new(o.x, o.y + h)];
            plan.polygon(&corners, true, true);
        }
        EditCommit::CreateShape { shape } => match shape {
            Shape::Line { from, to } => plan.polygon(&[*from, *to], false, false),
            Shape::Polyline { points, closed } => plan.polygon(points, *closed, false),
            Shape::Circle { center, radius } => plan.circle(*center, radius.mm()),
            _ => return Err(PlanError::Unsupported),
        },
        _ => return Err(PlanError::Unsupported),
    }
    Ok(plan.finish())
}

struct Planner<'a> {
    geometry: SketchGeometry<'a>,
    tol: f64,
    suppress: bool,
    next: u32,
    /// New point entities in the order they get their ids.
    points: Vec<Point>,
    /// Lines between point ids.
    lines: Vec<(EntityId, EntityId)>,
    circle: Option<(EntityId, f64)>,
    axis_aligned: bool,
    suggestions: Vec<Suggestion>,
}

impl Planner<'_> {
    fn new_point(&mut self, at: Point) -> EntityId {
        let id = EntityId::from_raw(self.next + u32::try_from(self.points.len()).unwrap_or(u32::MAX));
        self.points.push(at);
        id
    }

    /// The existing sketch point under `at` (shared instead of duplicated), unless suppressed.
    fn existing(&mut self, at: Point) -> Option<EntityId> {
        if self.suppress {
            return None;
        }
        let sketch = self.geometry.sketch();
        let hit = sketch
            .entities()
            .filter_map(|(id, _)| self.geometry.point(id).map(|p| (id, p)))
            .filter(|(_, p)| p.distance(at) <= self.tol)
            .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))?;
        self.suggestions.push(Suggestion { at: hit.1, kind: SuggestionKind::Coincident });
        Some(hit.0)
    }

    fn point(&mut self, at: Point) -> EntityId {
        match self.existing(at) {
            Some(id) => id,
            None => self.new_point(at),
        }
    }

    /// Points joined by lines; `closed` adds the side back to the first point. `axis_aligned` is a
    /// rectangle: every side is exactly horizontal or vertical, whatever the tolerance.
    fn polygon(&mut self, drawn: &[Point], closed: bool, axis_aligned: bool) {
        let mut drawn = drawn.to_vec();
        // Clicking on the first point of an open polyline closes it there.
        let mut closed = closed;
        if !self.suppress
            && !closed
            && drawn.len() >= 3
            && drawn.first().zip(drawn.last()).is_some_and(|(a, b)| a.distance(*b) <= self.tol)
        {
            drawn.pop();
            closed = true;
        }
        let ids: Vec<EntityId> = drawn.iter().map(|p| self.point(*p)).collect();
        for pair in ids.windows(2) {
            self.lines.push((pair[0], pair[1]));
        }
        if closed && ids.len() >= 3 {
            self.lines.push((ids[ids.len() - 1], ids[0]));
        }
        self.axis_aligned = axis_aligned;
    }

    fn circle(&mut self, center: Point, radius: f64) {
        let c = self.point(center);
        self.circle = Some((c, radius));
    }

    fn position(&self, id: EntityId) -> Option<Point> {
        match id.raw().checked_sub(self.next) {
            Some(i) => self.points.get(usize::try_from(i).ok()?).copied(),
            None => self.geometry.point(id),
        }
    }

    fn finish(mut self) -> ShapePlan {
        let entity = |entity| SketchStep::AddEntity(SketchEntity { entity, construction: false });
        let mut steps: Vec<SketchStep> = self.points.iter().map(|at| entity(Entity::Point { at: *at })).collect();
        let first_line = self.next + u32::try_from(self.points.len()).unwrap_or(u32::MAX);
        steps.extend(self.lines.iter().map(|(start, end)| entity(Entity::Line { start: *start, end: *end })));
        if let Some((center, radius)) = self.circle {
            steps.push(entity(Entity::Circle { center, radius }));
        }
        if !self.suppress {
            for (k, (a, b)) in self.lines.clone().into_iter().enumerate() {
                let (Some(pa), Some(pb)) = (self.position(a), self.position(b)) else { continue };
                let line = EntityId::from_raw(first_line + u32::try_from(k).unwrap_or(u32::MAX));
                let (dx, dy) = ((pb.x - pa.x).abs(), (pb.y - pa.y).abs());
                let kind = if self.axis_aligned {
                    if k % 2 == 0 { SuggestionKind::Horizontal } else { SuggestionKind::Vertical }
                } else if dy <= self.tol && dx > self.tol {
                    SuggestionKind::Horizontal
                } else if dx <= self.tol && dy > self.tol {
                    SuggestionKind::Vertical
                } else {
                    continue;
                };
                let relation = if kind == SuggestionKind::Horizontal {
                    Relation::Horizontal(line)
                } else {
                    Relation::Vertical(line)
                };
                steps.push(SketchStep::AddConstraint(SketchConstraint::Relation(relation)));
                self.suggestions.push(Suggestion { at: pa.lerp(pb, 0.5), kind });
            }
        }
        ShapePlan { steps, suggestions: self.suggestions }
    }
}

/// The suggestions a gesture in progress would apply if released now (the dashed symbols).
pub fn suggest_preview(sketch: &Sketch, preview: &ToolPreview, tol_mm: f64, suppress: bool) -> Vec<Suggestion> {
    let commit = match preview {
        ToolPreview::Rectangle(p) => match crate::tool::rect_from_bounds(p.bounds()) {
            Some(rect) => EditCommit::CreateRectangle { rect },
            None => return Vec::new(),
        },
        ToolPreview::Shape { path, shape: ShapeKind::Line | ShapeKind::Polyline } => {
            let Some(sub) = path.subpaths().first() else { return Vec::new() };
            let mut points = vec![sub.start()];
            for segment in sub.segments() {
                match segment {
                    Segment::Line { to } => points.push(*to),
                    _ => return Vec::new(),
                }
            }
            EditCommit::CreateShape { shape: Shape::Polyline { points, closed: sub.is_closed() } }
        }
        _ => return Vec::new(),
    };
    plan_commit(sketch, &commit, tol_mm, suppress).map(|p| p.suggestions).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use burnburn_document::sketch::{Entity, EntityId, Relation, SketchEntity};
    use burnburn_geometry::{Path, Rect, Shape};
    use burnburn_types::{Angle, Length};

    use crate::tool::ShapeKind;

    fn id(raw: u32) -> EntityId {
        EntityId::from_raw(raw)
    }

    fn mm(v: f64) -> Length {
        Length::from_mm(v).unwrap()
    }

    fn line(from: (f64, f64), to: (f64, f64)) -> EditCommit {
        EditCommit::CreateShape { shape: Shape::Line { from: Point::new(from.0, from.1), to: Point::new(to.0, to.1) } }
    }

    fn point_step(x: f64, y: f64) -> SketchStep {
        SketchStep::AddEntity(SketchEntity { entity: Entity::Point { at: Point::new(x, y) }, construction: false })
    }

    fn line_step(a: u32, b: u32) -> SketchStep {
        SketchStep::AddEntity(SketchEntity { entity: Entity::Line { start: id(a), end: id(b) }, construction: false })
    }

    fn relation(r: Relation) -> SketchStep {
        SketchStep::AddConstraint(SketchConstraint::Relation(r))
    }

    /// p0 (0,0), p1 (10,0), line e2 between them: the next ids are 3, 4, ….
    fn existing() -> Sketch {
        let mut s = Sketch::new("t");
        let (a, b) = (s.point(0.0, 0.0).unwrap(), s.point(10.0, 0.0).unwrap());
        s.line(a, b).unwrap();
        s
    }

    #[test]
    fn a_nearly_horizontal_line_suggests_horizontal_and_applies_it() {
        let plan = plan_commit(&Sketch::new("t"), &line((0.0, 0.0), (30.0, 0.2)), 0.5, false).unwrap();
        assert_eq!(
            plan.steps,
            vec![point_step(0.0, 0.0), point_step(30.0, 0.2), line_step(0, 1), relation(Relation::Horizontal(id(2)))]
        );
        assert_eq!(plan.suggestions, vec![Suggestion { at: Point::new(15.0, 0.1), kind: SuggestionKind::Horizontal }]);
    }

    #[test]
    fn a_nearly_vertical_line_suggests_vertical() {
        let plan = plan_commit(&Sketch::new("t"), &line((5.0, 0.0), (5.3, 20.0)), 0.5, false).unwrap();
        assert!(plan.steps.contains(&relation(Relation::Vertical(id(2)))));
        assert_eq!(plan.suggestions.len(), 1);
        assert_eq!(plan.suggestions[0].kind, SuggestionKind::Vertical);
    }

    #[test]
    fn an_oblique_line_gets_no_relation() {
        let plan = plan_commit(&Sketch::new("t"), &line((0.0, 0.0), (30.0, 20.0)), 0.5, false).unwrap();
        assert_eq!(plan.steps, vec![point_step(0.0, 0.0), point_step(30.0, 20.0), line_step(0, 1)]);
        assert!(plan.suggestions.is_empty());
    }

    #[test]
    fn new_entities_continue_the_sketch_numbering() {
        let plan = plan_commit(&existing(), &line((0.0, 5.0), (30.0, 5.1)), 0.5, false).unwrap();
        assert_eq!(plan.steps[2], line_step(3, 4));
        assert_eq!(plan.steps[3], relation(Relation::Horizontal(id(5))));
    }

    #[test]
    fn a_line_starting_on_an_existing_point_shares_that_point() {
        let plan = plan_commit(&existing(), &line((0.2, 0.1), (20.0, 30.0)), 0.5, false).unwrap();
        assert_eq!(plan.steps, vec![point_step(20.0, 30.0), line_step(0, 3)], "only the end point is new");
        assert_eq!(plan.suggestions, vec![Suggestion { at: Point::new(0.0, 0.0), kind: SuggestionKind::Coincident }]);
    }

    #[test]
    fn suppressing_the_suggestions_creates_no_relation_and_no_shared_point() {
        let plan = plan_commit(&existing(), &line((0.2, 0.1), (30.0, 0.3)), 0.5, true).unwrap();
        assert_eq!(plan.steps, vec![point_step(0.2, 0.1), point_step(30.0, 0.3), line_step(3, 4)]);
        assert!(plan.suggestions.is_empty());
    }

    #[test]
    fn a_rectangle_gets_four_axis_relations_unless_suppressed() {
        let rect = Rect::new(Point::new(0.0, 0.0), mm(20.0), mm(10.0)).unwrap();
        let plan = plan_commit(&Sketch::new("t"), &EditCommit::CreateRectangle { rect }, 0.5, false).unwrap();
        let (adds, relations): (Vec<_>, Vec<_>) =
            plan.steps.iter().partition(|s| matches!(s, SketchStep::AddEntity(_)));
        assert_eq!(adds.len(), 8, "four corners and four sides");
        assert_eq!(
            relations,
            vec![
                &relation(Relation::Horizontal(id(4))),
                &relation(Relation::Vertical(id(5))),
                &relation(Relation::Horizontal(id(6))),
                &relation(Relation::Vertical(id(7))),
            ]
        );
        assert_eq!(plan.suggestions.len(), 4);
        let plain = plan_commit(&Sketch::new("t"), &EditCommit::CreateRectangle { rect }, 0.5, true).unwrap();
        assert_eq!(plain.steps.len(), 8, "the same eight entities, no relations");
        assert!(plain.suggestions.is_empty());
    }

    #[test]
    fn clicking_the_first_point_closes_a_polyline_on_it() {
        let shape = Shape::Polyline {
            points: vec![Point::new(0.0, 0.0), Point::new(10.0, 3.0), Point::new(10.0, 13.0), Point::new(0.1, 0.1)],
            closed: false,
        };
        let plan = plan_commit(&Sketch::new("t"), &EditCommit::CreateShape { shape }, 0.5, false).unwrap();
        let points = plan
            .steps
            .iter()
            .filter(|s| matches!(s, SketchStep::AddEntity(e) if matches!(e.entity, Entity::Point { .. })))
            .count();
        let lines = plan
            .steps
            .iter()
            .filter(|s| matches!(s, SketchStep::AddEntity(e) if matches!(e.entity, Entity::Line { .. })))
            .count();
        assert_eq!((points, lines), (3, 3), "the last click is the first point again");
        assert!(plan.steps.contains(&line_step(2, 0)), "the closing side ends on the first point");
    }

    #[test]
    fn a_circle_adds_its_centre_and_the_circle() {
        let shape = Shape::Circle { center: Point::new(5.0, 5.0), radius: mm(3.0) };
        let plan = plan_commit(&Sketch::new("t"), &EditCommit::CreateShape { shape }, 0.5, false).unwrap();
        assert_eq!(
            plan.steps,
            vec![
                point_step(5.0, 5.0),
                SketchStep::AddEntity(SketchEntity {
                    entity: Entity::Circle { center: id(0), radius: 3.0 },
                    construction: false
                }),
            ]
        );
    }

    #[test]
    fn an_ellipse_cannot_be_drawn_into_a_sketch() {
        let shape = Shape::Ellipse { center: Point::new(0.0, 0.0), rx: mm(5.0), ry: mm(3.0), rotation: Angle::ZERO };
        assert_eq!(
            plan_commit(&Sketch::new("t"), &EditCommit::CreateShape { shape }, 0.5, false),
            Err(PlanError::Unsupported)
        );
    }

    #[test]
    fn the_preview_shows_what_release_would_apply() {
        let path = std::sync::Arc::new(Path::line(Point::new(0.0, 0.0), Point::new(30.0, 0.2)).unwrap());
        let preview = ToolPreview::Shape { path, shape: ShapeKind::Line };
        let shown = suggest_preview(&Sketch::new("t"), &preview, 0.5, false);
        assert_eq!(shown, vec![Suggestion { at: Point::new(15.0, 0.1), kind: SuggestionKind::Horizontal }]);
        assert!(suggest_preview(&Sketch::new("t"), &preview, 0.5, true).is_empty(), "suppressed: nothing shown");
    }
}
