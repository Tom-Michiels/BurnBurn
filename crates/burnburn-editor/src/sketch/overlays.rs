//! Sketch overlays: relation glyphs, dimensions, point freedom, selection and conflicts, as
//! `SceneSnapshot` overlays. Pure data in, overlays out; the renderer decides the look but must
//! not rely on colour alone (X03): a conflict is also marked with a cross.

use std::collections::{BTreeMap, BTreeSet};

use std::sync::Arc;

use burnburn_document::sketch::{
    Along, ConstraintId, DimensionKind, DimensionRole, Entity, EntityId, Relation, Sketch, SketchConstraint,
};
use burnburn_geometry::{EllipticalArc, Path, PathBuilder, Point, Vector};
use burnburn_types::Angle;

use super::geometry::SketchGeometry;
use crate::scene::Overlay;

/// The symbol of a relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SketchGlyph {
    Coincident,
    PointOn,
    Horizontal,
    Vertical,
    Parallel,
    Perpendicular,
    Tangent,
    Equal,
    Midpoint,
    Symmetric,
    Concentric,
    Fixed,
}

/// The glyph of `relation` (`None` for a relation kind this editor does not know yet).
pub fn glyph_of(relation: &Relation) -> Option<SketchGlyph> {
    Some(match relation {
        Relation::Coincident(..) => SketchGlyph::Coincident,
        Relation::PointOn { .. } => SketchGlyph::PointOn,
        Relation::Horizontal(_) => SketchGlyph::Horizontal,
        Relation::Vertical(_) => SketchGlyph::Vertical,
        Relation::Parallel(..) => SketchGlyph::Parallel,
        Relation::Perpendicular(..) => SketchGlyph::Perpendicular,
        Relation::Tangent(..) => SketchGlyph::Tangent,
        Relation::Equal(..) => SketchGlyph::Equal,
        Relation::Midpoint { .. } => SketchGlyph::Midpoint,
        Relation::Symmetric { .. } => SketchGlyph::Symmetric,
        Relation::Concentric(..) => SketchGlyph::Concentric,
        Relation::Fix { .. } => SketchGlyph::Fixed,
        _ => return None,
    })
}

/// How a sketch element is emphasised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SketchMark {
    Normal,
    Selected,
    /// Part of a conflict or an unmet constraint: drawn with a cross as well as a colour.
    Conflict,
}

/// How free a sketch point still is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointFreedom {
    /// At least one coordinate can still move.
    Free,
    Determined,
    /// Pinned by a `Fix` relation.
    Fixed,
}

/// What a window knows about the open sketch.
pub struct SketchView<'a> {
    pub sketch: &'a Sketch,
    pub selected_entities: &'a BTreeSet<EntityId>,
    pub selected_constraints: &'a BTreeSet<ConstraintId>,
    /// Constraints the solver names in a conflict or as unmet.
    pub conflicts: &'a BTreeSet<ConstraintId>,
    /// Points with a free coordinate; `None` when the sketch was not analysed (freedom not shown).
    pub free_points: Option<&'a BTreeSet<EntityId>>,
    /// The number each dimension shows (without brackets or prefix); missing = its own text.
    pub values: &'a BTreeMap<ConstraintId, String>,
    pub mm_per_point: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SketchOverlays {
    pub overlays: Vec<Overlay>,
    /// Where each relation's glyph is drawn (to pick it with the pointer).
    pub glyphs: Vec<(ConstraintId, Point)>,
    /// Where each dimension's label is drawn (to pick or edit it).
    pub labels: Vec<(ConstraintId, Point)>,
}

/// Screen distance of a glyph from the geometry it belongs to, in points.
const GLYPH_OFFSET_PT: f64 = 10.0;
/// Further glyphs on the same entity are stacked this many points apart.
const GLYPH_STACK_PT: f64 = 12.0;
/// Distance of a linear dimension line from the measured geometry, in points.
const DIMENSION_OFFSET_PT: f64 = 24.0;

pub fn sketch_overlays(view: &SketchView<'_>) -> SketchOverlays {
    let geometry = SketchGeometry::new(view.sketch);
    let mpp = view.mm_per_point;
    let mut out = SketchOverlays::default();

    let mut in_conflict: BTreeSet<EntityId> = BTreeSet::new();
    for id in view.conflicts {
        if let Some(c) = view.sketch.constraint(*id) {
            in_conflict.extend(c.entities());
        }
    }
    for (id, e) in view.sketch.entities() {
        let mark = if in_conflict.contains(&id) {
            SketchMark::Conflict
        } else if view.selected_entities.contains(&id) {
            SketchMark::Selected
        } else {
            SketchMark::Normal
        };
        if mark == SketchMark::Normal {
            continue;
        }
        let path = match e.entity {
            Entity::Line { .. } => geometry.line(id).and_then(|(a, b)| Path::line(a, b).ok()),
            Entity::Circle { .. } => geometry.circle(id).and_then(|(c, r)| circle_path(c, r)),
            _ => None,
        };
        if let Some(path) = path {
            out.overlays.push(Overlay::SketchEntity { path: Arc::new(path), mark });
        }
    }

    let mut stacked: BTreeMap<EntityId, u32> = BTreeMap::new();
    for (id, c) in view.sketch.constraints() {
        let mark = if view.conflicts.contains(&id) {
            SketchMark::Conflict
        } else if view.selected_constraints.contains(&id) {
            SketchMark::Selected
        } else {
            SketchMark::Normal
        };
        match c {
            SketchConstraint::Relation(r) => {
                let Some(glyph) = glyph_of(r) else { continue };
                let Some(&first) = c.entities().first() else { continue };
                let slot = stacked.entry(first).or_insert(0);
                let distance = (GLYPH_OFFSET_PT + GLYPH_STACK_PT * f64::from(*slot)) * mpp;
                *slot += 1;
                let Some(at) = glyph_anchor(&geometry, r, distance) else { continue };
                out.overlays.push(Overlay::SketchGlyph { at, glyph, mark });
                out.glyphs.push((id, at));
            }
            SketchConstraint::Dimension(d) => {
                let Some(shape) = dimension_shape(&geometry, d.kind, mpp) else { continue };
                let text = match view.values.get(&id) {
                    Some(v) => decorate(d.kind, v),
                    None => d.text.clone(),
                };
                let driven = d.role == DimensionRole::Driven;
                let text = if driven { format!("({text})") } else { text };
                out.labels.push((id, shape.label_at));
                out.overlays.push(Overlay::SketchDimension {
                    lines: shape.lines,
                    label_at: shape.label_at,
                    text,
                    driven,
                    mark,
                });
            }
        }
    }

    let pinned: BTreeSet<EntityId> = view
        .sketch
        .constraints()
        .filter_map(|(_, c)| match c {
            SketchConstraint::Relation(Relation::Fix { point, .. }) => Some(*point),
            _ => None,
        })
        .collect();
    for (id, _) in view.sketch.entities() {
        let Some(at) = geometry.point(id) else { continue };
        let freedom = if pinned.contains(&id) {
            PointFreedom::Fixed
        } else if view.free_points.is_some_and(|free| free.contains(&id)) {
            PointFreedom::Free
        } else {
            PointFreedom::Determined
        };
        out.overlays.push(Overlay::SketchPoint { at, freedom, selected: view.selected_entities.contains(&id) });
    }
    out
}

fn circle_path(center: Point, radius: f64) -> Option<Path> {
    let full = Angle::from_radians(std::f64::consts::TAU).ok()?;
    let arc = EllipticalArc::circle(center, radius, Angle::ZERO, full).ok()?;
    PathBuilder::new().move_to(arc.start_point()).arc(arc).close().build().ok()
}

/// Number text with the prefix or suffix of its kind: `R5`, `Ø10`, `30°`.
fn decorate(kind: DimensionKind, value: &str) -> String {
    match kind {
        DimensionKind::Radius { .. } => format!("R{value}"),
        DimensionKind::Diameter { .. } => format!("\u{00D8}{value}"),
        DimensionKind::Angle { .. } => format!("{value}\u{00B0}"),
        _ => value.to_owned(),
    }
}

fn unit(v: Vector) -> Option<Vector> {
    let l = v.length();
    (l > 1e-12).then(|| Vector::new(v.x / l, v.y / l))
}

/// A point on the entity to hang a glyph on, and the direction to move away from it.
fn entity_anchor(g: &SketchGeometry<'_>, id: EntityId) -> Option<(Point, Vector)> {
    if let Some(p) = g.point(id) {
        return Some((p, Vector::new(std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2)));
    }
    if let Some((a, b)) = g.line(id) {
        let d = unit(b - a).unwrap_or(Vector::new(1.0, 0.0));
        return Some((a.lerp(b, 0.5), Vector::new(-d.y, d.x)));
    }
    let (c, r) = g.circle(id)?;
    Some((
        Point::new(c.x + r * std::f64::consts::FRAC_1_SQRT_2, c.y + r * std::f64::consts::FRAC_1_SQRT_2),
        Vector::new(std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2),
    ))
}

fn glyph_anchor(g: &SketchGeometry<'_>, relation: &Relation, distance: f64) -> Option<Point> {
    let first = match *relation {
        Relation::Coincident(a, _)
        | Relation::Parallel(a, _)
        | Relation::Perpendicular(a, _)
        | Relation::Tangent(a, _)
        | Relation::Equal(a, _)
        | Relation::Concentric(a, _) => a,
        Relation::PointOn { point, .. } | Relation::Midpoint { point, .. } | Relation::Fix { point, .. } => point,
        Relation::Horizontal(l) | Relation::Vertical(l) => l,
        Relation::Symmetric { about, .. } => about,
        _ => return None,
    };
    let (at, away) = entity_anchor(g, first)?;
    Some(at + away * distance)
}

struct DimensionShape {
    lines: Vec<[Point; 2]>,
    label_at: Point,
}

/// Extension lines and dimension line of a linear dimension between `a` and `b`, moved by `n`.
fn linear(a: Point, b: Point, n: Vector) -> DimensionShape {
    let (a2, b2) = (a + n, b + n);
    DimensionShape { lines: vec![[a, a2], [b, b2], [a2, b2]], label_at: a2.lerp(b2, 0.5) }
}

fn dimension_shape(g: &SketchGeometry<'_>, kind: DimensionKind, mpp: f64) -> Option<DimensionShape> {
    let d = DIMENSION_OFFSET_PT * mpp;
    match kind {
        DimensionKind::Length { line } => {
            let (a, b) = g.line(line)?;
            let dir = unit(b - a)?;
            Some(linear(a, b, Vector::new(-dir.y, dir.x) * d))
        }
        DimensionKind::Distance { a, b, along } => {
            let (pa, pb) = (g.point(a)?, g.point(b)?);
            Some(match along {
                Along::X => {
                    let y = pa.y.max(pb.y) + d;
                    DimensionShape {
                        lines: vec![
                            [pa, Point::new(pa.x, y)],
                            [pb, Point::new(pb.x, y)],
                            [Point::new(pa.x, y), Point::new(pb.x, y)],
                        ],
                        label_at: Point::new((pa.x + pb.x) / 2.0, y),
                    }
                }
                Along::Y => {
                    let x = pa.x.max(pb.x) + d;
                    DimensionShape {
                        lines: vec![
                            [pa, Point::new(x, pa.y)],
                            [pb, Point::new(x, pb.y)],
                            [Point::new(x, pa.y), Point::new(x, pb.y)],
                        ],
                        label_at: Point::new(x, (pa.y + pb.y) / 2.0),
                    }
                }
                _ => {
                    let dir = unit(pb - pa)?;
                    linear(pa, pb, Vector::new(-dir.y, dir.x) * d)
                }
            })
        }
        DimensionKind::Radius { curve } | DimensionKind::Diameter { curve } => {
            let (c, r) = g.circle(curve)?;
            let dir = Vector::new(std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2);
            let diameter = matches!(kind, DimensionKind::Diameter { .. });
            let from = if diameter { c - dir * r } else { c };
            Some(DimensionShape { lines: vec![[from, c + dir * r]], label_at: c + dir * (r + 12.0 * mpp) })
        }
        DimensionKind::Angle { from, to } => {
            let ((a0, a1), (b0, b1)) = (g.line(from)?, g.line(to)?);
            let (u, v) = (unit(a1 - a0)?, unit(b1 - b0)?);
            let cross = u.cross(v);
            if cross.abs() < 1e-9 {
                return Some(DimensionShape {
                    lines: vec![[a0.lerp(a1, 0.5), b0.lerp(b1, 0.5)]],
                    label_at: a0.lerp(b1, 0.5),
                });
            }
            // Intersection of the two infinite lines.
            let t = (b0 - a0).cross(v) / cross;
            let vertex = a0 + u * t;
            let leg = 40.0 * mpp;
            let radius = 24.0 * mpp;
            let (angle_u, angle_v) = (u.y.atan2(u.x), v.y.atan2(v.x));
            let mut sweep = angle_v - angle_u;
            if sweep <= -std::f64::consts::PI {
                sweep += std::f64::consts::TAU;
            } else if sweep > std::f64::consts::PI {
                sweep -= std::f64::consts::TAU;
            }
            const STEPS: usize = 8;
            let arc_point = |k: usize| {
                let a = angle_u + sweep * (k as f64 / STEPS as f64);
                vertex + Vector::new(a.cos(), a.sin()) * radius
            };
            let mut lines = vec![[vertex, vertex + u * leg], [vertex, vertex + v * leg]];
            lines.extend((0..STEPS).map(|k| [arc_point(k), arc_point(k + 1)]));
            let mid = angle_u + sweep / 2.0;
            Some(DimensionShape { lines, label_at: vertex + Vector::new(mid.cos(), mid.sin()) * (radius + 12.0 * mpp) })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        sketch: Sketch,
        p: [EntityId; 3],
        line: EntityId,
        circle: EntityId,
    }

    /// p0 (0,0) — p1 (10,0) with line; circle around p2 (30,0), radius 5.
    fn fixture() -> Fixture {
        let mut s = Sketch::new("t");
        let p = [s.point(0.0, 0.0).unwrap(), s.point(10.0, 0.0).unwrap(), s.point(30.0, 0.0).unwrap()];
        let line = s.line(p[0], p[1]).unwrap();
        let circle = s.circle(p[2], 5.0).unwrap();
        Fixture { sketch: s, p, line, circle }
    }

    #[derive(Default)]
    struct Parts {
        entities: BTreeSet<EntityId>,
        constraints: BTreeSet<ConstraintId>,
        conflicts: BTreeSet<ConstraintId>,
        free: Option<BTreeSet<EntityId>>,
        values: BTreeMap<ConstraintId, String>,
    }

    fn build(sketch: &Sketch, parts: &Parts) -> SketchOverlays {
        sketch_overlays(&SketchView {
            sketch,
            selected_entities: &parts.entities,
            selected_constraints: &parts.constraints,
            conflicts: &parts.conflicts,
            free_points: parts.free.as_ref(),
            values: &parts.values,
            mm_per_point: 0.5,
        })
    }

    fn glyphs(o: &SketchOverlays) -> Vec<(SketchGlyph, SketchMark, Point)> {
        o.overlays
            .iter()
            .filter_map(|x| match x {
                Overlay::SketchGlyph { at, glyph, mark } => Some((*glyph, *mark, *at)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_relation_shows_a_glyph_beside_its_line_and_can_be_picked_there() {
        let mut f = fixture();
        let c = f.sketch.relate(Relation::Horizontal(f.line)).unwrap();
        let o = build(&f.sketch, &Parts::default());
        let g = glyphs(&o);
        assert_eq!(g.len(), 1);
        let (glyph, mark, at) = g[0];
        assert_eq!((glyph, mark), (SketchGlyph::Horizontal, SketchMark::Normal));
        assert!((at.x - 5.0).abs() < 1e-9, "centred along the line: {at:?}");
        assert!((at.y.abs() - 5.0).abs() < 1e-9, "10 points = 5 mm beside it: {at:?}");
        assert_eq!(o.glyphs, vec![(c, at)]);
    }

    #[test]
    fn a_conflicting_relation_and_its_entity_are_marked_with_a_conflict() {
        let mut f = fixture();
        let c = f.sketch.relate(Relation::Vertical(f.line)).unwrap();
        let parts = Parts { conflicts: [c].into(), ..Parts::default() };
        let o = build(&f.sketch, &parts);
        assert_eq!(glyphs(&o)[0].1, SketchMark::Conflict);
        assert!(
            o.overlays.iter().any(|x| matches!(x, Overlay::SketchEntity { mark: SketchMark::Conflict, .. })),
            "the line itself is highlighted"
        );
    }

    #[test]
    fn selected_entities_and_constraints_are_marked() {
        let mut f = fixture();
        let c = f.sketch.relate(Relation::Horizontal(f.line)).unwrap();
        let parts = Parts { entities: [f.circle].into(), constraints: [c].into(), ..Parts::default() };
        let o = build(&f.sketch, &parts);
        assert_eq!(glyphs(&o)[0].1, SketchMark::Selected);
        assert!(o.overlays.iter().any(|x| matches!(x, Overlay::SketchEntity { mark: SketchMark::Selected, .. })));
    }

    #[test]
    fn points_show_their_freedom_and_a_fixed_point_is_pinned() {
        let mut f = fixture();
        f.sketch.relate(Relation::Fix { point: f.p[0], at: Point::new(0.0, 0.0) }).unwrap();
        let parts = Parts { free: Some([f.p[1]].into()), ..Parts::default() };
        let o = build(&f.sketch, &parts);
        let freedom = |id: EntityId| {
            let at = SketchGeometryProbe::at(&f.sketch, id);
            o.overlays.iter().find_map(|x| match x {
                Overlay::SketchPoint { at: p, freedom, .. } if *p == at => Some(*freedom),
                _ => None,
            })
        };
        assert_eq!(freedom(f.p[0]), Some(PointFreedom::Fixed));
        assert_eq!(freedom(f.p[1]), Some(PointFreedom::Free));
        assert_eq!(freedom(f.p[2]), Some(PointFreedom::Determined));
        let unanalysed = build(&f.sketch, &Parts::default());
        assert!(
            unanalysed.overlays.iter().all(|x| !matches!(x, Overlay::SketchPoint { freedom: PointFreedom::Free, .. })),
            "without an analysis nothing is called free"
        );
    }

    struct SketchGeometryProbe;
    impl SketchGeometryProbe {
        fn at(sketch: &Sketch, id: EntityId) -> Point {
            super::super::geometry::SketchGeometry::new(sketch).point(id).unwrap()
        }
    }

    fn dimensions(o: &SketchOverlays) -> Vec<(&Vec<[Point; 2]>, Point, &String, bool)> {
        o.overlays
            .iter()
            .filter_map(|x| match x {
                Overlay::SketchDimension { lines, label_at, text, driven, .. } => {
                    Some((lines, *label_at, text, *driven))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_length_dimension_draws_extension_lines_and_its_value() {
        let mut f = fixture();
        let c = f.sketch.dimension(DimensionKind::Length { line: f.line }, "10 mm", DimensionRole::Driving).unwrap();
        let parts = Parts { values: [(c, "10".to_owned())].into(), ..Parts::default() };
        let o = build(&f.sketch, &parts);
        let d = dimensions(&o);
        assert_eq!(d.len(), 1);
        let (lines, label_at, text, driven) = &d[0];
        assert_eq!((text.as_str(), *driven), ("10", false));
        assert_eq!(lines.len(), 3, "two extension lines and the dimension line");
        assert_eq!(o.labels, vec![(c, *label_at)]);
        assert!((label_at.x - 5.0).abs() < 1e-9);
    }

    #[test]
    fn a_driven_dimension_is_shown_in_brackets_and_radius_and_diameter_have_their_prefix() {
        let mut f = fixture();
        let a = f.sketch.dimension(DimensionKind::Length { line: f.line }, "", DimensionRole::Driven).unwrap();
        let r = f.sketch.dimension(DimensionKind::Radius { curve: f.circle }, "5 mm", DimensionRole::Driving).unwrap();
        let d =
            f.sketch.dimension(DimensionKind::Diameter { curve: f.circle }, "10 mm", DimensionRole::Driven).unwrap();
        let values = [(a, "10".to_owned()), (r, "5".to_owned()), (d, "10".to_owned())].into();
        let o = build(&f.sketch, &Parts { values, ..Parts::default() });
        let texts: Vec<(&str, bool)> = dimensions(&o).iter().map(|(_, _, t, driven)| (t.as_str(), *driven)).collect();
        assert_eq!(texts, vec![("(10)", true), ("R5", false), ("(\u{00D8}10)", true)]);
    }

    #[test]
    fn a_dimension_without_a_computed_value_shows_what_was_typed() {
        let mut f = fixture();
        f.sketch
            .dimension(
                DimensionKind::Distance { a: f.p[0], b: f.p[1], along: Along::X },
                "breedte + 20 mm",
                DimensionRole::Driving,
            )
            .unwrap();
        let o = build(&f.sketch, &Parts::default());
        assert_eq!(dimensions(&o)[0].2, "breedte + 20 mm");
    }
}
