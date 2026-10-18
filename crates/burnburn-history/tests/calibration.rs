//! A calibration change makes jobs prepared for the old setup stale (IM29, W09).
#![allow(clippy::unwrap_used)]

mod common;

use burnburn_history::HistoryError;
use burnburn_machine::{CalibratedProfile, CalibrationSet, MachineId, ScanOffsetPoint};
use common::*;

#[test]
fn scan_calibration_update_invalidates_dependent_prepared_plan() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let mut cal = CalibrationSet::default();
    cal.scan_offset.push(ScanOffsetPoint { speed_mm_s: 200.0, offset_mm: 0.05 });
    let setup = CalibratedProfile::new(profile(1), cal.clone()).unwrap();
    let c = compile_session(&plate(), setup.profile());
    let job = h
        .record_compiled(
            &fs,
            burnburn_history::CompileRecord {
                plan: &c.plan,
                rd_file: &c.rd,
                project: &c.project,
                project_path: None,
                preflight: c.preflight.clone(),
                profile: setup.revision(),
            },
            t(0),
        )
        .unwrap();
    let machine = MachineId::new("ruida-bench-1");
    assert!(setup.is_current_for(&h.job(job).unwrap().profile));

    // The operator measures a new scan offset: new setup revision.
    cal.scan_offset[0].offset_mm = 0.07;
    let recalibrated = setup.with_calibration(cal).unwrap();
    assert_eq!(recalibrated.profile(), setup.profile(), "the machine profile itself is unchanged");
    assert!(!recalibrated.is_current_for(&h.job(job).unwrap().profile), "prepared job is stale");
    let refused = h.create_execution(&fs, job, machine.clone(), None, recalibrated.revision(), t(1));
    assert!(matches!(refused, Err(HistoryError::ProfileChanged { .. })), "{refused:?}");
    assert_eq!(h.executions().count(), 0);

    // The old snapshot stays explicit and usable for its own setup (W09: "houdt hun expliciete
    // oude snapshot vast").
    h.create_execution(&fs, job, machine, None, setup.revision(), t(2)).unwrap();
}
