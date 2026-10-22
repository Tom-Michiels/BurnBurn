//! Shared builders for cam contract tests.
#![allow(clippy::unwrap_used, dead_code)]

use burnburn_cam::{CompileOptions, Placement, StartPoint};
use burnburn_document::process::{
    FillSettings, Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand,
    ProcessSettings, ScanAxis, ScanDirection, Speed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Path, Point, ProductionTolerance, Rect};
use burnburn_job::{MachineFrame, MachinePoint, MachineProfile, OriginCorner, PlanEvent, PowerRatio};
use burnburn_types::{DocumentId, GeometryId, Length, OperationId, RequestId, StableId};

pub fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

pub fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

pub fn run(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

pub fn rectangle(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64) -> GeometryId {
    let rect = Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: String::new() }).changes.created[0]
}

pub fn settings(speed: f64, power: f64) -> ProcessSettings {
    let p = PowerPercent::new(power).unwrap();
    ProcessSettings {
        speed: Speed::mm_per_s(speed).unwrap(),
        power: PowerRange::new(p, p).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: true,
    }
}

pub fn line_op(name: &str, speed: f64, power: f64) -> OperationDraft {
    OperationDraft {
        name: name.into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: settings(speed, power),
        output_enabled: true,
    }
}

pub fn fill_op(name: &str, interval: f64, overscan: f64) -> OperationDraft {
    let f = FillSettings::new(mm(interval), ScanAxis::X, ScanDirection::Bidirectional, mm(overscan)).unwrap();
    OperationDraft {
        name: name.into(),
        mode: OperationMode::Fill(f),
        settings: settings(100.0, 20.0),
        output_enabled: true,
    }
}

pub fn add_op(s: &mut DocumentSession, draft: OperationDraft, targets: Vec<GeometryId>) -> OperationId {
    let out = run(s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets }));
    out.changes.operations[0]
}

/// The 1500 × 900 mm research profile of the LightBurn fixtures, origin bottom-left so that
/// document millimetres equal machine millimetres.
pub fn profile() -> MachineProfile {
    profile_with(OriginCorner::BottomLeft)
}

pub fn profile_with(origin: OriginCorner) -> MachineProfile {
    MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, origin).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

pub fn options() -> CompileOptions {
    CompileOptions {
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        placement: Placement::Absolute,
        start: StartPoint::MachineOrigin,
    }
}

/// `(kind, x, y)` of every motion event, for readable assertions.
pub fn motion(events: &[PlanEvent]) -> Vec<(&'static str, f64, f64)> {
    events
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::Travel { to } => Some(("travel", to.x_mm(), to.y_mm())),
            PlanEvent::Cut { to, .. } => Some(("cut", to.x_mm(), to.y_mm())),
            PlanEvent::ScanRun { line, .. } => Some(("scan", line.to.x_mm(), line.to.y_mm())),
            _ => None,
        })
        .collect()
}

pub fn path_of(points: &[(f64, f64)]) -> Path {
    let pts: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
    let mut b = burnburn_geometry::PathBuilder::new().move_to(pts[0]);
    for p in &pts[1..] {
        b = b.line_to(*p);
    }
    b.close().build().unwrap()
}

pub fn mp(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
}

/// Compiles with the head at (−1, −1), so the first travel to an entry at (0, 0) is visible.
pub fn plan_from_outside(
    s: &DocumentSession,
    p: &MachineProfile,
) -> Result<burnburn_job::JobPlan, burnburn_cam::CompileError> {
    let mut o = options();
    o.start = StartPoint::Bed(Point::new(-1.0, -1.0));
    burnburn_cam::compile(&s.snapshot(), p, &o, &burnburn_types::CancellationToken::never())
}

/// A profile whose matrix supports exactly the given extra features.
pub fn supporting(features: &[burnburn_job::Feature]) -> MachineProfile {
    let caps = features
        .iter()
        .fold(burnburn_job::MachineCapabilities::conservative(), |c, f| c.with(*f, burnburn_job::Support::Supported));
    profile().with_capabilities(caps).unwrap()
}

/// Total length of the `Cut` events (the head position before a cut is the previous event's end).
pub fn cut_length(plan: &burnburn_job::JobPlan) -> f64 {
    let mut at = plan.start();
    let mut total = 0.0;
    for e in plan.events() {
        match *e {
            // A cut at its own power moves the head but is not counted: this is full-power cutting.
            PlanEvent::Travel { to } | PlanEvent::CutPower { to, .. } => at = to,
            PlanEvent::Cut { to, .. } => {
                total += at.distance(to);
                at = to;
            }
            PlanEvent::ScanRun { line, .. } | PlanEvent::RampRun { line, .. } => at = line.to,
            _ => {}
        }
    }
    total
}

pub fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}
