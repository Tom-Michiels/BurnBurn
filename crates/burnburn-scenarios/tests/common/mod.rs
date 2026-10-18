//! Shared scenario helpers: a document session driver, the research machine profile, the plate
//! sketch, and an RD motion reader that turns a decoded RD file back into absolute cut segments.
//!
//! The expectations in the scenarios are derived by hand from the drawing (plate size, hole
//! centres, ring radii, SVG/DXF text), never from an earlier run of the chain.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)] // shared test helpers

use burnburn_cam::{CompileOptions, Placement, StartPoint, compile};
use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, Speed,
};
use burnburn_document::sketch::{Along, DimensionKind, DimensionRole, Relation, Sketch};
use burnburn_document::{
    CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, DocumentSnapshot, Evaluator,
};
use burnburn_geometry::{Point, ProductionTolerance};
use burnburn_job::{JobPlan, MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_ruida::{BoundsKind, Command, Magic, OverscanPolicy, RdFile, RuidaProfile, decode_rd_file, lower};
use burnburn_types::{CancellationToken, DocumentId, GeometryId, Length, RequestId, StableId};

pub const RUIDA: RuidaProfile =
    RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };

/// Flattening tolerance of every scenario compile (mm).
pub const TOLERANCE_MM: f64 = 0.001;

pub fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

/// 1500 × 900 mm research profile with a bottom-left origin: document mm = RD machine mm.
pub fn profile() -> MachineProfile {
    MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

pub fn options() -> CompileOptions {
    CompileOptions {
        tolerance: ProductionTolerance::new(mm(TOLERANCE_MM)).unwrap(),
        placement: Placement::Absolute,
        start: StartPoint::MachineOrigin,
    }
}

/// Applies `payload` to `s` at its current revision with the given evaluators.
pub fn apply(s: &mut DocumentSession, payload: CommandPayload, evaluators: &[&dyn Evaluator]) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply_with(cmd, evaluators).unwrap()
}

pub fn line_cut(name: &str) -> OperationDraft {
    let p = PowerPercent::new(20.0).unwrap();
    OperationDraft {
        name: name.into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

pub fn add_cut(s: &mut DocumentSession, targets: Vec<GeometryId>) {
    apply(s, CommandPayload::Process(ProcessCommand::AddOperation { draft: line_cut("Snijden"), targets }), &[]);
}

pub fn compile_plan(snapshot: &DocumentSnapshot) -> JobPlan {
    compile(snapshot, &profile(), &options(), &CancellationToken::never()).unwrap()
}

/// JobPlan → RD bytes (scrambled with the fixture magic).
pub fn rd_bytes(plan: &JobPlan) -> Vec<u8> {
    let program = lower(plan, &RUIDA).unwrap();
    assert_eq!(program.plan(), plan.id(), "the RD program names the plan it came from");
    program.to_rd_file(RUIDA.magic).unwrap()
}

pub fn decode(bytes: &[u8]) -> RdFile {
    decode_rd_file(bytes, RUIDA.magic).unwrap()
}

/// One laser-on segment in machine millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cut {
    pub from: Point,
    pub to: Point,
}

/// Every laser-on segment of `file`, relative moves resolved to absolute machine mm.
pub fn cuts(file: &RdFile) -> Vec<Cut> {
    let mut at = (0i64, 0i64);
    let mut out = Vec::new();
    let pt = |p: (i64, i64)| Point::new(p.0 as f64 / 1000.0, p.1 as f64 / 1000.0);
    for c in file.commands() {
        let (to, cut) = match *c {
            Command::MoveAbs { x, y } => ((x.um() as i64, y.um() as i64), false),
            Command::MoveRel { dx, dy } => ((at.0 + i64::from(dx.um()), at.1 + i64::from(dy.um())), false),
            Command::MoveRelX(d) => ((at.0 + i64::from(d.um()), at.1), false),
            Command::MoveRelY(d) => ((at.0, at.1 + i64::from(d.um())), false),
            Command::CutAbs { x, y } => ((x.um() as i64, y.um() as i64), true),
            Command::CutRel { dx, dy } => ((at.0 + i64::from(dx.um()), at.1 + i64::from(dy.um())), true),
            Command::CutRelX(d) => ((at.0 + i64::from(d.um()), at.1), true),
            Command::CutRelY(d) => ((at.0, at.1 + i64::from(d.um())), true),
            _ => continue,
        };
        if cut {
            out.push(Cut { from: pt(at), to: pt(to) });
        }
        at = to;
    }
    out
}

/// Document bounds from the RD header (`E7 03` / `E7 07`), mm.
pub fn header_bounds(file: &RdFile) -> (Point, Point) {
    let find = |k| {
        file.commands()
            .find_map(|c| match *c {
                Command::Bounds { kind, x, y } if kind == k => {
                    Some(Point::new(x.um() as f64 / 1000.0, y.um() as f64 / 1000.0))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no {k:?} in RD"))
    };
    (find(BoundsKind::DocumentMin), find(BoundsKind::DocumentMax))
}

/// Extent of the cut motion itself, mm (independent of the header).
pub fn cut_extent(cuts: &[Cut]) -> (Point, Point) {
    let mut lo = Point::new(f64::INFINITY, f64::INFINITY);
    let mut hi = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for c in cuts {
        for p in [c.from, c.to] {
            lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
        }
    }
    (lo, hi)
}

/// Connected closed loops of consecutive cuts (a loop ends where the next cut does not start).
pub fn loops(cuts: &[Cut]) -> Vec<Vec<Point>> {
    let mut out: Vec<Vec<Point>> = Vec::new();
    for c in cuts {
        match out.last_mut() {
            Some(l) if l.last().is_some_and(|p| p.distance(c.from) < 1e-9) => l.push(c.to),
            _ => out.push(vec![c.from, c.to]),
        }
    }
    out
}

pub fn centroid(points: &[Point]) -> Point {
    let n = points.len() as f64;
    Point::new(points.iter().map(|p| p.x).sum::<f64>() / n, points.iter().map(|p| p.y).sum::<f64>() / n)
}

/// 120 × 80 plate, four Ø4 holes symmetric about both centre lines, 10 mm from the edges (the
/// plan's montageplaat). Same construction as `burnburn-presentation/tests/gate_b.rs`; the
/// initial guesses are deliberately off so the solver has to move every point.
pub fn plate() -> Sketch {
    let mut s = Sketch::new("Montageplaat");
    for (n, v) in [("width", "120 mm"), ("height", "80 mm"), ("offset", "10 mm"), ("hole", "4 mm")] {
        s.set_parameter(n, v).unwrap();
    }
    let p0 = s.point(0.5, -0.3).unwrap();
    let p1 = s.point(118.0, 1.0).unwrap();
    let p2 = s.point(121.0, 79.0).unwrap();
    let p3 = s.point(-1.0, 81.0).unwrap();
    let e = [s.line(p0, p1).unwrap(), s.line(p1, p2).unwrap(), s.line(p2, p3).unwrap(), s.line(p3, p0).unwrap()];
    s.relate(Relation::Fix { point: p0, at: Point::new(0.0, 0.0) }).unwrap();
    s.relate(Relation::Horizontal(e[0])).unwrap();
    s.relate(Relation::Vertical(e[1])).unwrap();
    s.relate(Relation::Horizontal(e[2])).unwrap();
    s.relate(Relation::Vertical(e[3])).unwrap();
    s.dimension(DimensionKind::Distance { a: p0, b: p1, along: Along::X }, "width", DimensionRole::Driving).unwrap();
    s.dimension(DimensionKind::Distance { a: p0, b: p3, along: Along::Y }, "height", DimensionRole::Driving).unwrap();
    let mb = s.point(61.0, 0.0).unwrap();
    let mt = s.point(59.0, 80.0).unwrap();
    let ml = s.point(0.0, 41.0).unwrap();
    let mr = s.point(120.0, 39.0).unwrap();
    s.relate(Relation::Midpoint { point: mb, line: e[0] }).unwrap();
    s.relate(Relation::Midpoint { point: mt, line: e[2] }).unwrap();
    s.relate(Relation::Midpoint { point: ml, line: e[3] }).unwrap();
    s.relate(Relation::Midpoint { point: mr, line: e[1] }).unwrap();
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
    s.dimension(DimensionKind::Diameter { curve: holes[0] }, "hole", DimensionRole::Driving).unwrap();
    for h in &holes[1..] {
        s.relate(Relation::Equal(holes[0], *h)).unwrap();
    }
    s
}

pub fn new_session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

/// Derived (sketch output) objects of `snap`, in id order.
pub fn derived_objects(snap: &DocumentSnapshot) -> Vec<GeometryId> {
    snap.objects()
        .filter(|(_, o)| matches!(o.source(), burnburn_document::GeometrySource::Derived(_)))
        .map(|(id, _)| id)
        .collect()
}
