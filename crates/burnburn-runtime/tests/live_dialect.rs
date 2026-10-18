//! Wire forms of laser-window requests. Only commands named in the public registry have one, and
//! only through the opt-in `RegistryLiveDialect`; the production default has none.
//! A fake controller on 127.0.0.1 inside the test process stands in for the laser.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use burnburn_machine::{
    Capabilities, Capability, ControllerFileName, ExecutionActor, FileCapabilities, HomeTarget, JogKey,
    LiveCapabilities, LiveError, LiveOutcome, LiveRequest, MachineId, MachineIdentity, Policy, PortError, Request,
};
use burnburn_ruida::udp::parse_frame;
use burnburn_ruida::{Magic, scramble, scramble_byte, unscramble};
use burnburn_runtime::{NoVerifiedDialect, RegistryLiveDialect, RuidaUdpPort, SystemClock, UdpConfig, WireDialect};

const MAGIC: Magic = Magic::FIXTURE_0X88;

fn live(r: LiveRequest) -> Request {
    Request::Live(r)
}

#[test]
fn the_registry_dialect_encodes_exactly_the_registry_named_requests() {
    let d = RegistryLiveDialect;
    assert_eq!(d.encode(&live(LiveRequest::JogStart(JogKey::XLeft))), Some(vec![0xd8, 0x20]));
    assert_eq!(d.encode(&live(LiveRequest::JogStop(JogKey::XLeft))), Some(vec![0xd8, 0x30]));
    assert_eq!(d.encode(&live(LiveRequest::JogStart(JogKey::YBottom))), Some(vec![0xd8, 0x23]));
    assert_eq!(d.encode(&live(LiveRequest::Home(HomeTarget::Xy))), Some(vec![0xd8, 0x2a]));
    assert_eq!(d.encode(&live(LiveRequest::Pause)), Some(vec![0xd8, 0x02]));
    assert_eq!(d.encode(&live(LiveRequest::Resume)), Some(vec![0xd8, 0x03]));
    assert_eq!(d.encode(&Request::Stop), Some(vec![0xd8, 0x01]));
}

#[test]
fn requests_without_registry_evidence_have_no_wire_form_in_any_dialect() {
    let name = ControllerFileName::new("PLAAT01").unwrap();
    let unencodable = [
        live(LiveRequest::MoveAbsolute { x_um: 1, y_um: 2 }), // options byte of RAPID_MOVE_XY unexplained
        live(LiveRequest::ReadPosition),                      // GET_SETTING reply is not documented
        live(LiveRequest::ListFiles),
        live(LiveRequest::FileMetadata(name.clone())),
        live(LiveRequest::RunFile(name.clone())),
        live(LiveRequest::DeleteFile(name)),
        Request::Start, // starting a job is never a live command (D8 00)
        Request::QueryStatus,
        Request::QueryStored,
        Request::QueryIdentity,
    ];
    for dialect in [&RegistryLiveDialect as &dyn WireDialect, &NoVerifiedDialect] {
        for request in &unencodable {
            assert_eq!(dialect.encode(request), None, "{request:?}");
        }
    }
    // The production default has no live form at all.
    assert_eq!(NoVerifiedDialect.encode(&live(LiveRequest::Pause)), None);
    assert_eq!(NoVerifiedDialect.encode(&Request::Stop), None);
}

// ---- over a socket ----

struct Fake {
    addr: SocketAddr,
    received: Arc<Mutex<Vec<(Vec<u8>, Vec<u8>)>>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Fake {
    fn start() -> Fake {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.set_read_timeout(Some(Duration::from_millis(5))).unwrap();
        let addr = socket.local_addr().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, s) = (received.clone(), stop.clone());
        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while !s.load(Ordering::Acquire) {
                let Ok((n, from)) = socket.recv_from(&mut buf) else { continue };
                let Ok(payload) = parse_frame(&buf[..n]) else { continue };
                r.lock().unwrap().push((unscramble(payload, MAGIC), payload.to_vec()));
                socket.send_to(&[scramble_byte(0xcc, MAGIC)], from).unwrap();
            }
        });
        Fake { addr, received, stop, handle: Some(handle) }
    }

    fn received(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.received.lock().unwrap().clone()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(h) = self.handle.take() {
            h.join().unwrap();
        }
    }
}

fn caps() -> Capabilities {
    let s = || Capability::Supported;
    Capabilities {
        live: LiveCapabilities { jog: s(), home: s(), move_absolute: s(), pause_resume: s(), read_position: s() },
        files: FileCapabilities { list: s(), metadata: s(), upload_named: s(), run: s(), delete: s() },
    }
}

fn actor(fake: &Fake, dialect: Arc<dyn WireDialect>) -> ExecutionActor<RuidaUdpPort> {
    let cfg = UdpConfig {
        peer: fake.addr,
        local: "127.0.0.1:0".parse().unwrap(),
        magic: MAGIC,
        queue_capacity: 16,
        poll_interval: Duration::from_millis(5),
    };
    let machine = MachineId::new("bench");
    let port = RuidaUdpPort::connect(cfg, machine.clone(), dialect).unwrap();
    let identity = MachineIdentity { id: machine, model: Some("RDC6445G".into()), firmware: None };
    let mut a = ExecutionActor::new(
        identity,
        port,
        Arc::new(SystemClock::new()),
        Policy { reply_timeout: Duration::from_millis(300), max_query_attempts: 1 },
    );
    a.set_capabilities(caps());
    a
}

fn wait_settled(a: &mut ExecutionActor<RuidaUdpPort>) {
    for _ in 0..200 {
        a.poll().unwrap();
        if a.live_attempts().last().is_some_and(|x| x.outcome != LiveOutcome::Pending) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_jog_press_reaches_the_fake_controller_scrambled_with_a_valid_checksum_and_is_acked() {
    let fake = Fake::start();
    let mut a = actor(&fake, Arc::new(RegistryLiveDialect));
    a.live(LiveRequest::JogStart(JogKey::XLeft)).unwrap();
    wait_settled(&mut a);
    assert_eq!(a.live_attempts().last().unwrap().outcome, LiveOutcome::Acked);
    let got = fake.received();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, vec![0xd8, 0x20], "logical bytes = registry KEYDOWN_X_LEFT");
    assert_eq!(got[0].1, scramble(&[0xd8, 0x20], MAGIC), "scrambled on the wire");
    assert_eq!(a.jog_held(), Some(JogKey::XLeft));
}

#[test]
fn the_production_default_dialect_sends_nothing_for_any_live_request() {
    let fake = Fake::start();
    let mut a = actor(&fake, Arc::new(NoVerifiedDialect));
    let err = a.live(LiveRequest::Home(HomeTarget::Xy)).unwrap_err();
    assert!(matches!(err, LiveError::Port(PortError::NotSupported(_))), "{err:?}");
    assert_eq!(a.live_attempts().last().unwrap().outcome, LiveOutcome::NotSent);
    std::thread::sleep(Duration::from_millis(30));
    assert!(fake.received().is_empty(), "nothing may leave without a dialect");
}

#[test]
fn a_registry_dialect_still_refuses_a_move_it_cannot_encode() {
    let fake = Fake::start();
    let mut a = actor(&fake, Arc::new(RegistryLiveDialect));
    let err = a.live(LiveRequest::MoveAbsolute { x_um: 1_000, y_um: 1_000 }).unwrap_err();
    assert!(matches!(err, LiveError::Port(PortError::NotSupported(_))), "{err:?}");
    std::thread::sleep(Duration::from_millis(30));
    assert!(fake.received().is_empty());
}
