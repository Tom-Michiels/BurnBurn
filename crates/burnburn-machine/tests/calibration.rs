//! Calibration set, setup revision and calibration test patterns (IM29, W09).
#![allow(clippy::unwrap_used)]

use burnburn_job::{JobPlan, MachineFrame, MachineProfile, OriginCorner, PlanEvent, PlanMode, PowerRatio};
use burnburn_machine::calibration::{
    KerfTest, PatternLaser, PointerOffsetTest, ScanOffsetTest, kerf_test_plan, pointer_offset_plan, scan_offset_plan,
};
use burnburn_machine::{CalibratedProfile, CalibrationError, CalibrationSet, MaterialKey, ScanOffsetPoint};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile, lower};

fn profile() -> MachineProfile {
    MachineProfile::new(
        "bench",
        1,
        MachineFrame::new(600.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn laser() -> PatternLaser {
    PatternLaser { speed_mm_s: 20.0, power: PowerRatio::new(0.4).unwrap() }
}

const RUIDA: RuidaProfile = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::Reject };

/// Laser-on segments as ((x0, y0), (x1, y1)).
fn cuts(plan: &JobPlan) -> Vec<((f64, f64), (f64, f64))> {
    let mut at = (plan.start().x_mm(), plan.start().y_mm());
    let mut out = Vec::new();
    for e in plan.events() {
        match *e {
            PlanEvent::Travel { to } => at = (to.x_mm(), to.y_mm()),
            PlanEvent::Cut { to, .. } => {
                out.push((at, (to.x_mm(), to.y_mm())));
                at = (to.x_mm(), to.y_mm());
            }
            PlanEvent::ScanRun { line, .. } => {
                out.push(((line.from.x_mm(), line.from.y_mm()), (line.to.x_mm(), line.to.y_mm())));
                at = (line.to.x_mm(), line.to.y_mm());
            }
            _ => {}
        }
    }
    out
}

#[test]
fn kerf_pattern_cuts_closed_squares_of_nominal_size() {
    let test = KerfTest { origin: [10.0, 20.0], size_mm: 25.0, count: 3, gap_mm: 5.0, laser: laser() };
    let plan = kerf_test_plan(&profile(), &test).unwrap();
    let segments = cuts(&plan);
    assert_eq!(segments.len(), 12, "4 sides × 3 squares");
    for (i, square) in segments.chunks(4).enumerate() {
        let x0 = 10.0 + i as f64 * 30.0;
        assert_eq!(square[0].0, (x0, 20.0), "square {i} starts at its corner");
        assert_eq!(square[3].1, square[0].0, "square {i} is closed");
        for &((ax, ay), (bx, by)) in square {
            assert_eq!((bx - ax).abs() + (by - ay).abs(), 25.0, "axis-aligned side of nominal length");
        }
    }
    assert_eq!(plan.settings().len(), 1);
    assert_eq!((plan.settings()[0].speed_mm_s, plan.settings()[0].max_power.get()), (20.0, 0.4));
}

#[test]
fn scan_offset_pattern_has_one_block_per_speed_with_alternating_rows() {
    let test = ScanOffsetTest {
        origin: [50.0, 50.0],
        speeds_mm_s: vec![100.0, 200.0, 400.0],
        power: PowerRatio::new(0.2).unwrap(),
        length_mm: 30.0,
        rows: 4,
        interval_mm: 0.5,
        gap_mm: 5.0,
    };
    let plan = scan_offset_plan(&profile(), &test).unwrap();
    let speeds: Vec<f64> = plan.settings().iter().map(|s| s.speed_mm_s).collect();
    assert_eq!(speeds, vec![100.0, 200.0, 400.0]);
    assert!(plan.settings().iter().all(|s| matches!(s.mode, PlanMode::Scan { bidirectional: true, .. })));
    let rows = cuts(&plan);
    assert_eq!(rows.len(), 12);
    for (i, &((x0, _), (x1, _))) in rows.iter().enumerate() {
        let (from, to) = if i % 2 == 0 { (50.0, 80.0) } else { (80.0, 50.0) };
        assert_eq!((x0, x1), (from, to), "row {i}: even rows go +x, odd rows return");
    }
    // Blocks are separated by the gap: first row of block 2 at 50 + 4 × 0.5 + 5.
    assert_eq!(rows[4].0.1, 57.0);
}

#[test]
fn pointer_offset_pattern_crosses_at_the_given_point() {
    let test = PointerOffsetTest { at: [100.0, 120.0], arm_mm: 10.0, laser: laser() };
    let plan = pointer_offset_plan(&profile(), &test).unwrap();
    assert_eq!(cuts(&plan), vec![((90.0, 120.0), (110.0, 120.0)), ((100.0, 110.0), (100.0, 130.0))]);
}

#[test]
fn calibration_patterns_stay_on_the_bed_and_lower_to_rd() {
    let off = KerfTest { origin: [590.0, 20.0], size_mm: 25.0, count: 1, gap_mm: 5.0, laser: laser() };
    assert!(matches!(kerf_test_plan(&profile(), &off), Err(CalibrationError::OutsideBed { .. })));
    let p = PointerOffsetTest { at: [5.0, 5.0], arm_mm: 10.0, laser: laser() };
    assert!(matches!(pointer_offset_plan(&profile(), &p), Err(CalibrationError::OutsideBed { .. })));

    let ok = KerfTest { origin: [10.0, 20.0], size_mm: 25.0, count: 2, gap_mm: 5.0, laser: laser() };
    let plan = kerf_test_plan(&profile(), &ok).unwrap();
    assert_eq!(plan.source().machine_profile, *profile().revision());
    lower(&plan, &RUIDA).unwrap();
}

#[test]
fn every_calibration_value_is_part_of_the_setup_revision() {
    let base = CalibratedProfile::new(profile(), CalibrationSet::default()).unwrap();
    assert_ne!(base.revision().content, profile().revision().content, "setup ≠ bare profile");
    let same = CalibratedProfile::new(profile(), CalibrationSet::default()).unwrap();
    assert_eq!(base.revision(), same.revision());

    let mut kerf = CalibrationSet::default();
    kerf.kerf_mm.insert(MaterialKey("plywood-3mm".into()), 0.12);
    let mut scan = CalibrationSet::default();
    scan.scan_offset.push(ScanOffsetPoint { speed_mm_s: 200.0, offset_mm: 0.08 });
    let pointer = CalibrationSet { pointer_offset_mm: Some([12.5, -3.0]), ..CalibrationSet::default() };
    let revisions: Vec<_> =
        [kerf, scan, pointer].into_iter().map(|c| base.with_calibration(c).unwrap().revision().content).collect();
    assert!(revisions.iter().all(|r| *r != base.revision().content));
    assert!(revisions[0] != revisions[1] && revisions[1] != revisions[2] && revisions[0] != revisions[2]);

    let bad = CalibrationSet {
        scan_offset: vec![
            ScanOffsetPoint { speed_mm_s: 200.0, offset_mm: 0.1 },
            ScanOffsetPoint { speed_mm_s: 200.0, offset_mm: 0.2 },
        ],
        ..CalibrationSet::default()
    };
    assert_eq!(base.with_calibration(bad), Err(CalibrationError::DuplicateSpeed(200.0)));
    let mut negative = CalibrationSet::default();
    negative.kerf_mm.insert(MaterialKey("x".into()), -0.1);
    assert!(matches!(base.with_calibration(negative), Err(CalibrationError::OutOfRange(_))));
}
