//! W2-L3 / M08: a rotary profile compiles to a plan in surface millimetres that remembers the
//! rotary; the flat bed is unchanged and returning to it restores the plan it had.
#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::{Finding, compile, preflight};
use burnburn_job::{RotaryConfig, RotaryKind};
use burnburn_types::CancellationToken;
use common::*;

const D200: f64 = 200.0 / std::f64::consts::PI;

fn chuck() -> RotaryConfig {
    RotaryConfig::new(RotaryKind::Chuck { object_diameter_mm: D200 }, 8000, 20.0, false).unwrap()
}

#[test]
fn a_rotary_plan_stays_in_surface_millimetres_and_records_the_rotary() {
    let mut s = session();
    let g = rectangle(&mut s, 10.0, 20.0, 30.0, 40.0);
    add_op(&mut s, line_op("Snijden", 10.0, 50.0), vec![g]);
    let flat = plan_from_outside(&s, &profile()).unwrap();
    let on = profile().with_rotary(chuck()).unwrap();
    let turned = plan_from_outside(&s, &on).unwrap();
    assert_eq!(turned.rotary(), Some(&chuck()));
    assert_eq!(turned.events(), flat.events(), "the geometry is the same surface millimetres");
    assert_ne!(turned.id(), flat.id());
    assert!((turned.frame().height_mm() - 200.0).abs() < 1e-9, "the bed is one wrap");
    assert!(flat.rotary().is_none());
}

#[test]
fn returning_to_the_flat_bed_compiles_the_original_plan_again() {
    let mut s = session();
    let g = rectangle(&mut s, 10.0, 20.0, 30.0, 40.0);
    add_op(&mut s, line_op("Snijden", 10.0, 50.0), vec![g]);
    let flat = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    let back = profile().with_rotary(chuck()).unwrap().without_rotary();
    let again = compile(&s.snapshot(), &back, &options(), &CancellationToken::never()).unwrap();
    assert_eq!(again.id(), flat.id());
    assert_eq!(again.source().machine_profile, flat.source().machine_profile);
}

#[test]
fn a_shape_longer_than_one_wrap_is_blocked_by_preflight() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 30.0, 250.0);
    add_op(&mut s, line_op("Snijden", 10.0, 50.0), vec![g]);
    let on = profile().with_rotary(chuck()).unwrap();
    let plan = compile(&s.snapshot(), &on, &options(), &CancellationToken::never()).unwrap();
    let report = preflight(&s.snapshot(), &plan, &on);
    assert!(report.findings.iter().any(|f| matches!(f, Finding::OutsideBed { .. })), "{:?}", report.findings);
    // On the flat bed (900 mm high) the same shape is fine.
    let flat = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    assert!(
        !preflight(&s.snapshot(), &flat, &profile()).findings.iter().any(|f| matches!(f, Finding::OutsideBed { .. }))
    );
}
