//! FE01 (G5-cam): a plan with feeder events is a real plan (preview, hash, placement) but the
//! Ruida backend has no proven form for the feeder and says so; it never writes the job without it.

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use burnburn_cam::{FeederSettings, plan_feeder};
use burnburn_job::PlanEvent;
use burnburn_ruida::{LowerError, lower};
use burnburn_types::{CancellationToken, Length};
use common::*;

fn fed_plan(repeat: u32, before: f64, after: f64, pause: f64) -> burnburn_job::JobPlan {
    let mut s = new_session();
    let rect = burnburn_geometry::Rect::new(burnburn_geometry::Point::new(10.0, 10.0), mm(20.0), mm(20.0)).unwrap();
    let g = apply(&mut s, burnburn_document::CommandPayload::CreateRectangle { rect, name: String::new() }, &[])
        .changes
        .created[0];
    add_cut(&mut s, vec![g]);
    let plan = compile_plan(&s.snapshot());
    let settings =
        FeederSettings::new(repeat, Length::from_mm(before).unwrap(), Length::from_mm(after).unwrap(), pause, 50.0)
            .unwrap();
    plan_feeder(&plan, &settings, &CancellationToken::never()).unwrap()
}

#[test]
fn the_ruida_backend_refuses_every_feeder_plan_by_name() {
    for plan in [fed_plan(1, 25.0, 0.0, 0.0), fed_plan(1, 0.0, 40.0, 0.0), fed_plan(3, 0.0, 0.0, 5.0)] {
        let err = lower(&plan, &RUIDA).unwrap_err();
        assert!(matches!(err, LowerError::Unsupported { what: "feeder", .. }), "{err:?}");
    }
}

#[test]
fn a_plan_without_feeder_events_still_lowers_and_a_repeat_of_one_changes_nothing() {
    let plain = fed_plan(1, 0.0, 0.0, 0.0);
    assert!(!plain.events().iter().any(|e| matches!(e, PlanEvent::Feed { .. } | PlanEvent::Pause { .. })));
    assert!(lower(&plain, &RUIDA).is_ok());
}
