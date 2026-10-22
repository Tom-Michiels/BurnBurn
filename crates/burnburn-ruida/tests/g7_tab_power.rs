//! Tab Cut Power has no proven RD form (C04, G7-cam, `docs/hardware/evidence-cam.md`): a plan with
//! a cut at its own power is refused by name, never written as a full-power or skipped tab.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_job::{
    JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanEvent, PlanMode, PlanSource,
    PowerRatio, ProfileRevision,
};
use burnburn_ruida::{LowerError, Magic, OverscanPolicy, RuidaProfile, lower};
use burnburn_types::{ContentHash, OperationId, RevisionId, StableId};

const RUIDA: RuidaProfile = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };

fn pt(x: f64, y: f64) -> MachinePoint {
    MachinePoint::new(x, y).unwrap()
}

fn plan_with_tab() -> burnburn_job::JobPlan {
    let source = PlanSource {
        document_revision: RevisionId::from_raw(1),
        source_hash: ContentHash::from_bytes([1; 32]),
        machine_profile: ProfileRevision { id: "bed".into(), revision: 1, content: ContentHash::from_bytes([2; 32]) },
        compiler_version: "test".into(),
    };
    let mut b =
        JobPlanBuilder::new(source, MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(), pt(0.0, 0.0));
    let p = PowerRatio::new(0.5).unwrap();
    let s = b
        .add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s: 100.0,
            min_power: p,
            max_power: p,
            air_assist: false,
        })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Travel { to: pt(10.0, 10.0) }).unwrap();
    b.push(PlanEvent::Cut { to: pt(50.0, 10.0), origin: None }).unwrap();
    b.push(PlanEvent::CutPower { to: pt(52.0, 10.0), power: PowerRatio::new(0.1).unwrap(), origin: None }).unwrap();
    b.finish().unwrap()
}

#[test]
fn rd_refuses_a_cut_at_its_own_power_by_name() {
    match lower(&plan_with_tab(), &RUIDA) {
        Err(LowerError::Unsupported { what, event }) => {
            assert_eq!(what, "tab cut power");
            assert_eq!(event, 3, "the refusal points at the event");
        }
        other => panic!("expected the named refusal, got {other:?}"),
    }
}
