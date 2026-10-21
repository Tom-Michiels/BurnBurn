//! Hand-built `JobPlan`s with analytically known geometry (see `tests/fixtures/README.md`).
#![allow(dead_code, clippy::unwrap_used)]

use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanEvent, PlanMode,
    PlanSource, PowerRatio, ProfileRevision, ScanAxis, ScanLine, SettingsIndex,
};
use burnburn_types::{ContentHash, OperationId, RevisionId, StableId};

pub fn pt(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
}

pub fn frame() -> MachineFrame {
    MachineFrame::new(400.0, 300.0, OriginCorner::BottomLeft).unwrap()
}

fn source() -> PlanSource {
    PlanSource {
        document_revision: RevisionId::from_raw(1),
        source_hash: ContentHash::from_bytes([9; 32]),
        machine_profile: ProfileRevision {
            id: "grbl-test".into(),
            revision: 1,
            content: ContentHash::from_bytes([4; 32]),
        },
        compiler_version: "gcode-test".into(),
    }
}

pub fn settings(mode: PlanMode, speed_mm_s: f64, power: f64, air: bool) -> OperationSettings {
    let p = PowerRatio::new(power).unwrap();
    OperationSettings { mode, speed_mm_s, min_power: p, max_power: p, air_assist: air }
}

pub fn builder() -> JobPlanBuilder {
    JobPlanBuilder::new(source(), frame(), pt(0.0, 0.0))
}

pub fn boundary(b: &mut JobPlanBuilder, s: SettingsIndex, op: u64, pass: u32) {
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(op), settings: s, pass }).unwrap();
}

/// A 40 x 20 mm rectangle with its lower-left corner at (10, 10), cut once at 20 mm/s, 60 % power.
/// Analytical: cut length 120 mm, burn time 6 s, energy 3.6 ratio-seconds, bounds (10,10)-(50,30).
pub fn rectangle_cut(air: bool) -> JobPlan {
    rectangle_passes(1, air)
}

pub fn rectangle_passes(passes: u32, air: bool) -> JobPlan {
    let mut b = builder();
    let s = b.add_settings(settings(PlanMode::Line, 20.0, 0.6, air)).unwrap();
    for pass in 0..passes {
        boundary(&mut b, s, 1, pass);
        b.push(PlanEvent::Travel { to: pt(10.0, 10.0) }).unwrap();
        for (x, y) in [(50.0, 10.0), (50.0, 30.0), (10.0, 30.0), (10.0, 10.0)] {
            b.push(PlanEvent::Cut { to: pt(x, y), origin: None }).unwrap();
        }
    }
    b.finish().unwrap()
}

/// Fill of a 20 x 5 mm area: 6 scan rows (y = 100..=105, interval 1 mm; or columns x = 50..=55 along y 100..120 for
/// the Y axis), bidirectional, overscan 2 mm,
/// 30 % power at 100 mm/s. Analytical: burn length 6 x 20 = 120 mm, burn time 1.2 s, energy 0.36
/// ratio-seconds, burn bounds (50,100)-(70,105), overscan bounds x 48..72.
pub fn fill_scan(axis_x: bool) -> JobPlan {
    let mut b = builder();
    let mode =
        PlanMode::Scan { interval_mm: 1.0, axis: if axis_x { ScanAxis::X } else { ScanAxis::Y }, bidirectional: true };
    let s = b.add_settings(settings(mode, 100.0, 0.3, false)).unwrap();
    boundary(&mut b, s, 2, 0);
    for row in 0..6 {
        let k = f64::from(row);
        let (a, z) = if row % 2 == 0 { (50.0, 70.0) } else { (70.0, 50.0) };
        // X scan: rows at y = 100 + k along x 50..70. Y scan: columns at x = 50 + k along y 100..120.
        let (from, to) = if axis_x {
            (pt(a, 100.0 + k), pt(z, 100.0 + k))
        } else {
            (pt(50.0 + k, a + 50.0), pt(50.0 + k, z + 50.0))
        };
        if b.position() != from {
            b.push(PlanEvent::Travel { to: from }).unwrap();
        }
        b.push(PlanEvent::ScanRun { line: ScanLine { from, to, overscan_mm: 2.0 }, origin: None }).unwrap();
    }
    b.finish().unwrap()
}

/// Grayscale: three runs of 10 mm at 100 mm/s with powers 0.2, 0.5, 1.0 (minimum 0.0, maximum 1.0).
/// Energy = (0.2 + 0.5 + 1.0) * 0.1 s = 0.17 ratio-seconds.
pub fn grayscale() -> JobPlan {
    let mut b = builder();
    let p = |v| PowerRatio::new(v).unwrap();
    let st = OperationSettings {
        mode: PlanMode::GrayscaleScan { interval_mm: 0.5, bidirectional: false },
        speed_mm_s: 100.0,
        min_power: p(0.0),
        max_power: p(1.0),
        air_assist: false,
    };
    let s = b.add_settings(st).unwrap();
    boundary(&mut b, s, 3, 0);
    for (i, power) in [0.2, 0.5, 1.0].into_iter().enumerate() {
        let y = 20.0 + 0.5 * i as f64;
        b.push(PlanEvent::Travel { to: pt(30.0, y) }).unwrap();
        b.push(PlanEvent::PowerRun {
            line: ScanLine { from: pt(30.0, y), to: pt(40.0, y), overscan_mm: 0.0 },
            power: p(power),
            origin: None,
        })
        .unwrap();
    }
    b.finish().unwrap()
}

/// Dots mode with two pulses.
pub fn dots() -> JobPlan {
    let mut b = builder();
    let s = b.add_settings(settings(PlanMode::Dots { dwell_ms: 5.0 }, 100.0, 0.5, false)).unwrap();
    boundary(&mut b, s, 4, 0);
    b.push(PlanEvent::Travel { to: pt(5.0, 5.0) }).unwrap();
    b.push(PlanEvent::Dot { origin: None }).unwrap();
    b.finish().unwrap()
}

/// Ramp run (power ramps at the ends).
pub fn ramp() -> JobPlan {
    let mut b = builder();
    let mode = PlanMode::Scan { interval_mm: 1.0, axis: ScanAxis::X, bidirectional: false };
    let s = b.add_settings(settings(mode, 100.0, 0.5, false)).unwrap();
    boundary(&mut b, s, 5, 0);
    b.push(PlanEvent::Travel { to: pt(10.0, 10.0) }).unwrap();
    b.push(PlanEvent::RampRun {
        line: ScanLine { from: pt(10.0, 10.0), to: pt(30.0, 10.0), overscan_mm: 0.0 },
        ramp_mm: 2.0,
        origin: None,
    })
    .unwrap();
    b.finish().unwrap()
}

/// A line plan with a focus (Z) offset event.
pub fn focus_offset() -> JobPlan {
    let mut b = builder();
    let s = b.add_settings(settings(PlanMode::Line, 20.0, 0.5, false)).unwrap();
    boundary(&mut b, s, 6, 0);
    b.push(PlanEvent::FocusOffset { z_mm: 1.5 }).unwrap();
    b.push(PlanEvent::Travel { to: pt(5.0, 5.0) }).unwrap();
    b.push(PlanEvent::Cut { to: pt(15.0, 5.0), origin: None }).unwrap();
    b.finish().unwrap()
}

/// Angled scan: one diagonal run (3,4)-(63,84): length 100 mm (3-4-5 triangle scaled by 20).
pub fn angled() -> JobPlan {
    let mut b = builder();
    let mode = PlanMode::AngledScan { interval_mm: 1.0, angle_deg: 53.13010235415598, bidirectional: false };
    let s = b.add_settings(settings(mode, 50.0, 0.4, false)).unwrap();
    boundary(&mut b, s, 7, 0);
    b.push(PlanEvent::Travel { to: pt(3.0, 4.0) }).unwrap();
    b.push(PlanEvent::ScanRun {
        line: ScanLine { from: pt(3.0, 4.0), to: pt(63.0, 84.0), overscan_mm: 0.0 },
        origin: None,
    })
    .unwrap();
    b.finish().unwrap()
}

/// A cut that leaves the 400 x 300 mm bed.
pub fn outside_bed() -> JobPlan {
    let mut b = builder();
    let s = b.add_settings(settings(PlanMode::Line, 20.0, 0.5, false)).unwrap();
    boundary(&mut b, s, 8, 0);
    b.push(PlanEvent::Travel { to: pt(390.0, 10.0) }).unwrap();
    b.push(PlanEvent::Cut { to: pt(410.0, 10.0), origin: None }).unwrap();
    b.finish().unwrap()
}
