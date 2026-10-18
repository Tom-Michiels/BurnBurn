//! `RuidaUdpPort` against a fake controller thread on 127.0.0.1 inside this test process
//! (random ports). No real laser or network device is ever contacted.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test harness

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use burnburn_job::{JobPlanBuilder, JobPlanId, MachineFrame, MachinePoint, OriginCorner, PlanSource, ProfileRevision};
use burnburn_machine::{
    ControllerIdentity, ControllerPort, ExecutionActor, ExecutionId, ExecutionState, FailReason, MachineId,
    MachineIdentity, Policy, PortError, RdArtifact, Reply, Request, UnknownReason,
};
use burnburn_ruida::udp::{MAX_PAYLOAD, parse_frame};
use burnburn_ruida::{Magic, scramble, scramble_byte, unscramble};
use burnburn_runtime::{RuidaUdpPort, SystemClock, UdpConfig, WireDialect};
use burnburn_types::{ContentHash, HashDomain, RevisionId};

const MAGIC: Magic = Magic::FIXTURE_0X88;
/// Test-only wire form of the identity query (not a real Ruida command).
const IDENTITY_QUERY: &[u8] = &[0x7a, 0x01];

/// Test dialect: an invented identity query/answer, only meaningful to the fake below.
struct TestDialect;

impl WireDialect for TestDialect {
    fn encode(&self, request: &Request) -> Option<Vec<u8>> {
        matches!(request, Request::QueryIdentity).then(|| IDENTITY_QUERY.to_vec())
    }

    fn decode(&self, request: &Request, payload: &[u8]) -> Option<Reply> {
        let text = std::str::from_utf8(payload.strip_prefix(b"ID:")?).ok()?;
        let (model, firmware) = text.split_once('|')?;
        matches!(request, Request::QueryIdentity)
            .then(|| Reply::Identity(ControllerIdentity { model: Some(model.into()), firmware: Some(firmware.into()) }))
    }
}

/// What the fake does with one received (checksum-valid) datagram: the datagrams it sends back.
type Behaviour = Box<dyn FnMut(&Received) -> Vec<Vec<u8>> + Send>;

#[derive(Debug, Clone)]
struct Received {
    /// Unscrambled payload (checksum already verified).
    logical: Vec<u8>,
    /// Scrambled payload as on the wire.
    scrambled: Vec<u8>,
}

struct FakeController {
    addr: SocketAddr,
    received: Arc<Mutex<Vec<Received>>>,
    bad_frames: Arc<Mutex<usize>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl FakeController {
    fn start(mut behaviour: Behaviour) -> Self {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_read_timeout(Some(Duration::from_millis(5))).unwrap();
        let addr = socket.local_addr().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let bad_frames = Arc::new(Mutex::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, b, s) = (received.clone(), bad_frames.clone(), stop.clone());
        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while !s.load(Ordering::Acquire) {
                let Ok((n, from)) = socket.recv_from(&mut buf) else { continue };
                let Ok(payload) = parse_frame(&buf[..n]) else {
                    *b.lock().unwrap() += 1;
                    continue;
                };
                let rec = Received { logical: unscramble(payload, MAGIC), scrambled: payload.to_vec() };
                r.lock().unwrap().push(rec.clone());
                for reply in behaviour(&rec) {
                    socket.send_to(&reply, from).unwrap();
                }
            }
        });
        FakeController { addr, received, bad_frames, stop, handle: Some(handle) }
    }

    fn received(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
    }

    /// Stops answering (and receiving): the controller "disappears".
    fn shut_down(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(h) = self.handle.take() {
            h.join().unwrap();
        }
    }
}

impl Drop for FakeController {
    fn drop(&mut self) {
        self.shut_down();
    }
}

fn ack() -> Vec<u8> {
    vec![scramble_byte(0xcc, MAGIC)]
}

fn identity(model: &str) -> Vec<u8> {
    scramble(format!("ID:{model}|fake-1").as_bytes(), MAGIC)
}

fn is_identity_query(r: &Received) -> bool {
    r.logical == IDENTITY_QUERY
}

const TIMEOUT: Duration = Duration::from_millis(150);

fn plan_id() -> JobPlanId {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"udp-test"),
        machine_profile: ProfileRevision {
            id: "bench".into(),
            revision: 1,
            content: ContentHash::of(HashDomain::Profile, b"bench"),
        },
        compiler_version: "test".into(),
    };
    let frame = MachineFrame::new(100.0, 100.0, OriginCorner::BottomLeft).unwrap();
    JobPlanBuilder::new(source, frame, MachinePoint::new(0.0, 0.0).unwrap()).finish().unwrap().id()
}

/// A pseudo RD file of 3000 bytes (3 chunks at 1470).
fn rd_file() -> Vec<u8> {
    (0..3000u32).map(|i| (i.wrapping_mul(31) % 251) as u8).collect()
}

fn config(peer: SocketAddr) -> UdpConfig {
    UdpConfig {
        peer,
        local: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        magic: MAGIC,
        queue_capacity: 16,
        poll_interval: Duration::from_millis(5),
    }
}

fn actor(peer: SocketAddr) -> ExecutionActor<RuidaUdpPort> {
    let machine = MachineId::new("loopback-fake");
    let port = RuidaUdpPort::connect(config(peer), machine.clone(), Arc::new(TestDialect)).unwrap();
    let identity = MachineIdentity { id: machine, model: Some("RDC6445G".into()), firmware: Some("fake-1".into()) };
    ExecutionActor::new(
        identity,
        port,
        Arc::new(SystemClock::new()),
        Policy { reply_timeout: TIMEOUT, max_query_attempts: 2 },
    )
}

fn state(a: &ExecutionActor<RuidaUdpPort>) -> ExecutionState {
    a.execution().map(|e| e.state.clone()).unwrap()
}

/// Polls until `done` holds or 3 s pass; returns the final state.
fn drive(a: &mut ExecutionActor<RuidaUdpPort>, done: impl Fn(&ExecutionState) -> bool) -> ExecutionState {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        a.poll().unwrap();
        let s = state(a);
        if done(&s) {
            return s;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    state(a)
}

fn upload(a: &mut ExecutionActor<RuidaUdpPort>) {
    let artifact = RdArtifact::from_rd_file(plan_id(), &rd_file(), MAX_PAYLOAD).unwrap();
    a.begin_upload(ExecutionId::from_raw(1), &artifact).unwrap();
}

fn not_uploading(s: &ExecutionState) -> bool {
    !matches!(s, ExecutionState::VerifyingIdentity | ExecutionState::Uploading { .. })
}

#[test]
fn upload_reaches_upload_acked_with_checksummed_unchanged_chunks() {
    let fake = FakeController::start(Box::new(|r| {
        if is_identity_query(r) { vec![ack(), identity("RDC6445G")] } else { vec![ack()] }
    }));
    let mut a = actor(fake.addr);
    upload(&mut a);
    assert_eq!(drive(&mut a, not_uploading), ExecutionState::UploadAcked);

    let got = fake.received();
    assert!(is_identity_query(&got[0]), "identity is checked before any job byte");
    let chunks: Vec<_> = got[1..].iter().map(|r| r.scrambled.clone()).collect();
    assert_eq!(chunks.iter().map(Vec::len).collect::<Vec<_>>(), vec![1470, 1470, 60]);
    assert_eq!(chunks.concat(), rd_file(), "RD bytes are framed unchanged (no second scrambling)");
    assert_eq!(*fake.bad_frames.lock().unwrap(), 0, "every datagram had a valid checksum");
}

#[test]
fn lost_packet_ack_becomes_unknown_and_the_packet_is_not_resent() {
    let mut n = 0;
    let fake = FakeController::start(Box::new(move |r| {
        if is_identity_query(r) {
            return vec![identity("RDC6445G")];
        }
        n += 1;
        if n == 2 { Vec::new() } else { vec![ack()] } // the ACK of chunk 1 is "lost"
    }));
    let mut a = actor(fake.addr);
    upload(&mut a);
    let s = drive(&mut a, not_uploading);
    assert_eq!(s, ExecutionState::Unknown(UnknownReason::UploadChunkUnconfirmed { index: 1 }));
    std::thread::sleep(TIMEOUT * 2);
    a.poll().unwrap();
    assert_eq!(fake.received().len(), 3, "identity + chunk 0 + chunk 1 once; nothing resent");
}

#[test]
fn duplicated_ack_does_not_confirm_the_next_packet() {
    let mut n = 0;
    let fake = FakeController::start(Box::new(move |r| {
        if is_identity_query(r) {
            return vec![identity("RDC6445G")];
        }
        n += 1;
        if n == 1 { vec![ack(), ack()] } else { Vec::new() } // chunk 0 ACKed twice, chunk 1 never
    }));
    let mut a = actor(fake.addr);
    upload(&mut a);
    let s = drive(&mut a, not_uploading);
    assert_eq!(s, ExecutionState::Unknown(UnknownReason::UploadChunkUnconfirmed { index: 1 }));
}

#[test]
fn identity_mismatch_over_udp_sends_no_job_bytes() {
    let fake =
        FakeController::start(Box::new(
            |r| if is_identity_query(r) { vec![identity("RDC6442S")] } else { vec![ack()] },
        ));
    let mut a = actor(fake.addr);
    upload(&mut a);
    assert_eq!(drive(&mut a, not_uploading), ExecutionState::Failed(FailReason::IdentityMismatch));
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(fake.received().len(), 1, "only the identity query reached the controller");
}

#[test]
fn controller_disconnect_is_unknown_and_reconnect_sends_nothing() {
    let mut fake =
        FakeController::start(Box::new(|r| if is_identity_query(r) { vec![identity("RDC6445G")] } else { Vec::new() }));
    let mut a = actor(fake.addr);
    upload(&mut a);
    // Wait until chunk 0 left, then the controller vanishes.
    let deadline = Instant::now() + Duration::from_secs(2);
    while fake.received().len() < 2 && Instant::now() < deadline {
        a.poll().unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
    fake.shut_down();
    let s = drive(&mut a, not_uploading);
    assert_eq!(s, ExecutionState::Unknown(UnknownReason::UploadChunkUnconfirmed { index: 0 }));
    let before = fake.received().len();
    a.reconnect().unwrap();
    for _ in 0..20 {
        a.poll().unwrap();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fake.received().len(), before);
    assert_eq!(state(&a), ExecutionState::Unknown(UnknownReason::UploadChunkUnconfirmed { index: 0 }));
}

#[test]
fn datagram_from_a_foreign_address_is_ignored_and_counted() {
    let fake =
        FakeController::start(Box::new(|r| if is_identity_query(r) { vec![identity("RDC6445G")] } else { Vec::new() }));
    let mut a = actor(fake.addr);
    upload(&mut a);
    let deadline = Instant::now() + Duration::from_secs(2);
    while fake.received().len() < 2 && Instant::now() < deadline {
        a.poll().unwrap();
        std::thread::sleep(Duration::from_millis(2));
    }
    // Someone else on the network "ACKs" chunk 0.
    let intruder = UdpSocket::bind("127.0.0.1:0").unwrap();
    intruder.send_to(&ack(), a.port().local_addr().unwrap()).unwrap();
    let s = drive(&mut a, not_uploading);
    assert_eq!(s, ExecutionState::Unknown(UnknownReason::UploadChunkUnconfirmed { index: 0 }));
    assert_eq!(a.port().stats().foreign, 1);
}

#[test]
fn flood_beyond_the_queue_is_dropped_without_blocking() {
    let fake = FakeController::start(Box::new(|r| {
        if is_identity_query(r) { vec![identity("RDC6445G"); 200] } else { Vec::new() }
    }));
    let mut port =
        RuidaUdpPort::connect(config(fake.addr), MachineId::new("loopback-fake"), Arc::new(TestDialect)).unwrap();
    let tag = burnburn_machine::RequestTag { execution: ExecutionId::from_raw(1), epoch: 0, seq: 0 };
    port.send(burnburn_machine::Envelope { to: MachineId::new("loopback-fake"), tag, request: Request::QueryIdentity })
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let mut delivered = 0;
    while let Some(i) = port.try_recv().unwrap() {
        assert_eq!(i.tag, tag);
        delivered += 1;
    }
    let stats = port.stats();
    assert!(delivered <= 16, "queue is bounded: {delivered}");
    // Loopback normally delivers all 200; the kernel may drop some under load, never add.
    assert!(stats.dropped_queue_full > 0, "{stats:?}");
    assert!(delivered as u64 + stats.dropped_queue_full <= 200, "{stats:?}");
}

#[test]
fn broadcast_unspecified_and_multicast_peers_are_refused() {
    for peer in ["255.255.255.255:50200", "0.0.0.0:50200", "224.0.0.1:50200", "127.0.0.1:0"] {
        let cfg = config(peer.parse().unwrap());
        let r = RuidaUdpPort::connect(cfg, MachineId::new("x"), Arc::new(TestDialect));
        assert!(matches!(r, Err(PortError::NotSupported(_))), "{peer}: {r:?}");
    }
}

#[test]
fn request_without_verified_wire_form_is_refused_not_guessed() {
    let fake = FakeController::start(Box::new(|_| Vec::new()));
    let mut port = RuidaUdpPort::connect(
        config(fake.addr),
        MachineId::new("loopback-fake"),
        Arc::new(burnburn_runtime::NoVerifiedDialect),
    )
    .unwrap();
    let tag = burnburn_machine::RequestTag { execution: ExecutionId::from_raw(1), epoch: 0, seq: 0 };
    for request in [Request::QueryIdentity, Request::QueryStored, Request::Start, Request::Stop, Request::QueryStatus] {
        let env = burnburn_machine::Envelope { to: MachineId::new("loopback-fake"), tag, request: request.clone() };
        assert!(matches!(port.send(env), Err(PortError::NotSupported(_))), "{request:?}");
    }
    std::thread::sleep(Duration::from_millis(30));
    assert!(fake.received().is_empty());
}
