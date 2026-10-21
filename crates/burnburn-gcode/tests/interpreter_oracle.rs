//! Sanity of the independent G-code interpreter itself (hand-computed programs), so the oracle
//! used by the lowering tests is itself pinned to analytical values.
#![allow(clippy::unwrap_used)]

mod support;
use support::interp::run;

#[test]
fn interpreter_follows_hand_computed_programs() {
    // 10 mm burn at S500 (of 1000) and 600 mm/min = 10 mm/s: 1 s at 50 % = 0.5 ratio-seconds.
    let r = run("G21\nG90\nG0 X0 Y0\nM4 S0\nG1 X10 S500 F600\nM5\nM2\n").unwrap();
    assert_eq!(r.tool_path_length(), 10.0);
    assert_eq!(r.laser_on_length(), 10.0);
    assert!((r.energy_ratio_seconds(1000.0) - 0.5).abs() < 1e-12);
    assert!(r.ended && !r.laser_on_at_end);
}

#[test]
fn interpreter_keeps_the_laser_off_for_rapids_and_s0() {
    let r = run("G21 G90 F600 M4\nG0 X5 S900\nG1 X10 S0\nG1 X20 S100\n").unwrap();
    assert_eq!(r.laser_on_length(), 10.0);
    assert!(r.segments[0].rapid && !r.segments[0].laser_on);
    assert!(!r.segments[1].laser_on);
}

#[test]
fn interpreter_rejects_unknown_words_and_inch_mode() {
    assert!(run("G20\n").is_err());
    assert!(run("G21\nG90\nG2 X1 Y1 I1\n").is_err());
    assert!(run("G21\nG90\nG1 X1 F100\nM2\nG0 X0\n").is_err());
    assert!(run("G1 X1\n").is_err());
}
