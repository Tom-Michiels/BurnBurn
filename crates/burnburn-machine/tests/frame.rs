//! Laser-off framing (M05) as a plan derivation. No controller involved.
#![allow(clippy::unwrap_used)]

use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanEvent, PlanMode,
    PlanSource, PowerRatio, ProfileRevision, ScanAxis, ScanLine,
};
use burnburn_machine::{FrameError, FrameOptions, FrameShape, frame_plan, frame_plan_shaped};
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

// ---- W1-L4a: rubber-band frame (convex hull) ----

fn travels(plan: &JobPlan) -> Vec<(f64, f64)> {
    plan.events()
        .iter()
        .filter_map(|e| if let PlanEvent::Travel { to } = e { Some((to.x_mm(), to.y_mm())) } else { None })
        .collect()
}

/// Cut triangle (10,10) (50,10) (30,40) with a travel to an interior point and a cut inside.
fn triangle_plan() -> JobPlan {
    let mut b = builder();
    let cut = OperationSettings {
        mode: PlanMode::Line,
        speed_mm_s: 20.0,
        min_power: PowerRatio::new(0.5).unwrap(),
        max_power: PowerRatio::new(0.6).unwrap(),
        air_assist: false,
    };
    let s = b.add_settings(cut).unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: p(10.0, 10.0) }).unwrap();
    for to in [p(50.0, 10.0), p(30.0, 40.0), p(10.0, 10.0)] {
        b.push(PlanEvent::Cut { to, origin: None }).unwrap();
    }
    b.push(PlanEvent::Travel { to: p(30.0, 20.0) }).unwrap();
    b.push(PlanEvent::Cut { to: p(32.0, 22.0), origin: None }).unwrap();
    b.finish().unwrap()
}

#[test]
fn rubber_band_follows_the_convex_hull_while_the_bounding_frame_stays_a_rectangle() {
    let plan = triangle_plan();
    let band = frame_plan_shaped(&plan, &OPTIONS, FrameShape::RubberBand).unwrap();
    // Counter-clockwise from the lowest-left vertex, closed; interior points are not visited.
    assert_eq!(travels(band.plan()), vec![(10.0, 10.0), (50.0, 10.0), (30.0, 40.0), (10.0, 10.0)]);
    let rect = frame_plan_shaped(&plan, &OPTIONS, FrameShape::BoundingRectangle).unwrap();
    assert_eq!(
        travels(rect.plan()),
        vec![(10.0, 10.0), (50.0, 10.0), (50.0, 40.0), (10.0, 40.0), (10.0, 10.0)]
    );
    assert_eq!(frame_plan(&plan, &OPTIONS).unwrap().plan().events(), rect.plan().events(), "frame_plan = bounds");
    assert_ne!(band.plan().id(), rect.plan().id(), "the shape is part of the frame plan's identity");
}

#[test]
fn rubber_band_is_laser_off_travel_only_with_zero_power() {
    let band = frame_plan_shaped(&triangle_plan(), &OPTIONS, FrameShape::RubberBand).unwrap();
    for s in band.plan().settings() {
        assert_eq!((s.min_power.get(), s.max_power.get()), (0.0, 0.0));
    }
    assert!(band
        .plan()
        .events()
        .iter()
        .all(|e| matches!(e, PlanEvent::Travel { .. } | PlanEvent::OperationBoundary { .. })));
    assert_eq!(band.source(), triangle_plan().id());
}

#[test]
fn rubber_band_includes_scan_overscan_like_the_bounds_do() {
    let band = frame_plan_shaped(&production_plan(), &OPTIONS, FrameShape::RubberBand).unwrap();
    let t = travels(band.plan());
    assert!(t.contains(&(62.0, 30.0)) || t.iter().any(|&(x, _)| x == 62.0), "overscan end missing: {t:?}");
    assert!(t.iter().all(|&(x, _)| (10.0..=62.0).contains(&x)));
}

#[test]
fn rubber_band_of_collinear_or_single_point_motion_is_there_and_back() {
    let mut b = builder();
    let s = b
        .add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s: 20.0,
            min_power: PowerRatio::new(0.1).unwrap(),
            max_power: PowerRatio::new(0.2).unwrap(),
            air_assist: false,
        })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: p(5.0, 5.0) }).unwrap();
    b.push(PlanEvent::Cut { to: p(25.0, 5.0), origin: None }).unwrap();
    b.push(PlanEvent::Cut { to: p(15.0, 5.0), origin: None }).unwrap();
    let line = b.finish().unwrap();
    let band = frame_plan_shaped(&line, &OPTIONS, FrameShape::RubberBand).unwrap();
    assert_eq!(travels(band.plan()), vec![(5.0, 5.0), (25.0, 5.0), (5.0, 5.0)]);

    let mut b = builder();
    let s = b
        .add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s: 20.0,
            min_power: PowerRatio::new(0.1).unwrap(),
            max_power: PowerRatio::new(0.2).unwrap(),
            air_assist: false,
        })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: p(7.0, 9.0) }).unwrap();
    let dot = b.finish().unwrap();
    let band = frame_plan_shaped(&dot, &OPTIONS, FrameShape::RubberBand).unwrap();
    assert_eq!(travels(band.plan()), vec![(7.0, 9.0)]);
}

#[test]
fn rubber_band_is_a_strictly_convex_polygon_that_contains_every_point() {
    // Deterministic scatter (LCG) so the invariant is checked on many shapes, not on one example.
    let mut seed = 0x2545_f491_u64;
    let mut next = move || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 33) % 28_000) as f64 / 100.0 // 0..280 mm
    };
    for round in 0..25 {
        let n = 3 + round;
        let pts: Vec<(f64, f64)> = (0..n).map(|_| (next(), next())).collect();
        let mut b = builder();
        let s = b
            .add_settings(OperationSettings {
                mode: PlanMode::Line,
                speed_mm_s: 20.0,
                min_power: PowerRatio::new(0.1).unwrap(),
                max_power: PowerRatio::new(0.2).unwrap(),
                air_assist: false,
            })
            .unwrap();
        b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
        for &(x, y) in &pts {
            b.push(PlanEvent::Travel { to: p(x, y) }).unwrap();
        }
        let plan = b.finish().unwrap();
        let band = frame_plan_shaped(&plan, &OPTIONS, FrameShape::RubberBand).unwrap();
        let mut hull = travels(band.plan());
        assert_eq!(hull.first(), hull.last(), "closed");
        hull.pop();
        let cross = |o: (f64, f64), a: (f64, f64), c: (f64, f64)| (a.0 - o.0) * (c.1 - o.1) - (a.1 - o.1) * (c.0 - o.0);
        let m = hull.len();
        if m >= 3 {
            for i in 0..m {
                assert!(cross(hull[i], hull[(i + 1) % m], hull[(i + 2) % m]) > 0.0, "round {round}: not strictly convex");
            }
        }
        for &q in &pts {
            for i in 0..m {
                assert!(cross(hull[i], hull[(i + 1) % m], q) >= -1e-9, "round {round}: {q:?} outside the band");
            }
        }
        // Every hull vertex is one of the plan's own points.
        assert!(hull.iter().all(|h| pts.contains(h)), "round {round}");
    }
}

#[test]
fn rubber_band_of_a_plan_without_motion_has_no_frame() {
    let plan = builder().finish().unwrap();
    assert_eq!(frame_plan_shaped(&plan, &OPTIONS, FrameShape::RubberBand), Err(FrameError::NoMotion));
}
