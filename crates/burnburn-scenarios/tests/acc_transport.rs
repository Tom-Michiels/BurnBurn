//! Acceptance plan, "Transport/emulator" section: fault injection against the fake controller and
//! the real execution actor, driven through the real app controller (job card, machine panel).
//!
//! Every case ends with the same questions, asked of the view model the operator sees and of the
//! controller's request log: did the app call something "stored", "running" or "completed" that it
//! has no evidence for, did it offer a start, did it resend a non-idempotent request? The lower
//! layers have their own tests (`burnburn-machine/tests/execution.rs`, `burnburn-runtime/tests/udp.rs`,
//! `burnburn-ruida/tests/udp.rs`); this file is the end-to-end proof per fault of the plan.
//!
//! No real laser, socket or wall clock: `ScriptedPeer` + `FakeClock`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod acc_machine;

use std::sync::Arc;
use std::time::Duration;

use acc_machine::*;
use burnburn_machine::{
    Capabilities, Capability, ControllerFileName, ExecutionActor, FileCapabilities, LiveCapabilities, LiveError,
    LiveFeature, MachineIdentity, Policy, RdArtifact, Reply, Request,
};
use burnburn_presentation::{JobCardViewModel, MachineAction, MachineStatusKey};
use burnburn_ruida::udp::{UdpError, checksum, frame, parse_frame, reply_from_wire};
use burnburn_ruida::{Magic, ReplyByte, scramble};
use burnburn_test_support::Step;
use proptest::prelude::*;

fn chunk_is(i: usize) -> Step<Request, Reply> {
    Step::expect_matching(format!("chunk {i}"), move |r| matches!(r, Request::UploadChunk { index, .. } if *index == i))
}

fn status(r: &Rig) -> MachineStatusKey {
    r.h.machine_view().expect("machine panel").status
}

fn chunks_sent(r: &Rig, index: usize) -> usize {
    r.peer.sent().iter().filter(|q| matches!(q, Request::UploadChunk { index: i, .. } if *i == index)).count()
}

fn press_upload(r: &mut Rig, job: &JobCardViewModel) {
    r.h.press(Some(r.doc), job.upload.as_ref().expect("upload control"));
    pump(r);
}

fn press(r: &mut Rig, kind: MachineAction) {
    let button = r.h.machine_view().expect("machine panel").button(kind).cloned();
    let button = button.unwrap_or_else(|| panic!("no {kind:?} control"));
    assert!(button.enabled, "{kind:?} is disabled");
    r.h.press(None, &button);
    pump(r);
}

/// Time passes without the controller saying anything more.
fn wait(r: &Rig, timeouts: u32) {
    for _ in 0..timeouts {
        r.clock.advance(TIMEOUT + Duration::from_millis(1));
        pump(r);
    }
}

/// The invariant of the whole section: whatever the fault, the job is not trusted and no start is
/// possible or sent. `allow_unconfirmed` admits "all chunks ACKed, storage not read back".
fn assert_untrusted(r: &Rig, allow_unconfirmed: bool) {
    let vm = r.h.machine_view().expect("machine panel");
    let bad = [
        MachineStatusKey::Stored,
        MachineStatusKey::StartRequested,
        MachineStatusKey::Running,
        MachineStatusKey::Completed,
    ];
    assert!(!bad.contains(&vm.status), "an uncertain upload is shown as {:?}", vm.status);
    if !allow_unconfirmed {
        assert_ne!(vm.status, MachineStatusKey::UploadedNotConfirmed, "ACKs are missing, so even this is too much");
    }
    assert!(!vm.offers(MachineAction::Start), "a start is offered after a fault");
    assert_eq!(starts_sent(&r.peer), 0, "a start reached the controller");
}

fn ready() -> (Rig, JobCardViewModel) {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    assert!(chunk_count(&job) >= 3, "the fixture job must span several chunks, has {}", chunk_count(&job));
    (r, job)
}

// ---------------------------------------------------------------------------------------------
// ACK lost, duplicated, delayed, wrong source
// ---------------------------------------------------------------------------------------------

#[test]
fn tr_ack_lost_makes_the_upload_unknown_and_the_chunk_is_never_resent() {
    let (mut r, job) = ready();
    script_identity(&r.peer, MODEL);
    r.peer.script(chunk_is(0).reply(Reply::Ack));
    r.peer.script(chunk_is(1).lose_reply(Reply::Ack));
    press_upload(&mut r, &job);
    wait(&r, 6);
    assert_eq!(status(&r), MachineStatusKey::Unknown);
    assert_eq!(chunks_sent(&r, 1), 1, "a non-idempotent chunk is not resent after a lost ACK");
    assert_eq!(chunks_sent(&r, 2), 0, "and the following chunks are not sent past an unconfirmed one");
    assert_untrusted(&r, false);
    assert!(
        !r.h.machine_view().unwrap().offers(MachineAction::ConfirmStored) || status(&r) != MachineStatusKey::Stored
    );
}

#[test]
fn tr_duplicate_ack_does_not_confirm_the_next_chunk() {
    let (mut r, job) = ready();
    script_identity(&r.peer, MODEL);
    r.peer.script(chunk_is(0).reply_duplicated(Reply::Ack, 2));
    r.peer.script(chunk_is(1)); // the controller says nothing to chunk 1
    press_upload(&mut r, &job);
    wait(&r, 4);
    assert_eq!(status(&r), MachineStatusKey::Unknown, "the stray second ACK of chunk 0 must not stand in for chunk 1");
    assert_eq!(chunks_sent(&r, 1), 1);
    assert_eq!(chunks_sent(&r, 2), 0);
    assert_untrusted(&r, false);
}

#[test]
fn tr_delayed_ack_after_the_timeout_does_not_revive_the_upload() {
    let (mut r, job) = ready();
    script_identity(&r.peer, MODEL);
    r.peer.script(chunk_is(0).reply(Reply::Ack));
    r.peer.script(chunk_is(1).reply_after(TIMEOUT * 3, Reply::Ack));
    press_upload(&mut r, &job);
    wait(&r, 2); // timeout: unknown
    assert_eq!(status(&r), MachineStatusKey::Unknown);
    wait(&r, 4); // the late ACK arrives now
    assert_eq!(status(&r), MachineStatusKey::Unknown, "a late ACK does not turn Unknown back into progress");
    assert_eq!(chunks_sent(&r, 1), 1);
    assert_eq!(chunks_sent(&r, 2), 0);
    assert_untrusted(&r, false);
}

#[test]
fn tr_delayed_content_reply_is_accepted_only_when_it_really_arrives() {
    // All chunks ACKed; the read-back "stored?" answer is slow.
    let (mut r, job) = ready();
    script_upload_acks(&r.peer, &job);
    press_upload(&mut r, &job);
    assert_eq!(status(&r), MachineStatusKey::UploadedNotConfirmed);

    // (a) The answer arrives within the reply timeout, delayed: now it is Stored.
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Ack).reply_after(TIMEOUT / 2, Reply::Stored(true)));
    press(&mut r, MachineAction::ConfirmStored);
    assert_eq!(status(&r), MachineStatusKey::UploadedNotConfirmed, "ACK alone, the content reply is still on its way");
    r.clock.advance(TIMEOUT / 2 + Duration::from_millis(1));
    pump(&r);
    assert_eq!(status(&r), MachineStatusKey::Stored);

    // (b) A controller that never answers the read-back: the job is never called stored.
    let (mut r, job) = ready();
    script_upload_acks(&r.peer, &job);
    press_upload(&mut r, &job);
    for _ in 0..3 {
        r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Ack));
    }
    press(&mut r, MachineAction::ConfirmStored);
    wait(&r, 6);
    assert_ne!(status(&r), MachineStatusKey::Stored);
    assert!(!r.h.machine_view().unwrap().offers(MachineAction::Start));
    assert_eq!(starts_sent(&r.peer), 0);
}

#[test]
fn tr_wrong_source_replies_are_ignored() {
    let (mut r, job) = ready();
    script_identity(&r.peer, MODEL);
    r.peer.script(chunk_is(0).reply(Reply::Ack));
    // From here every reply claims to come from another machine.
    press_upload(&mut r, &job);
    assert_eq!(chunks_sent(&r, 0), 1);
    spoof_source(true);
    wait(&r, 4);
    spoof_source(false);
    assert!(
        matches!(status(&r), MachineStatusKey::Unknown | MachineStatusKey::Uploading),
        "replies of another machine did not move the upload: {:?}",
        status(&r)
    );
    assert_eq!(chunks_sent(&r, 2), 0);
    assert_untrusted(&r, false);
}

// ---------------------------------------------------------------------------------------------
// disconnect, busy, disk full
// ---------------------------------------------------------------------------------------------

#[test]
fn tr_disconnect_in_the_middle_of_a_chunk_is_unknown_and_a_reconnect_sends_nothing() {
    let (mut r, job) = ready();
    script_identity(&r.peer, MODEL);
    r.peer.script(chunk_is(0).reply(Reply::Ack));
    r.peer.script(chunk_is(1).reply(Reply::Ack).then_disconnect());
    press_upload(&mut r, &job);
    wait(&r, 3);
    assert_eq!(status(&r), MachineStatusKey::Unknown);
    let sent = r.peer.sent().len();
    press(&mut r, MachineAction::Reconnect);
    assert_eq!(r.peer.sent().len(), sent, "a reconnect sends nothing");
    assert_eq!(status(&r), MachineStatusKey::Unknown);
    assert_eq!(chunks_sent(&r, 1), 1, "the interrupted chunk is not resent");
    assert!(!r.h.machine_view().unwrap().offers(MachineAction::Upload));
    assert_untrusted(&r, false);
}

#[test]
fn tr_busy_controller_refuses_a_chunk_and_the_upload_fails_without_further_bytes() {
    // The wire form of "busy" is not known for any model; what a busy controller can do on the
    // verified wire is answer a chunk with NAK or ERR. Both are a refusal, never progress.
    for refusal in [Reply::Nak, Reply::Err] {
        let (mut r, job) = ready();
        script_identity(&r.peer, MODEL);
        r.peer.script(chunk_is(0).reply(Reply::Ack));
        r.peer.script(chunk_is(1).reply(refusal.clone()));
        press_upload(&mut r, &job);
        wait(&r, 1);
        assert_eq!(status(&r), MachineStatusKey::Failed, "{refusal:?}");
        assert_eq!(chunks_sent(&r, 2), 0, "{refusal:?}: nothing is sent after a refusal");
        assert_eq!(chunks_sent(&r, 1), 1);
        assert_untrusted(&r, false);
    }
}

#[test]
fn tr_disk_full_on_the_last_chunk_or_at_read_back_is_a_failure_not_a_stored_job() {
    // The controller's "disk full" has no distinct wire form either: it shows as an ERR on a chunk
    // or as "not stored" at read-back.
    let (mut r, job) = ready();
    script_identity(&r.peer, MODEL);
    let last = chunk_count(&job) - 1;
    for i in 0..last {
        r.peer.script(chunk_is(i).reply(Reply::Ack));
    }
    r.peer.script(chunk_is(last).reply(Reply::Err));
    press_upload(&mut r, &job);
    assert_eq!(status(&r), MachineStatusKey::Failed);
    assert_untrusted(&r, false);

    let (mut r, job) = ready();
    script_upload_acks(&r.peer, &job);
    press_upload(&mut r, &job);
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Ack).reply(Reply::Stored(false)));
    press(&mut r, MachineAction::ConfirmStored);
    assert_eq!(status(&r), MachineStatusKey::Failed, "every chunk ACKed, but the controller does not hold the file");
    assert_untrusted(&r, true);
}

#[test]
fn tr_identity_of_another_model_uploads_nothing() {
    let (mut r, job) = ready();
    script_identity(&r.peer, "RDC9999X");
    press_upload(&mut r, &job);
    assert_eq!(status(&r), MachineStatusKey::Failed);
    assert!(uploaded_bytes(&r.peer).is_empty());
    assert_untrusted(&r, false);
}

// ---------------------------------------------------------------------------------------------
// app restart
// ---------------------------------------------------------------------------------------------

#[test]
fn tr_app_restart_in_the_middle_of_an_upload_resumes_nothing_and_offers_no_start() {
    let (mut r, job) = ready();
    script_identity(&r.peer, MODEL);
    r.peer.script(chunk_is(0).reply(Reply::Ack));
    r.peer.script(chunk_is(1)); // in flight when the app dies
    press_upload(&mut r, &job);
    let before = r.peer.sent().len();

    let restarted = service(&r.clock, &r.peer, Arc::clone(&r.history));
    let mut h = burnburn_presentation::HeadlessEditorHarness::new();
    h.controller().attach_machine(restarted.clone());
    let vm = h.machine_view().expect("machine panel");
    assert_eq!(vm.status, MachineStatusKey::Unknown, "an execution that was not finished is unknown after a restart");
    assert!(!vm.offers(MachineAction::Start));
    assert!(!vm.offers(MachineAction::Upload), "no automatic or one-click resume of the upload");
    restarted.poll().unwrap();
    assert_eq!(r.peer.sent().len(), before, "the restart itself sent nothing");
    assert_eq!(starts_sent(&r.peer), 0);
}

// ---------------------------------------------------------------------------------------------
// duplicate filename / overwrite: the app has no way to name or replace a controller file
// ---------------------------------------------------------------------------------------------

#[test]
fn tr_duplicate_filename_and_overwrite_are_refused_because_no_named_upload_is_wired() {
    use burnburn_test_support::FakeClock;
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let machine = burnburn_machine::MachineId::new(MACHINE);
    let port = FakePort::new(&peer, machine.clone());
    let identity = MachineIdentity { id: machine, model: Some(MODEL.into()), firmware: None };
    let mut actor = ExecutionActor::new(
        identity,
        port,
        Arc::new(clock.clone()),
        Policy { reply_timeout: TIMEOUT, max_query_attempts: 2 },
    );
    let s = || Capability::Supported;
    actor.set_capabilities(Capabilities {
        live: LiveCapabilities { jog: s(), home: s(), move_absolute: s(), pause_resume: s(), read_position: s() },
        files: FileCapabilities { list: s(), metadata: s(), upload_named: s(), run: s(), delete: s() },
    });
    // The controller already holds PLAAT01 (as a listing would show).
    let (mut r, job) = ready();
    let _ = &mut r;
    let artifact = RdArtifact::from_rd_file(job.plan, &[1, 2, 3, 4], 2).unwrap();
    let name = ControllerFileName::new("PLAAT01").unwrap();
    for attempt in 0..2 {
        let err = actor.save_rd_to_controller(&name, &artifact).unwrap_err();
        assert_eq!(err, LiveError::NoWireForm { feature: LiveFeature::FileUpload }, "attempt {attempt}");
    }
    assert!(peer.sent().is_empty(), "no bytes, no delete, no replace reached the controller");
    // And the normal job upload never lists, replaces or deletes a controller file by itself.
    let ordinary = {
        let (mut r2, job2) = ready();
        script_upload_acks(&r2.peer, &job2);
        press_upload(&mut r2, &job2);
        r2.peer.sent()
    };
    assert!(ordinary.iter().all(|q| !matches!(q, Request::Live(_))), "an upload issues no file-management request");
}

// ---------------------------------------------------------------------------------------------
// corrupted checksum, truncation: the datagram codec (pure) and the reply byte
// ---------------------------------------------------------------------------------------------

fn flip(datagram: &[u8], bit: usize) -> Vec<u8> {
    let mut d = datagram.to_vec();
    d[bit / 8] ^= 1 << (bit % 8);
    d
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

    /// A single flipped bit anywhere in a framed datagram is always caught: in the checksum bytes
    /// the declaration changes, in the payload the sum changes by a power of two (never 0 mod 2^16).
    #[test]
    fn tr_a_corrupted_datagram_never_validates(payload in proptest::collection::vec(any::<u8>(), 1..400), bit in 0_usize..8_000) {
        let framed = frame(&payload).unwrap();
        prop_assert_eq!(parse_frame(&framed).unwrap(), &payload[..]);
        let bit = bit % (framed.len() * 8);
        let bad = flip(&framed, bit);
        prop_assert!(matches!(parse_frame(&bad), Err(UdpError::ChecksumMismatch { declared: _, computed: _ })), "bit {} flipped but accepted", bit);
    }

    /// Truncation is detected exactly when the removed bytes do not sum to zero: an independent
    /// statement of what a sum-of-bytes checksum can and cannot see.
    #[test]
    fn tr_a_truncated_datagram_is_caught_unless_it_only_lost_zero_bytes(
        payload in proptest::collection::vec(prop_oneof![3 => any::<u8>(), 1 => Just(0_u8)], 2..300), cut in 1_usize..300,
    ) {
        let framed = frame(&payload).unwrap();
        let keep = framed.len().saturating_sub(cut);
        let truncated = &framed[..keep];
        let removed_sum: u32 = framed[keep.max(2)..].iter().map(|b| u32::from(*b)).sum();
        let verdict = parse_frame(truncated);
        if keep < 2 {
            prop_assert!(matches!(verdict, Err(UdpError::TooShort { len: _ })), "short datagram accepted");
        } else if removed_sum == 0 && keep < framed.len() {
            prop_assert!(verdict.is_ok(), "only zero bytes were lost, the sum is unchanged");
        } else {
            prop_assert!(verdict.is_err(), "truncated by {cut} bytes (removed sum {removed_sum}) was accepted");
        }
    }
}

#[test]
fn tr_the_checksum_is_the_big_endian_sum_of_the_scrambled_bytes_and_wraps_at_16_bits() {
    // 300 bytes of 0xff: 300 * 255 = 76500 = 0x12AD4 -> 0x2AD4 after the 16-bit wrap.
    let payload = vec![0xff_u8; 300];
    assert_eq!(checksum(&payload), 0x2ad4);
    assert_eq!(&frame(&payload).unwrap()[..2], &[0x2a, 0xd4]);
    assert_eq!(parse_frame(&[]), Err(UdpError::TooShort { len: 0 }));
    assert_eq!(parse_frame(&[0x00]), Err(UdpError::TooShort { len: 1 }));
    assert_eq!(frame(&[]), Err(UdpError::EmptyPayload));
}

#[test]
fn tr_no_single_bit_corruption_of_an_ack_byte_reads_as_an_ack() {
    let magic = Magic::FIXTURE_0X88;
    let ack_wire = scramble(&[0xcc], magic)[0];
    assert_eq!(reply_from_wire(ack_wire, magic), Some(ReplyByte::Ack));
    for bit in 0..8 {
        let corrupted = ack_wire ^ (1 << bit);
        assert_ne!(reply_from_wire(corrupted, magic), Some(ReplyByte::Ack), "bit {bit}");
    }
}
