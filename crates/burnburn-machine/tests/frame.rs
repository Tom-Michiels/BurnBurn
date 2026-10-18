//! Laser-off framing (M05) as a plan derivation. No controller involved.
#![allow(clippy::unwrap_used)]

use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanEvent, PlanMode,
    PlanSource, PowerRatio, ProfileRevision, ScanAxis, ScanLine,
};
use burnburn_machine::{FrameError, FrameOptions, frame_plan};
use burnburn_ruida::{Command, Magic, OverscanPolicy, RuidaProfile, decode, lower, unscramble};
use burnburn_types::{ContentHash, HashDomain, OperationId, RevisionId, StableId};

fn p(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
}

fn builder() -> JobPlanBuilder {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"frame-test"),
        machine_profile: ProfileRevision {
            id: "bench".into(),
            revision: 1,
            content: ContentHash::of(HashDomain::Profile, b"bench"),
        },
        compiler_version: "test".into(),
    };
    JobPlanBuilder::new(source, MachineFrame::new(300.0, 200.0, OriginCorner::BottomLeft).unwrap(), p(0.0, 0.0))
}

/// A 40 × 20 mm rectangle cut at 60 % plus a fill strip with 2 mm overscan.
fn production_plan() -> JobPlan {
    let mut b = builder();
    let cut = OperationSettings {
        mode: PlanMode::Line,
        speed_mm_s: 20.0,
        min_power: PowerRatio::new(0.5).unwrap(),
        max_power: PowerRatio::new(0.6).unwrap(),
        air_assist: true,
    };
    let scan = OperationSettings {
        mode: PlanMode::Scan { interval_mm: 0.1, axis: ScanAxis::X, bidirectional: true },
        speed_mm_s: 200.0,
        min_power: PowerRatio::new(0.1).unwrap(),
        max_power: PowerRatio::new(0.2).unwrap(),
        air_assist: false,
    };
    let (c, s) = (b.add_settings(cut).unwrap(), b.add_settings(scan).unwrap());
    let op = OperationId::from_raw(1);
    b.push(PlanEvent::OperationBoundary { operation: op, settings: c, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: p(10.0, 20.0) }).unwrap();
    for to in [p(50.0, 20.0), p(50.0, 40.0), p(10.0, 40.0), p(10.0, 20.0)] {
        b.push(PlanEvent::Cut { to, origin: None }).unwrap();
    }
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(2), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: p(20.0, 30.0) }).unwrap();
    let line = ScanLine { from: p(20.0, 30.0), to: p(60.0, 30.0), overscan_mm: 2.0 };
    b.push(PlanEvent::ScanRun { line, origin: None }).unwrap();
    b.finish().unwrap()
}

const OPTIONS: FrameOptions = FrameOptions { speed_mm_s: 100.0 };

#[test]
fn frame_plan_follows_bounds_with_all_powers_zero() {
    let plan = production_plan();
    let frame = frame_plan(&plan, &OPTIONS).unwrap();
    assert_eq!(frame.source(), plan.id());
    let f = frame.plan();
    assert_ne!(f.id(), plan.id(), "a frame is its own plan");
    for s in f.settings() {
        assert_eq!((s.min_power.get(), s.max_power.get()), (0.0, 0.0), "frame power must be zero: {s:?}");
        assert!(!s.air_assist);
    }
    assert!(
        f.events().iter().all(|e| matches!(e, PlanEvent::Travel { .. } | PlanEvent::OperationBoundary { .. })),
        "only laser-off travel: {:?}",
        f.events()
    );
    // Bounds include the scan overscan: x 10..62, y 20..40.
    let (want, got) = (plan.bounds().unwrap(), f.bounds().unwrap());
    assert_eq!((got.min(), got.max()), (want.min(), want.max()));
    assert_eq!((want.min().x, want.max().x), (10.0, 62.0));
    let travels: Vec<_> =
        f.events().iter().filter_map(|e| if let PlanEvent::Travel { to } = e { Some(*to) } else { None }).collect();
    assert_eq!(travels.first(), travels.last(), "frame returns to its first corner");
    assert_eq!(travels.len(), 5);
}

#[test]
fn frame_program_contains_no_laser_on_command() {
    let frame = frame_plan(&production_plan(), &OPTIONS).unwrap();
    let ruida = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::Reject };
    let rd = lower(frame.plan(), &ruida).unwrap().to_rd_file(ruida.magic).unwrap();
    // Decode the bytes that would be uploaded, not the in-memory command list.
    let records = decode(&unscramble(&rd, ruida.magic)).unwrap();
    for r in &records {
        match &r.command {
            Command::CutAbs { .. } | Command::CutRel { .. } | Command::CutRelX(_) | Command::CutRelY(_) => {
                panic!("laser-on motion in frame program at offset {}: {:?}", r.offset, r.command)
            }
            Command::LaserPower { value, .. }
            | Command::PartPower { value, .. }
            | Command::ImmediatePower { value, .. }
            | Command::ThroughPower { value, .. } => {
                assert_eq!(value.raw(), 0, "non-zero power at offset {}: {:?}", r.offset, r.command)
            }
            _ => {}
        }
    }
    assert!(records.iter().any(|r| matches!(r.command, Command::MoveAbs { .. })), "frame must move");
}

#[test]
fn plan_without_motion_has_no_frame() {
    let plan = builder().finish().unwrap();
    assert_eq!(frame_plan(&plan, &OPTIONS), Err(FrameError::NoMotion));
}
