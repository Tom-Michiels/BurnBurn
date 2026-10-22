//! G7-werk, W09: the measured red-dot offset of a machine is used where the story asks for it, the
//! frame and the position, and nowhere else. A job placed on the head position (or on the user
//! origin) is aimed with the red dot: its anchor lands where the dot is, which is the head plus the
//! offset. The cutting geometry is never changed, the plan says which offset it used, and without a
//! proof or with the switch off the beam position is used as before.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use burnburn_job::MoveKind;
use burnburn_machine::{FrameShape, JobAnchor, LiveRequest, Reply, Request, StartFrom};
use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, LaserCommand, LaserControlKind, LaserSetup, LaserViewModel, MachinePort,
    ProofInput, ProofKind, UiIntent,
};
use burnburn_test_support::Step;
use burnburn_types::DocumentId;

use common::laser::{Lab, all_supported, lab};
use common::*;

struct Win {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    lab: Lab,
}

/// One window with a 120 x 80 rectangle (10,10)-(130,90) and a cut operation, controller live features on.
fn win() -> Win {
    let lab = lab();
    lab.service.set_capabilities(all_supported());
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_machine(lab.service.clone());
    let doc = h.new_document().expect("window");
    design(&mut h, doc);
    Win { h, doc, lab }
}

fn view(w: &mut Win) -> LaserViewModel {
    w.h.controller().laser_view().expect("laser window")
}

fn settle(w: &Win) {
    for _ in 0..8 {
        w.lab.service.poll().unwrap();
    }
}

fn setup(w: &mut Win, setup: LaserSetup) {
    w.h.send(UiIntent::LaserSetup { setup });
}

fn head_at(w: &mut Win, x_mm: f64, y_mm: f64) {
    let (x_um, y_um) = ((x_mm * 1000.0) as i64, (y_mm * 1000.0) as i64);
    w.lab.peer.script(
        Step::expect(Request::Live(LiveRequest::ReadPosition))
            .reply(Reply::Ack)
            .reply(Reply::Position { x_um: Some(x_um), y_um: Some(y_um) }),
    );
    let vm = view(w);
    let control = vm.control(&LaserControlKind::ShowPosition).expect("show position");
    let command = LaserCommand::Action(burnburn_machine::LaserAction::ShowPosition);
    w.h.send(UiIntent::Laser { document: Some(w.doc), action: control.action, seen: vm.seen.clone(), command });
    settle(w);
}

fn burn_bounds(w: &Win) -> ((f64, f64), (f64, f64)) {
    let plan = w.h.document(w.doc).unwrap().plan.expect("a compiled plan");
    let mut points = Vec::new();
    for m in plan.preview.moves.iter().filter(|m| matches!(m.kind, MoveKind::Cut)) {
        points.push((m.from.x_mm(), m.from.y_mm()));
        points.push((m.to.x_mm(), m.to.y_mm()));
    }
    let min = points.iter().fold((f64::MAX, f64::MAX), |a, p| (a.0.min(p.0), a.1.min(p.1)));
    let max = points.iter().fold((f64::MIN, f64::MIN), |a, p| (a.0.max(p.0), a.1.max(p.1)));
    (min, max)
}

fn compile(w: &mut Win) {
    w.h.invoke(w.doc, CommandKey::Compile);
    w.h.run_tasks();
}

fn assert_close(got: (f64, f64), want: (f64, f64)) {
    assert!((got.0 - want.0).abs() < 1e-6 && (got.1 - want.1).abs() < 1e-6, "{got:?} != {want:?}");
}

fn record_pointer(w: &mut Win, x: &str, y: &str, date: &str) {
    w.h.send(UiIntent::RecordCalibration {
        input: ProofInput {
            kind: ProofKind::PointerOffset,
            material: String::new(),
            speed: String::new(),
            value: x.into(),
            value_y: y.into(),
            method: "kruis gebrand, rode stip erop gelegd".into(),
            date: date.into(),
        },
    });
    w.h.run_tasks();
}

fn on_head(w: &mut Win, x: f64, y: f64) {
    head_at(w, x, y);
    setup(w, LaserSetup::StartFrom(StartFrom::CurrentPosition));
    setup(w, LaserSetup::Anchor(JobAnchor::TopLeft));
}

#[test]
fn a_job_placed_on_the_head_is_aimed_with_the_red_dot_when_the_machine_has_a_pointer_offset() {
    let mut w = win();
    record_pointer(&mut w, "2", "-1", "2026-10-22");
    on_head(&mut w, 500.0, 400.0);
    compile(&mut w);

    // The dot sits 2 mm right of and 1 mm below the beam: the top-left corner (10, 90) lands on
    // the dot at (502, 399), not on the head.
    let (min, max) = burn_bounds(&w);
    assert_close(min, (502.0, 319.0));
    assert_close(max, (622.0, 399.0));
    let placement = view(&mut w).placement.expect("placement shown");
    assert_eq!(placement.pointer_offset_mm, Some((2.0, -1.0)), "the plan says which offset it used");
    assert_close(placement.delta_mm, (492.0, 309.0));
    assert_eq!(placement.from_position_mm, Some((500.0, 400.0)), "the head position itself is not shifted");
    assert!(!placement.setup_changed);
}

#[test]
fn the_offset_moves_the_anchor_and_never_the_cutting_geometry() {
    let mut w = win();
    on_head(&mut w, 500.0, 400.0);
    compile(&mut w);
    let (a_min, a_max) = burn_bounds(&w);
    record_pointer(&mut w, "2", "-1", "2026-10-22");
    compile(&mut w);
    let (b_min, b_max) = burn_bounds(&w);
    assert_close((a_max.0 - a_min.0, a_max.1 - a_min.1), (b_max.0 - b_min.0, b_max.1 - b_min.1));
    assert_close((b_min.0 - a_min.0, b_min.1 - a_min.1), (2.0, -1.0));
}

#[test]
fn with_the_switch_off_or_without_a_proof_the_beam_position_is_used() {
    let mut w = win();
    on_head(&mut w, 500.0, 400.0);
    compile(&mut w);
    assert_close(burn_bounds(&w).0, (500.0, 320.0));
    assert_eq!(view(&mut w).setup.pointer_offset_mm, None);
    assert_eq!(view(&mut w).placement.unwrap().pointer_offset_mm, None);

    record_pointer(&mut w, "2", "-1", "2026-10-22");
    assert_eq!(view(&mut w).setup.pointer_offset_mm, Some((2.0, -1.0)));
    assert!(view(&mut w).setup.use_pointer_offset, "a measured offset is used unless the user switches it off");
    setup(&mut w, LaserSetup::UsePointerOffset(false));
    compile(&mut w);
    assert_close(burn_bounds(&w).0, (500.0, 320.0));
    assert_eq!(view(&mut w).placement.unwrap().pointer_offset_mm, None);
}

#[test]
fn a_new_pointer_proof_or_the_switch_makes_the_compiled_plan_stale() {
    let mut w = win();
    record_pointer(&mut w, "2", "-1", "2026-10-22");
    on_head(&mut w, 500.0, 400.0);
    compile(&mut w);
    assert!(!view(&mut w).placement.unwrap().setup_changed);

    setup(&mut w, LaserSetup::UsePointerOffset(false));
    assert!(view(&mut w).placement.unwrap().setup_changed, "another aim is another plan");
    setup(&mut w, LaserSetup::UsePointerOffset(true));
    assert!(!view(&mut w).placement.unwrap().setup_changed);

    record_pointer(&mut w, "3", "-1", "2026-10-23");
    assert_eq!(view(&mut w).setup.pointer_offset_mm, Some((3.0, -1.0)), "the newest proof counts");
    assert!(view(&mut w).placement.unwrap().setup_changed);
    compile(&mut w);
    assert_close(burn_bounds(&w).0, (503.0, 319.0));
}

#[test]
fn a_job_placed_on_the_user_origin_is_aimed_with_the_dot_too() {
    let mut w = win();
    record_pointer(&mut w, "2", "-1", "2026-10-22");
    head_at(&mut w, 300.0, 200.0);
    setup(&mut w, LaserSetup::SetOriginHere);
    setup(&mut w, LaserSetup::StartFrom(StartFrom::UserOrigin));
    setup(&mut w, LaserSetup::Anchor(JobAnchor::TopLeft));
    compile(&mut w);
    assert_close(burn_bounds(&w).0, (302.0, 119.0));
    assert_eq!(view(&mut w).setup.user_origin_mm, Some((300.0, 200.0)), "the stored origin stays the beam position");
}

#[test]
fn absolute_coordinates_ignore_the_offset() {
    let mut w = win();
    record_pointer(&mut w, "2", "-1", "2026-10-22");
    compile(&mut w);
    assert_close(burn_bounds(&w).0, (10.0, 10.0));
}

/// A frame run of the reviewed plan in a window with the given pointer proof and switch; returns the
/// plan id of the frame the machine took.
fn frame_plan_id(proof: Option<(&str, &str)>, switch: bool) -> burnburn_job::JobPlanId {
    let mut w = win();
    if let Some((x, y)) = proof {
        record_pointer(&mut w, x, y, "2026-10-22");
    }
    setup(&mut w, LaserSetup::UsePointerOffset(switch));
    prepare(&mut w.h, w.doc);
    script_identity(&w.lab.peer, MODEL);
    for i in 0..8 {
        w.lab.peer.script(
            Step::expect_matching(format!("frame chunk {i}"), |r| matches!(r, Request::UploadChunk { .. }))
                .reply(Reply::Ack),
        );
    }
    let kind = LaserControlKind::Frame(FrameShape::BoundingRectangle);
    let vm = view(&mut w);
    let control = vm.control(&kind).expect("frame control");
    assert!(control.enabled, "{:?}", control.why_not);
    let command = LaserCommand::Frame(FrameShape::BoundingRectangle);
    w.h.send(UiIntent::Laser { document: Some(w.doc), action: control.action, seen: vm.seen.clone(), command });
    settle(&w);
    let status = w.lab.service.status();
    assert!(status.current_is_frame);
    status.current.unwrap().execution.plan
}

#[test]
fn the_frame_run_follows_the_job_with_the_dot_and_is_another_plan_than_the_beam_frame() {
    let beam = frame_plan_id(None, true);
    let beam_switched_off = frame_plan_id(Some(("2", "-1")), false);
    let dot = frame_plan_id(Some(("2", "-1")), true);
    let other_dot = frame_plan_id(Some(("3", "-1")), true);
    assert_eq!(beam, beam_switched_off, "no proof or the switch off: the plain frame");
    assert_ne!(dot, beam, "the dot frame is shifted by minus the offset");
    assert_ne!(dot, other_dot, "another measurement is another frame");
    assert_eq!(dot, frame_plan_id(Some(("2", "-1")), true), "deterministic");
}
