use std::time::Duration;

use burnburn_test_support::{FakeClock, PeerError, ScriptedPeer, SendOutcome, Step};
use burnburn_types::MonotonicTime;

#[derive(Debug, Clone, PartialEq)]
enum Req {
    Hello,
    Upload(u32),
    Start,
}

#[derive(Debug, Clone, PartialEq)]
enum Resp {
    Ack,
    Stored(u32),
    Started,
}

fn peer() -> (FakeClock, ScriptedPeer<Req, Resp>) {
    let clock = FakeClock::new();
    (clock.clone(), ScriptedPeer::new(clock))
}

#[test]
fn scripted_peer_answers_in_script_order_and_records_sends() {
    let (_clock, peer) = peer();
    peer.script(Step::expect(Req::Hello).reply(Resp::Ack)).script(Step::expect(Req::Upload(7)).reply(Resp::Stored(7)));

    peer.send(Req::Hello).unwrap();
    assert_eq!(peer.recv().unwrap(), Some(Resp::Ack));
    assert_eq!(peer.recv().unwrap(), None);
    peer.send(Req::Upload(7)).unwrap();
    assert_eq!(peer.recv().unwrap(), Some(Resp::Stored(7)));

    assert_eq!(peer.sent(), vec![Req::Hello, Req::Upload(7)]);
    assert!(peer.sent_log().iter().all(|r| r.outcome == SendOutcome::Accepted));
    peer.verify_complete().unwrap();
}

#[test]
fn scripted_peer_rejects_out_of_order_request() {
    let (_clock, peer) = peer();
    peer.script(Step::expect(Req::Hello).reply(Resp::Ack));

    let err = peer.send(Req::Start).unwrap_err();
    assert!(matches!(&err, PeerError::Unexpected { step: 0, got, .. } if got == "Start"), "{err:?}");
    assert_eq!(peer.sent_log()[0].outcome, SendOutcome::Rejected);
    // The rejected request did not consume the step.
    peer.send(Req::Hello).unwrap();
    assert!(matches!(peer.verify_complete(), Err(PeerError::Incomplete { rejected: 1, .. })));
}

#[test]
fn scripted_peer_rejects_requests_after_script_end() {
    let (_clock, peer) = peer();
    assert!(matches!(peer.send(Req::Start), Err(PeerError::Unscripted { .. })));
}

#[test]
fn delayed_reply_arrives_only_at_fake_time() {
    let (clock, peer) = peer();
    peer.script(Step::expect(Req::Start).reply_after(Duration::from_millis(500), Resp::Started));
    clock.advance(Duration::from_millis(100));
    peer.send(Req::Start).unwrap();

    assert_eq!(peer.next_reply_due(), Some(MonotonicTime::from_offset(Duration::from_millis(600))));
    clock.advance(Duration::from_millis(499));
    assert_eq!(peer.recv().unwrap(), None);
    clock.advance(Duration::from_millis(1));
    assert_eq!(peer.recv().unwrap(), Some(Resp::Started));
    assert_eq!(peer.next_reply_due(), None);
}

#[test]
fn replies_are_delivered_by_due_time_then_script_order() {
    let (clock, peer) = peer();
    peer.script(
        Step::expect(Req::Hello)
            .reply_after(Duration::from_millis(20), Resp::Stored(2))
            .reply(Resp::Ack)
            .reply_after(Duration::from_millis(20), Resp::Stored(3)),
    );
    peer.send(Req::Hello).unwrap();
    clock.advance(Duration::from_millis(20));
    let got: Vec<_> = std::iter::from_fn(|| peer.recv().unwrap()).collect();
    assert_eq!(got, vec![Resp::Ack, Resp::Stored(2), Resp::Stored(3)]);
}

#[test]
fn duplicated_reply_is_delivered_twice() {
    let (_clock, peer) = peer();
    peer.script(Step::expect(Req::Start).reply_duplicated(Resp::Started, 2));
    peer.send(Req::Start).unwrap();
    assert_eq!(peer.recv().unwrap(), Some(Resp::Started));
    assert_eq!(peer.recv().unwrap(), Some(Resp::Started));
    assert_eq!(peer.recv().unwrap(), None);
}

#[test]
fn lost_reply_never_arrives_but_is_recorded() {
    let (clock, peer) = peer();
    peer.script(Step::expect(Req::Start).lose_reply(Resp::Started));
    peer.send(Req::Start).unwrap();
    clock.advance(Duration::from_secs(60));
    assert_eq!(peer.recv().unwrap(), None);
    assert_eq!(peer.lost(), vec![Resp::Started]);
    peer.verify_complete().unwrap();
}

#[test]
fn disconnect_after_request_loses_its_reply_until_reconnect() {
    let (_clock, peer) = peer();
    peer.script(Step::expect(Req::Start).reply(Resp::Started).then_disconnect())
        .script(Step::expect(Req::Hello).reply(Resp::Ack));

    peer.send(Req::Start).unwrap();
    assert!(!peer.is_connected());
    assert_eq!(peer.recv(), Err(PeerError::Disconnected));
    assert_eq!(peer.send(Req::Hello), Err(PeerError::Disconnected));
    assert_eq!(peer.lost(), vec![Resp::Started]);

    peer.reconnect();
    peer.send(Req::Hello).unwrap();
    assert_eq!(peer.recv().unwrap(), Some(Resp::Ack));
    let outcomes: Vec<_> = peer.sent_log().iter().map(|r| r.outcome).collect();
    assert_eq!(outcomes, vec![SendOutcome::Accepted, SendOutcome::Disconnected, SendOutcome::Accepted]);
    // The start was sent exactly once: the evidence an actor test needs for "no second start".
    assert_eq!(peer.sent().iter().filter(|r| **r == Req::Start).count(), 1);
}

#[test]
fn manual_disconnect_discards_pending_delayed_replies() {
    let (clock, peer) = peer();
    peer.script(Step::expect(Req::Upload(1)).reply_after(Duration::from_secs(1), Resp::Stored(1)));
    peer.send(Req::Upload(1)).unwrap();
    peer.disconnect();
    peer.reconnect();
    clock.advance(Duration::from_secs(2));
    assert_eq!(peer.recv().unwrap(), None);
    assert_eq!(peer.lost(), vec![Resp::Stored(1)]);
}

#[test]
fn sends_are_timestamped_with_fake_time() {
    let (clock, peer) = peer();
    peer.script(Step::expect(Req::Hello)).script(Step::expect_matching("any upload", |r| matches!(r, Req::Upload(_))));
    peer.send(Req::Hello).unwrap();
    clock.advance(Duration::from_millis(30));
    peer.send(Req::Upload(99)).unwrap();
    let at: Vec<_> = peer.sent_log().iter().map(|r| r.at).collect();
    assert_eq!(at, vec![MonotonicTime::ZERO, MonotonicTime::from_offset(Duration::from_millis(30))]);
}

#[test]
fn peer_handle_can_move_to_another_thread() {
    let (_clock, peer) = peer();
    peer.script(Step::expect(Req::Hello).reply(Resp::Ack));
    let remote = peer.clone();
    std::thread::spawn(move || remote.send(Req::Hello)).join().unwrap().unwrap();
    assert_eq!(peer.recv().unwrap(), Some(Resp::Ack));
}
