//! API01: the desktop-automation protocol over loopback UDP. Replies are checked as exact
//! datagrams; the host is a recording fake, the socket a scripted fake (plus one real
//! 127.0.0.1 test). Nothing here touches a machine, a project of the user or the network.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test harness

use std::collections::VecDeque;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::Duration;

use burnburn_runtime::automation::{
    AutomationEndpoint, AutomationHost, AutomationService, DatagramSocket, EndpointError, HostError, HostStatus,
    Served, bind_loopback,
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Load(PathBuf),
    Import(PathBuf),
}

struct FakeHost {
    status: HostStatus,
    calls: Vec<Call>,
    result: Result<(), HostError>,
}

impl FakeHost {
    fn clean() -> Self {
        FakeHost {
            status: HostStatus { has_document: true, dirty: false, busy: false },
            calls: Vec::new(),
            result: Ok(()),
        }
    }
}

impl AutomationHost for FakeHost {
    fn status(&self) -> HostStatus {
        self.status
    }

    fn load(&mut self, path: &Path) -> Result<(), HostError> {
        self.calls.push(Call::Load(path.to_path_buf()));
        self.result
    }

    fn import(&mut self, path: &Path) -> Result<(), HostError> {
        self.calls.push(Call::Import(path.to_path_buf()));
        self.result
    }
}

fn service(host: FakeHost) -> AutomationService<FakeHost> {
    AutomationService::new(host, true)
}

fn ask(s: &mut AutomationService<FakeHost>, text: &str) -> String {
    String::from_utf8(s.handle(text.as_bytes()).expect("a reply")).unwrap()
}

// ---- ping and status -------------------------------------------------------------------------

#[test]
fn ping_is_answered_with_pong_and_status_reports_state_and_dirtiness() {
    let mut s = service(FakeHost::clean());
    assert_eq!(ask(&mut s, "PING"), "OK:PONG");
    assert_eq!(ask(&mut s, "status\r\n"), "OK:READY:CLEAN", "verbs are case-insensitive, a line end is ignored");
    let host = &mut s;
    let _ = host;

    let mut dirty = FakeHost::clean();
    dirty.status.dirty = true;
    assert_eq!(ask(&mut service(dirty), "STATUS"), "OK:READY:DIRTY");

    let mut busy = FakeHost::clean();
    busy.status.busy = true;
    assert_eq!(ask(&mut service(busy), "STATUS"), "OK:BUSY:CLEAN");

    let mut none = FakeHost::clean();
    none.status.has_document = false;
    assert_eq!(ask(&mut service(none), "STATUS"), "OK:READY:NODOC");
}

// ---- load: dirty checks ----------------------------------------------------------------------

#[test]
fn load_opens_the_project_when_nothing_would_be_lost() {
    let mut s = service(FakeHost::clean());
    assert_eq!(ask(&mut s, "LOADFILE:/Users/ik/werk/doos.burnburn"), "OK");
    assert_eq!(s.host().calls, [Call::Load(PathBuf::from("/Users/ik/werk/doos.burnburn"))]);
}

#[test]
fn load_never_overwrites_a_changed_document_and_says_so() {
    let mut host = FakeHost::clean();
    host.status.dirty = true;
    let mut s = service(host);
    assert_eq!(ask(&mut s, "LOADFILE:/Users/ik/werk/doos.burnburn"), "ERR:DIRTY");
    assert!(s.host().calls.is_empty(), "the host was not asked to load anything");
    // There is deliberately no force variant: the user decides in the application.
    assert_eq!(ask(&mut s, "FORCELOAD:/Users/ik/werk/doos.burnburn"), "ERR:UNKNOWN");
    assert!(s.host().calls.is_empty());
}

#[test]
fn load_without_a_document_is_fine_because_nothing_can_be_lost() {
    let mut host = FakeHost::clean();
    host.status.has_document = false;
    let mut s = service(host);
    assert_eq!(ask(&mut s, "LOADFILE:/a/b.burnburn"), "OK");
}

#[test]
fn a_busy_application_refuses_load_and_import_without_touching_the_document() {
    let mut host = FakeHost::clean();
    host.status.busy = true;
    let mut s = service(host);
    assert_eq!(ask(&mut s, "LOADFILE:/a/b.burnburn"), "ERR:BUSY");
    assert_eq!(ask(&mut s, "IMPORT:/a/b.svg"), "ERR:BUSY");
    assert!(s.host().calls.is_empty());
}

// ---- import ----------------------------------------------------------------------------------

#[test]
fn import_adds_to_the_open_document_even_when_it_has_unsaved_changes() {
    let mut host = FakeHost::clean();
    host.status.dirty = true;
    let mut s = service(host);
    assert_eq!(ask(&mut s, "IMPORT:/Users/ik/logo.svg"), "OK");
    assert_eq!(s.host().calls, [Call::Import(PathBuf::from("/Users/ik/logo.svg"))]);
}

#[test]
fn import_needs_an_open_document() {
    let mut host = FakeHost::clean();
    host.status.has_document = false;
    let mut s = service(host);
    assert_eq!(ask(&mut s, "IMPORT:/Users/ik/logo.svg"), "ERR:NODOC");
    assert!(s.host().calls.is_empty());
}

#[test]
fn host_failures_are_distinct_replies() {
    for (error, reply) in [
        (HostError::NotFound, "ERR:NOTFOUND"),
        (HostError::Unsupported, "ERR:FORMAT"),
        (HostError::Dirty, "ERR:DIRTY"),
        (HostError::Failed, "ERR:FAILED"),
    ] {
        let mut host = FakeHost::clean();
        host.result = Err(error);
        assert_eq!(ask(&mut service(host), "LOADFILE:/a/b.burnburn"), reply);
    }
}

// ---- paths -----------------------------------------------------------------------------------

#[test]
fn only_clean_absolute_paths_with_a_known_extension_reach_the_host() {
    let mut s = service(FakeHost::clean());
    for bad in [
        "LOADFILE:",
        "LOADFILE:relatief/doos.burnburn",
        "LOADFILE:/a/../etc/doos.burnburn",
        "LOADFILE:/a/b.txt",
        "LOADFILE:/a/b",
        "LOADFILE:/a/b\0.burnburn",
        "LOADFILE:/a/b\n.burnburn",
        "IMPORT:/a/b.burnburn",
        "IMPORT:/a/b.exe",
    ] {
        assert_eq!(ask(&mut s, bad), "ERR:PATH", "{bad:?}");
    }
    let long = format!("LOADFILE:/{}.burnburn", "x".repeat(1050));
    assert_eq!(ask(&mut s, &long), "ERR:PATH");
    assert!(s.host().calls.is_empty());
    // Spaces and other letters are fine, and the extension is not case sensitive.
    assert_eq!(ask(&mut s, "LOADFILE:/Users/ik/Mijn werk/Doos 2.BURNBURN"), "OK");
    assert_eq!(ask(&mut s, "IMPORT:/Users/ik/logo.SVG"), "OK");
    assert_eq!(s.host().calls.len(), 2);
}

// ---- machine actions are not available -------------------------------------------------------

#[test]
fn machine_actions_are_forbidden_and_never_reach_the_host() {
    let mut s = service(FakeHost::clean());
    for verb in ["START", "FRAME", "JOG", "HOME", "SEND", "UPLOAD", "STOP", "PAUSE", "RUN", "BURN", "CONNECT", "MOVE"] {
        assert_eq!(ask(&mut s, &verb.to_lowercase()), "ERR:FORBIDDEN", "{verb}");
        assert_eq!(ask(&mut s, &format!("{verb}:/a/b.rd")), "ERR:FORBIDDEN", "{verb} with argument");
    }
    assert!(s.host().calls.is_empty());
}

#[test]
fn unknown_and_malformed_requests_get_an_error_reply_not_silence() {
    let mut s = service(FakeHost::clean());
    assert_eq!(ask(&mut s, "HELLO"), "ERR:UNKNOWN");
    assert_eq!(ask(&mut s, ""), "ERR:UNKNOWN");
    assert_eq!(ask(&mut s, "PING:extra"), "ERR:MALFORMED");
    assert_eq!(ask(&mut s, "STATUS:x"), "ERR:MALFORMED");
    assert_eq!(String::from_utf8(s.handle(&[0xff, 0xfe, 0x00]).unwrap()).unwrap(), "ERR:MALFORMED");
    let huge = vec![b'A'; 5000];
    assert_eq!(String::from_utf8(s.handle(&huge).unwrap()).unwrap(), "ERR:MALFORMED");
    assert!(s.host().calls.is_empty());
}

// ---- opt-in ----------------------------------------------------------------------------------

#[test]
fn a_disabled_service_does_not_answer_and_does_not_act() {
    let mut s = AutomationService::new(FakeHost::clean(), false);
    assert_eq!(s.handle(b"PING"), None);
    assert_eq!(s.handle(b"LOADFILE:/a/b.burnburn"), None);
    assert!(s.host().calls.is_empty());
    s.set_enabled(true);
    assert_eq!(s.handle(b"PING"), Some(b"OK:PONG".to_vec()));
    s.set_enabled(false);
    assert_eq!(s.handle(b"PING"), None);
}

// ---- the socket ------------------------------------------------------------------------------

struct FakeSocket {
    local: SocketAddr,
    incoming: VecDeque<(Vec<u8>, SocketAddr)>,
    sent: Vec<(Vec<u8>, SocketAddr)>,
}

impl FakeSocket {
    fn on(local: SocketAddr) -> Self {
        FakeSocket { local, incoming: VecDeque::new(), sent: Vec::new() }
    }
}

impl DatagramSocket for FakeSocket {
    fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let (data, from) = self.incoming.pop_front().ok_or_else(|| io::Error::from(io::ErrorKind::WouldBlock))?;
        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok((n, from))
    }

    fn send_to(&mut self, data: &[u8], to: SocketAddr) -> io::Result<usize> {
        self.sent.push((data.to_vec(), to));
        Ok(data.len())
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.local)
    }
}

fn loopback(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

#[test]
fn an_endpoint_refuses_a_socket_that_is_not_bound_to_loopback() {
    for addr in [
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 19850),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)), 19850),
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 19850),
    ] {
        let result = AutomationEndpoint::new(FakeSocket::on(addr), service(FakeHost::clean()));
        assert!(matches!(result, Err(EndpointError::NotLoopback)), "{addr}");
    }
    assert!(AutomationEndpoint::new(FakeSocket::on(loopback(19850)), service(FakeHost::clean())).is_ok());
    assert!(
        AutomationEndpoint::new(
            FakeSocket::on(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 19850)),
            service(FakeHost::clean())
        )
        .is_ok()
    );
}

#[test]
fn exactly_one_reply_goes_back_to_the_sender_and_foreign_senders_are_dropped() {
    let mut socket = FakeSocket::on(loopback(19850));
    socket.incoming.push_back((b"PING".to_vec(), loopback(50001)));
    socket
        .incoming
        .push_back((b"LOADFILE:/a/b.burnburn".to_vec(), SocketAddr::new(Ipv4Addr::new(10, 0, 0, 7).into(), 4000)));
    socket.incoming.push_back((b"STATUS".to_vec(), loopback(50002)));
    let mut endpoint = AutomationEndpoint::new(socket, service(FakeHost::clean())).unwrap();
    assert_eq!(endpoint.serve_one().unwrap(), Served::Replied);
    assert_eq!(endpoint.serve_one().unwrap(), Served::DroppedNonLoopback);
    assert_eq!(endpoint.serve_one().unwrap(), Served::Replied);
    assert!(endpoint.service().host().calls.is_empty(), "the remote request was never executed");
    let sent = endpoint.into_socket().sent;
    assert_eq!(sent, [(b"OK:PONG".to_vec(), loopback(50001)), (b"OK:READY:CLEAN".to_vec(), loopback(50002))]);
}

#[test]
fn a_disabled_service_is_silent_on_the_socket() {
    let mut socket = FakeSocket::on(loopback(19850));
    socket.incoming.push_back((b"PING".to_vec(), loopback(50001)));
    let mut endpoint = AutomationEndpoint::new(socket, AutomationService::new(FakeHost::clean(), false)).unwrap();
    assert_eq!(endpoint.serve_one().unwrap(), Served::Silent);
    assert!(endpoint.into_socket().sent.is_empty());
}

#[test]
fn bind_loopback_only_listens_on_127_0_0_1_and_answers_a_real_datagram() {
    let socket = bind_loopback(0).unwrap();
    let addr = socket.local_addr().unwrap();
    assert!(addr.ip().is_loopback(), "{addr}");
    socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut endpoint = AutomationEndpoint::new(socket, service(FakeHost::clean())).unwrap();

    let client = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    client.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    client.send_to(b"PING", addr).unwrap();
    assert_eq!(endpoint.serve_one().unwrap(), Served::Replied);
    let mut buf = [0u8; 64];
    let (n, from) = client.recv_from(&mut buf).unwrap();
    assert_eq!((&buf[..n], from), (&b"OK:PONG"[..], addr));
}
