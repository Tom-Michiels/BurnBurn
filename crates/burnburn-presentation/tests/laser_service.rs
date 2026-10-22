//! W2-L1-laser, service level: laser-window actions through the machine service over the real
//! execution actor, a scripted fake controller and a fake clock. Intent first in the journal, then
//! the wire; a refused action leaves no record and no traffic; unknown is never zero.
//! No socket, no real laser.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use std::sync::atomic::Ordering;

use burnburn_machine::{
    Capabilities, ControllerFileName, FileEntry, HomeTarget, JogKey, LaserAction, LaserRefusal, LiveError, LiveFeature,
    LiveOutcome, LiveRequest, MachineId, OriginTarget, Reply, Request, UserOrigin,
};
use burnburn_presentation::{
    ActionSubject, Dispatched, LaserCall, LaserIntent, LiveKind, MachineAction, MachineError, MachinePort,
};
use burnburn_test_support::Step;

use common::laser::{Lab, all_supported, frame, lab};
use common::*;

fn intent_for(l: &Lab, action: LaserAction, user_origin: Option<UserOrigin>) -> LaserIntent {
    let status = l.service.status();
    LaserIntent {
        action: status.action_id(MachineAction::Live(LiveKind::of(&action)), ActionSubject::Machine),
        machine: MachineId::new(MACHINE),
        call: LaserCall::Action(action),
        frame: frame(),
        user_origin,
    }
}

fn act(l: &Lab, action: LaserAction) -> Result<Dispatched, MachineError> {
    l.service.laser(intent_for(l, action, None))
}

fn live_requests(peer: &Peer) -> Vec<LiveRequest> {
    peer.sent()
        .into_iter()
        .filter_map(|r| match r {
            Request::Live(l) => Some(l),
            _ => None,
        })
        .collect()
}

fn settle(l: &Lab) {
    for _ in 0..8 {
        l.service.poll().unwrap();
    }
}

#[test]
fn a_refused_action_leaves_no_journal_record_and_no_wire_traffic() {
    let l = lab(); // every capability is unverified
    let result = act(&l, LaserAction::Home(HomeTarget::Xy));
    let Err(MachineError::Live(LiveError::NotVerified { feature, needs, .. })) = result else {
        panic!("expected a typed 'not verified' refusal, got {result:?}")
    };
    assert_eq!(feature, LiveFeature::Home);
    assert!(needs.contains("HOME_XY"), "the refusal names the missing evidence: {needs}");
    assert!(l.history.intents.lock().unwrap().is_empty(), "nothing journaled");
    assert!(l.peer.sent().is_empty(), "nothing sent");
}

#[test]
fn the_intent_is_journaled_before_the_request_reaches_the_controller() {
    let l = lab();
    l.service.set_capabilities(all_supported());
    l.peer.script(Step::expect(Request::Live(LiveRequest::Home(HomeTarget::Xy))).reply(Reply::Ack));

    assert!(matches!(act(&l, LaserAction::Home(HomeTarget::Xy)), Ok(Dispatched::Sent(None))));
    settle(&l);

    let intents = l.history.intents.lock().unwrap();
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].0.request, LiveRequest::Home(HomeTarget::Xy));
    assert_eq!(intents[0].1, 0, "the controller had seen nothing when the intent was recorded");
    assert_eq!(live_requests(&l.peer), vec![LiveRequest::Home(HomeTarget::Xy)]);
    let outcomes = l.history.outcomes.lock().unwrap();
    assert_eq!(outcomes.len(), 1, "the proven outcome is journaled once");
    assert_eq!(outcomes[0].action, intents[0].0.action);
    assert_eq!(outcomes[0].outcome, LiveOutcome::Acked, "an ACK is receipt, not 'home'");
}

#[test]
fn the_same_action_id_twice_sends_one_request() {
    let l = lab();
    l.service.set_capabilities(all_supported());
    l.peer.script(Step::expect(Request::Live(LiveRequest::Home(HomeTarget::Xy))).reply(Reply::Ack));
    let intent = intent_for(&l, LaserAction::Home(HomeTarget::Xy), None);

    assert!(matches!(l.service.laser(intent.clone()), Ok(Dispatched::Sent(_))));
    settle(&l);
    assert!(matches!(l.service.laser(intent), Ok(Dispatched::AlreadyHandled(_))));
    settle(&l);

    assert_eq!(live_requests(&l.peer).len(), 1);
    assert_eq!(l.history.intents.lock().unwrap().len(), 1);
}

#[test]
fn a_failing_journal_stops_the_action_before_the_wire() {
    let l = lab();
    l.service.set_capabilities(all_supported());
    l.history.fail.store(true, Ordering::SeqCst);

    assert!(matches!(act(&l, LaserAction::Home(HomeTarget::Xy)), Err(MachineError::NotRecorded(_))));
    assert!(l.peer.sent().is_empty(), "no record, no request");

    // The same click may be tried again once the journal works.
    l.history.fail.store(false, Ordering::SeqCst);
    l.peer.script(Step::expect(Request::Live(LiveRequest::Home(HomeTarget::Xy))).reply(Reply::Ack));
    assert!(act(&l, LaserAction::Home(HomeTarget::Xy)).is_ok());
}

#[test]
fn the_position_is_unknown_not_zero_until_the_controller_answers() {
    let l = lab();
    l.service.set_capabilities(all_supported());
    let status = l.service.status();
    assert_eq!(status.live.position.x.mm(), None);
    assert_eq!(status.live.position.y.mm(), None);

    l.peer.script(
        Step::expect(Request::Live(LiveRequest::ReadPosition))
            .reply(Reply::Ack)
            .reply(Reply::Position { x_um: Some(12_500), y_um: Some(40_000) }),
    );
    act(&l, LaserAction::ShowPosition).unwrap();
    settle(&l);
    let position = l.service.status().live.position;
    assert_eq!((position.x.mm(), position.y.mm()), (Some(12.5), Some(40.0)));

    // Any motion command makes it unknown again until it is read.
    l.peer.script(Step::expect(Request::Live(LiveRequest::Home(HomeTarget::Xy))).reply(Reply::Ack));
    act(&l, LaserAction::Home(HomeTarget::Xy)).unwrap();
    settle(&l);
    let position = l.service.status().live.position;
    assert_eq!((position.x.mm(), position.y.mm()), (None, None));
}

#[test]
fn a_jog_release_is_never_blocked_and_sends_nothing_when_no_key_is_held() {
    let l = lab();
    // Nothing held: the key-up of a refused press must not become a stray stop.
    let released = act(&l, LaserAction::JogRelease(JogKey::XLeft));
    assert!(matches!(released, Ok(Dispatched::Sent(None))));
    assert!(l.peer.sent().is_empty());
    assert!(l.history.intents.lock().unwrap().is_empty());

    l.service.set_capabilities(all_supported());
    l.peer.script(Step::expect(Request::Live(LiveRequest::JogStart(JogKey::XLeft))).reply(Reply::Ack));
    act(&l, LaserAction::JogPress(JogKey::XLeft)).unwrap();
    settle(&l);
    assert_eq!(l.service.status().live.jog_held, Some(JogKey::XLeft));

    // The capability is withdrawn while the key is held; the release still goes out.
    l.service.set_capabilities(Capabilities::default());
    l.peer.script(Step::expect(Request::Live(LiveRequest::JogStop(JogKey::XLeft))).reply(Reply::Ack));
    act(&l, LaserAction::JogRelease(JogKey::XLeft)).unwrap();
    settle(&l);
    assert_eq!(l.service.status().live.jog_held, None);
    assert_eq!(live_requests(&l.peer), vec![LiveRequest::JogStart(JogKey::XLeft), LiveRequest::JogStop(JogKey::XLeft)]);
}

#[test]
fn the_gates_follow_the_capabilities_and_the_state_of_the_machine() {
    let l = lab();
    let status = l.service.status();
    for feature in [LiveFeature::Jog, LiveFeature::Home, LiveFeature::MoveAbsolute, LiveFeature::FileList] {
        assert!(
            matches!(status.live.gate(feature), Some(LiveError::NotVerified { .. })),
            "{feature:?} is unverified by default"
        );
    }
    l.service.set_capabilities(all_supported());
    let status = l.service.status();
    for feature in [LiveFeature::Jog, LiveFeature::Home, LiveFeature::MoveAbsolute, LiveFeature::FileRun] {
        assert_eq!(status.live.gate(feature), None, "{feature:?} would be accepted");
    }
}

#[test]
fn numeric_moves_outside_the_bed_and_to_an_unset_user_origin_are_refused_before_the_journal() {
    let l = lab();
    l.service.set_capabilities(all_supported());
    let off = act(&l, LaserAction::MoveTo { x_um: 2_000_000, y_um: 0 });
    assert!(matches!(off, Err(MachineError::Laser(LaserRefusal::OutsideBed { .. }))));
    let origin = act(&l, LaserAction::GoToOrigin(OriginTarget::User));
    assert!(matches!(origin, Err(MachineError::Laser(LaserRefusal::UserOriginNotSet))));
    let step = act(&l, LaserAction::JogStep { dx_view_um: 1000, dy_view_um: 0 });
    assert!(matches!(step, Err(MachineError::Laser(LaserRefusal::PositionUnknown))));
    assert!(l.peer.sent().is_empty());
    assert!(l.history.intents.lock().unwrap().is_empty());
}

#[test]
fn a_file_list_reply_is_kept_and_a_missing_size_stays_unknown() {
    let l = lab();
    l.service.set_capabilities(all_supported());
    let files = vec![
        FileEntry { name: ControllerFileName::new("PLAAT1").unwrap(), size_bytes: Some(2048) },
        FileEntry { name: ControllerFileName::new("PLAAT2").unwrap(), size_bytes: None },
    ];
    l.peer.script(
        Step::expect(Request::Live(LiveRequest::ListFiles)).reply(Reply::Ack).reply(Reply::Files(files.clone())),
    );
    act(&l, LaserAction::ListFiles).unwrap();
    settle(&l);
    assert_eq!(l.service.status().live.files, Some(files));
}

#[test]
fn running_a_controller_file_is_its_own_request_and_creates_no_execution() {
    let l = lab();
    l.service.set_capabilities(all_supported());
    let name = ControllerFileName::new("PLAAT1").unwrap();
    l.peer.script(Step::expect(Request::Live(LiveRequest::RunFile(name.clone()))).reply(Reply::Ack));
    act(&l, LaserAction::RunFile(name.clone())).unwrap();
    settle(&l);
    assert_eq!(live_requests(&l.peer), vec![LiveRequest::RunFile(name)]);
    assert!(l.service.status().current.is_none(), "no execution, no job start");
    assert!(!l.peer.sent().contains(&Request::Start));
}

/// A quick tap: the key-up arrives while the key-down's reply is still on its way. The release is
/// not lost (the head must never keep moving): it goes out as soon as the controller has answered.
#[test]
fn a_release_that_arrives_before_the_press_was_answered_is_sent_right_after_the_answer() {
    let l = lab();
    l.service.set_capabilities(all_supported());
    l.peer.script(
        Step::expect(Request::Live(LiveRequest::JogStart(JogKey::XLeft)))
            .reply_after(std::time::Duration::from_millis(100), Reply::Ack),
    );
    l.peer.script(Step::expect(Request::Live(LiveRequest::JogStop(JogKey::XLeft))).reply(Reply::Ack));
    act(&l, LaserAction::JogPress(JogKey::XLeft)).unwrap();

    // Key-up at once: the press is unanswered, so the stop cannot go out yet.
    assert!(act(&l, LaserAction::JogRelease(JogKey::XLeft)).is_ok());
    assert_eq!(live_requests(&l.peer), vec![LiveRequest::JogStart(JogKey::XLeft)]);
    assert_eq!(l.service.status().live.jog_held, Some(JogKey::XLeft));

    l.clock.advance(std::time::Duration::from_millis(150));
    settle(&l);
    settle(&l);
    assert_eq!(
        live_requests(&l.peer),
        vec![LiveRequest::JogStart(JogKey::XLeft), LiveRequest::JogStop(JogKey::XLeft)],
        "the queued release followed the answer"
    );
    assert_eq!(l.service.status().live.jog_held, None);
    assert_eq!(l.history.intents.lock().unwrap().len(), 2, "the release is journaled like any action");
    let outcomes = l.history.outcomes.lock().unwrap();
    assert_eq!(outcomes.len(), 2);
}
