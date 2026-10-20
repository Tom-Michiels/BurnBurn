//! W2-L3: a focus (Z) offset per operation, gated by the same capability as the Z step, and the
//! focus returning to the start value for the operation that follows.
#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::CompileError;
use burnburn_document::process::{AdvancedOptions, Passes, ProcessCommand, ZOffset, ZStep};
use burnburn_document::{CommandPayload, DocumentSession};
use burnburn_job::{Feature, JobPlan, PlanEvent, Support};
use burnburn_types::OperationId;
use common::*;

fn advanced(s: &mut DocumentSession, operation: OperationId, advanced: AdvancedOptions) {
    run(s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation, advanced }));
}

/// `pass N` / `z V` marks in event order.
fn marks(plan: &JobPlan) -> Vec<String> {
    plan.events()
        .iter()
        .filter_map(|e| match e {
            PlanEvent::OperationBoundary { operation, pass, .. } => {
                Some(format!("op {} pass {pass}", operation.raw_value()))
            }
            PlanEvent::FocusOffset { z_mm } => Some(format!("z {z_mm}")),
            _ => None,
        })
        .collect()
}

trait Raw {
    fn raw_value(&self) -> u64;
}
impl Raw for OperationId {
    fn raw_value(&self) -> u64 {
        burnburn_types::StableId::raw(*self)
    }
}

fn offset(mm_: f64) -> Option<ZOffset> {
    Some(ZOffset::new(mm(mm_)).unwrap())
}

#[test]
fn a_z_offset_is_validated_and_needs_the_z_capability() {
    assert!(ZOffset::new(mm(0.0)).is_err(), "no offset is None, not zero");
    assert!(ZOffset::new(mm(50.1)).is_err());
    assert!(ZOffset::new(mm(-50.0)).is_ok());

    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let op = add_op(&mut s, line_op("Snijden", 10.0, 50.0), vec![g]);
    advanced(&mut s, op, AdvancedOptions { z_offset: offset(1.5), ..Default::default() });
    let err = plan_from_outside(&s, &profile()).unwrap_err();
    assert_eq!(
        err,
        CompileError::UnsupportedFeature { operation: op, feature: Feature::ZStep, support: Support::Unsupported }
    );
}

#[test]
fn the_offset_moves_the_focus_before_the_first_motion_and_adds_to_the_step() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let mut thrice = line_op("Snijden", 10.0, 50.0);
    thrice.settings.passes = Passes::new(3).unwrap();
    let op = add_op(&mut s, thrice, vec![g]);
    advanced(
        &mut s,
        op,
        AdvancedOptions { z_offset: offset(1.0), z_step: Some(ZStep::new(mm(0.5)).unwrap()), ..Default::default() },
    );
    let plan = plan_from_outside(&s, &supporting(&[Feature::ZStep])).unwrap();
    let id = burnburn_types::StableId::raw(op);
    assert_eq!(
        marks(&plan),
        vec![
            format!("op {id} pass 0"),
            "z 1".into(),
            format!("op {id} pass 1"),
            "z 1.5".into(),
            format!("op {id} pass 2"),
            "z 2".into()
        ]
    );
    // The focus is set before the first travel or cut of the operation.
    let first_motion =
        plan.events().iter().position(|e| matches!(e, PlanEvent::Travel { .. } | PlanEvent::Cut { .. })).unwrap();
    let first_focus = plan.events().iter().position(|e| matches!(e, PlanEvent::FocusOffset { .. })).unwrap();
    assert!(first_focus < first_motion);
}

#[test]
fn the_next_operation_without_a_focus_change_returns_to_the_start_focus() {
    let mut s = session();
    let a = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let b = rectangle(&mut s, 100.0, 0.0, 10.0, 10.0);
    let first = add_op(&mut s, line_op("Met focus", 10.0, 50.0), vec![a]);
    let second = add_op(&mut s, line_op("Zonder focus", 10.0, 50.0), vec![b]);
    advanced(&mut s, first, AdvancedOptions { z_offset: offset(-2.0), ..Default::default() });
    let plan = plan_from_outside(&s, &supporting(&[Feature::ZStep])).unwrap();
    let (f, g) = (burnburn_types::StableId::raw(first), burnburn_types::StableId::raw(second));
    assert_eq!(marks(&plan), vec![format!("op {f} pass 0"), "z -2".into(), format!("op {g} pass 0"), "z 0".into()]);
}
