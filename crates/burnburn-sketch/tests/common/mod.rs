//! Shared fixtures: the mounting plate (montageplaat) of the plan.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::sketch::{
    Along, ConstraintId, DimensionKind, DimensionRole, EntityId, Relation, Sketch, SketchCommand, SketchId,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::Point;
use burnburn_types::{DocumentId, RequestId, StableId};

pub struct Plate {
    pub sketch: Sketch,
    pub corners: [EntityId; 4],
    pub edges: [EntityId; 4],
    pub centres: [EntityId; 4],
    pub holes: [EntityId; 4],
    pub width: ConstraintId,
    pub hole_diameter: ConstraintId,
}

/// 120 × 80 plate, four Ø4 holes symmetric about both centre lines, 10 mm from the edges.
/// Drawn roughly: the solver must move everything into place.
pub fn plate() -> Plate {
    let mut s = Sketch::new("Montageplaat");
    s.set_parameter("width", "120 mm").unwrap();
    s.set_parameter("height", "80 mm").unwrap();
    s.set_parameter("offset", "10 mm").unwrap();
    s.set_parameter("hole", "4 mm").unwrap();
    let p0 = s.point(0.5, -0.3).unwrap();
    let p1 = s.point(118.0, 1.0).unwrap();
    let p2 = s.point(121.0, 79.0).unwrap();
    let p3 = s.point(-1.0, 81.0).unwrap();
    let edges = [s.line(p0, p1).unwrap(), s.line(p1, p2).unwrap(), s.line(p2, p3).unwrap(), s.line(p3, p0).unwrap()];
    s.relate(Relation::Fix { point: p0, at: Point::new(0.0, 0.0) }).unwrap();
    s.relate(Relation::Horizontal(edges[0])).unwrap();
    s.relate(Relation::Vertical(edges[1])).unwrap();
    s.relate(Relation::Horizontal(edges[2])).unwrap();
    s.relate(Relation::Vertical(edges[3])).unwrap();
    let width = s
        .dimension(DimensionKind::Distance { a: p0, b: p1, along: Along::X }, "width", DimensionRole::Driving)
        .unwrap();
    s.dimension(DimensionKind::Distance { a: p0, b: p3, along: Along::Y }, "height", DimensionRole::Driving).unwrap();
    // Centre lines as construction geometry.
    let mb = s.point(61.0, 0.0).unwrap();
    let mt = s.point(59.0, 80.0).unwrap();
    let ml = s.point(0.0, 41.0).unwrap();
    let mr = s.point(120.0, 39.0).unwrap();
    s.relate(Relation::Midpoint { point: mb, line: edges[0] }).unwrap();
    s.relate(Relation::Midpoint { point: mt, line: edges[2] }).unwrap();
    s.relate(Relation::Midpoint { point: ml, line: edges[3] }).unwrap();
    s.relate(Relation::Midpoint { point: mr, line: edges[1] }).unwrap();
    let vaxis = s.construction_line(mb, mt).unwrap();
    let haxis = s.construction_line(ml, mr).unwrap();
    let c = [
        s.point(11.0, 9.0).unwrap(),
        s.point(108.0, 11.0).unwrap(),
        s.point(111.0, 71.0).unwrap(),
        s.point(9.0, 69.0).unwrap(),
    ];
    let holes = [
        s.circle(c[0], 2.2).unwrap(),
        s.circle(c[1], 1.9).unwrap(),
        s.circle(c[2], 2.0).unwrap(),
        s.circle(c[3], 2.1).unwrap(),
    ];
    s.dimension(DimensionKind::Distance { a: p0, b: c[0], along: Along::X }, "offset", DimensionRole::Driving).unwrap();
    s.dimension(DimensionKind::Distance { a: p0, b: c[0], along: Along::Y }, "offset", DimensionRole::Driving).unwrap();
    s.relate(Relation::Symmetric { a: c[0], b: c[1], about: vaxis }).unwrap();
    s.relate(Relation::Symmetric { a: c[1], b: c[2], about: haxis }).unwrap();
    s.relate(Relation::Symmetric { a: c[0], b: c[3], about: haxis }).unwrap();
    let hole_diameter =
        s.dimension(DimensionKind::Diameter { curve: holes[0] }, "hole", DimensionRole::Driving).unwrap();
    for h in &holes[1..] {
        s.relate(Relation::Equal(holes[0], *h)).unwrap();
    }
    Plate { sketch: s, corners: [p0, p1, p2, p3], edges, centres: c, holes, width, hole_diameter }
}

pub fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

pub fn sketch_cmd(s: &DocumentSession, c: SketchCommand) -> DocumentCommand {
    DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Sketch(c),
    }
}

pub fn set_param(name: &str, text: &str, sketch: SketchId) -> SketchCommand {
    SketchCommand::SetParameter { sketch, name: name.into(), text: text.into() }
}
