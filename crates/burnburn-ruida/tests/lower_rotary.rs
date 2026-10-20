//! W2-L3: RD lowering of a rotary plan. The plan is in surface millimetres; the lowering maps
//! the Y axis onto controller micrometres. The oracle is the decoded `.rd` file and a circumference
//! chosen so that the expected scale is exactly 2.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_cam::{CompileOptions, Placement, StartPoint, compile};
use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, Speed,
};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Point, ProductionTolerance, Rect};
use burnburn_job::{
    JobPlan, MachineFrame, MachineProfile, OriginCorner, PowerRatio, RotaryConfig, RotaryKind, ScanAxis,
};
use burnburn_ruida::{Command, Magic, OverscanPolicy, RuidaProfile, decode_rd_file, lower};
use burnburn_types::{CancellationToken, DocumentId, Length, RequestId, StableId};

const RUIDA: RuidaProfile = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };
const D200: f64 = 200.0 / std::f64::consts::PI;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn bed() -> MachineProfile {
    MachineProfile::new(
        "bed",
        1,
        MachineFrame::new(600.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        500.0,
        300.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn rotary(mirror: bool) -> RotaryConfig {
    // 200 mm of surface per rotation, 8000 steps per rotation, 20 steps/mm: scale exactly 2.
    RotaryConfig::new(RotaryKind::Chuck { object_diameter_mm: D200 }, 8000, 20.0, mirror).unwrap()
}

/// A 30 × 50 mm rectangle at (10, 20) cut on `profile`.
fn plan_on(profile: &MachineProfile) -> JobPlan {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut apply = |payload| {
        let cmd =
            DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
        s.apply(cmd).unwrap()
    };
    let rect = Rect::new(Point::new(10.0, 20.0), mm(30.0), mm(50.0)).unwrap();
    let g = apply(CommandPayload::CreateRectangle { rect, name: String::new() }).changes.created[0];
    let p = PowerPercent::new(20.0).unwrap();
    let draft = OperationDraft {
        name: "C00".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: false,
        },
        output_enabled: true,
    };
    apply(CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![g] }));
    let options = CompileOptions {
        tolerance: ProductionTolerance::new(mm(0.001)).unwrap(),
        placement: Placement::Absolute,
        start: StartPoint::MachineOrigin,
    };
    compile(&s.snapshot(), profile, &options, &CancellationToken::never()).unwrap()
}

/// Decoded absolute cut/travel targets and the process bounds, in µm.
fn decoded(plan: &JobPlan) -> (Vec<(i64, i64)>, ((u64, u64), (u64, u64))) {
    let bytes = lower(plan, &RUIDA).unwrap().to_rd_file(Magic::FIXTURE_0X88).unwrap();
    let file = decode_rd_file(&bytes, Magic::FIXTURE_0X88).unwrap();
    let mut at = (0i64, 0i64);
    let mut points = Vec::new();
    let mut bounds = [None, None];
    for c in file.commands() {
        match *c {
            Command::MoveAbs { x, y } | Command::CutAbs { x, y } => {
                at = (x.um() as i64, y.um() as i64);
                points.push(at);
            }
            Command::CutRelX(d) => {
                at.0 += i64::from(d.um());
                points.push(at);
            }
            Command::MoveRelY(d) => {
                at.1 += i64::from(d.um());
                points.push(at);
            }
            Command::Bounds { kind: burnburn_ruida::BoundsKind::ProcessTopLeft, x, y } => {
                bounds[0] = Some((x.um(), y.um()))
            }
            Command::Bounds { kind: burnburn_ruida::BoundsKind::ProcessBottomRight, x, y } => {
                bounds[1] = Some((x.um(), y.um()))
            }
            _ => {}
        }
    }
    (points, (bounds[0].unwrap(), bounds[1].unwrap()))
}

#[test]
fn the_flat_bed_is_lowered_as_before() {
    let (points, bounds) = decoded(&plan_on(&bed()));
    assert_eq!(bounds, ((10_000, 20_000), (40_000, 70_000)));
    assert!(points.iter().any(|p| p.1 == 70_000));
}

#[test]
fn a_rotary_scales_only_the_y_axis_onto_the_controller() {
    let (points, bounds) = decoded(&plan_on(&bed().with_rotary(rotary(false)).unwrap()));
    // X is untouched; the surface Y range 20..70 mm becomes 40..140 controller mm.
    assert_eq!(bounds, ((10_000, 40_000), (40_000, 140_000)));
    assert!(points.iter().all(|p| (10_000..=40_000).contains(&p.0)), "{points:?}");
    let mut ys: Vec<i64> = points.iter().map(|p| p.1).collect();
    ys.sort_unstable();
    ys.dedup();
    assert_eq!(ys, vec![40_000, 140_000]);
}

#[test]
fn a_mirrored_rotary_turns_the_object_the_other_way() {
    let (_, bounds) = decoded(&plan_on(&bed().with_rotary(rotary(true)).unwrap()));
    // Frame height 200 (one wrap): surface 20..70 becomes (200 − y) · 2 = 360..260 controller mm.
    assert_eq!(bounds, ((10_000, 260_000), (40_000, 360_000)));
}

#[test]
fn a_rotary_plan_and_a_flat_plan_lower_to_different_programs_for_the_same_document() {
    let flat = lower(&plan_on(&bed()), &RUIDA).unwrap();
    let turned = lower(&plan_on(&bed().with_rotary(rotary(false)).unwrap()), &RUIDA).unwrap();
    assert_ne!(flat.body(), turned.body());
    let _ = ScanAxis::X;
}
