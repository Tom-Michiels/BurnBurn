//! Ruida Ethernet/UDP transport (`ControllerPort` implementation, IM16).
//!
//! - Framing comes from the pure codec (`burnburn_ruida::udp`): checksum prefix over the
//!   scrambled payload. RD chunks are already scrambled `.rd` bytes and are framed unchanged.
//! - One socket, bound to the configured local address, talking to exactly one configured peer.
//!   There is no discovery and no broadcast: unspecified, broadcast and multicast peers are
//!   refused. Datagrams from any other address are dropped and counted.
//! - A dedicated receive thread pushes datagrams into a bounded queue; when it is full the
//!   datagram is dropped and counted (a lost reply then times out in the actor and becomes
//!   `Unknown`/retried query — never an assumed success). `try_recv` never blocks.
//! - Ruida has no sequence numbers on the wire. The actor keeps one request in flight, so a
//!   reply is attributed to the last request sent; `reconnect` starts a fresh socket and queue,
//!   so nothing from before the reconnect is attributed afterwards.
//!
//! Wire forms other than RD chunks and the single-byte ACK/NAK/ERR/ENQ replies are not
//! confirmed for any controller. They come from a [`WireDialect`]; the only production dialect,
//! [`NoVerifiedDialect`], has none, so identity/storage/status/start requests are refused with
//! `PortError::NotSupported` instead of guessed.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::JoinHandle;
use std::time::Duration;

use burnburn_machine::{ControllerPort, Envelope, Incoming, MachineId, PortError, Reply, Request, RequestTag};
use burnburn_ruida::udp::{frame, reply_from_wire};
use burnburn_ruida::{Magic, ReplyByte, scramble, unscramble};

/// Controller-side port of the primary job channel in public implementations (OSS, unconfirmed).
pub const RUIDA_CONTROLLER_PORT: u16 = 50200;
/// Host-side port used by public implementations (OSS, unconfirmed).
pub const RUIDA_HOST_PORT: u16 = 40200;

/// Translation of non-chunk requests and data replies. Implementations must only encode what
/// has been verified for the concrete controller model and firmware.
pub trait WireDialect: Send + Sync {
    /// Logical (unscrambled) bytes for `request`, or `None` when there is no verified form.
    /// Never called for [`Request::UploadChunk`].
    fn encode(&self, request: &Request) -> Option<Vec<u8>>;
    /// Interprets an unscrambled multi-byte reply to `request`; `None` = not a reply to it.
    fn decode(&self, request: &Request, payload: &[u8]) -> Option<Reply>;
}

/// The production default: only RD chunks and single-byte replies are carried.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoVerifiedDialect;

impl WireDialect for NoVerifiedDialect {
    fn encode(&self, _request: &Request) -> Option<Vec<u8>> {
        None
    }

    fn decode(&self, _request: &Request, _payload: &[u8]) -> Option<Reply> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UdpConfig {
    /// The one controller this port talks to.
    pub peer: SocketAddr,
    /// Local bind address (Ruida convention: port 40200; tests use 127.0.0.1:0).
    pub local: SocketAddr,
    pub magic: Magic,
    /// Capacity of the receive queue (datagrams).
    pub queue_capacity: usize,
    /// Receive thread wake-up interval (stop-flag latency).
    pub poll_interval: Duration,
}

impl UdpConfig {
    /// OSS-described defaults for one controller address. Not a measured configuration.
    pub fn ruida(peer: IpAddr) -> Self {
        UdpConfig {
            peer: SocketAddr::new(peer, RUIDA_CONTROLLER_PORT),
            local: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), RUIDA_HOST_PORT),
            magic: Magic::FIXTURE_0X88,
            queue_capacity: 64,
            poll_interval: Duration::from_millis(20),
        }
    }
}

/// Diagnostics counters (M07): what arrived but was not handed to the actor.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UdpStats {
    /// Datagrams from an address other than the configured peer.
    pub foreign: u64,
    /// Datagrams dropped because the receive queue was full.
    pub dropped_queue_full: u64,
    /// Datagrams with no request in flight to attribute them to.
    pub unattributed: u64,
    /// Multi-byte datagrams the dialect did not recognise as a reply.
    pub unrecognised: u64,
    /// ENQ bytes (meaning unconfirmed; not treated as a reply).
    pub enq: u64,
}

#[derive(Debug, Default)]
struct Counters {
    foreign: AtomicU64,
    dropped_queue_full: AtomicU64,
    unattributed: AtomicU64,
    unrecognised: AtomicU64,
    enq: AtomicU64,
}

fn bump(c: &AtomicU64) {
    c.fetch_add(1, Ordering::Relaxed);
}

struct Link {
    socket: UdpSocket,
    rx: Receiver<Vec<u8>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Link {
    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(h) = self.worker.take() {
            // The worker never panics (no unwrap/expect); a join error would only mean it did.
            let _ = h.join();
        }
    }
}

/// Production `ControllerPort` for one Ruida controller over UDP.
pub struct RuidaUdpPort {
    cfg: UdpConfig,
    machine: MachineId,
    dialect: Arc<dyn WireDialect>,
    counters: Arc<Counters>,
    link: Option<Link>,
    in_flight: Option<(RequestTag, Request)>,
}

impl fmt::Debug for RuidaUdpPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuidaUdpPort").field("peer", &self.cfg.peer).field("machine", &self.machine).finish()
    }
}

impl RuidaUdpPort {
    /// Binds the local socket and starts the receive thread. Sends nothing.
    pub fn connect(cfg: UdpConfig, machine: MachineId, dialect: Arc<dyn WireDialect>) -> Result<Self, PortError> {
        check_peer(&cfg)?;
        let counters = Arc::<Counters>::default();
        let link = spawn_link(&cfg, &counters)?;
        Ok(RuidaUdpPort { cfg, machine, dialect, counters, link: Some(link), in_flight: None })
    }

    /// The bound local address (diagnostics, tests).
    pub fn local_addr(&self) -> Result<SocketAddr, PortError> {
        let link = self.link.as_ref().ok_or(PortError::Disconnected)?;
        link.socket.local_addr().map_err(|e| PortError::Other(e.to_string()))
    }

    pub fn stats(&self) -> UdpStats {
        let c = &self.counters;
        let get = |a: &AtomicU64| a.load(Ordering::Relaxed);
        UdpStats {
            foreign: get(&c.foreign),
            dropped_queue_full: get(&c.dropped_queue_full),
            unattributed: get(&c.unattributed),
            unrecognised: get(&c.unrecognised),
            enq: get(&c.enq),
        }
    }
}

/// No discovery: the peer must be one concrete unicast address with a port.
fn check_peer(cfg: &UdpConfig) -> Result<(), PortError> {
    let ip = cfg.peer.ip();
    let broadcast = matches!(ip, IpAddr::V4(v4) if v4.is_broadcast());
    if ip.is_unspecified() || ip.is_multicast() || broadcast || cfg.peer.port() == 0 {
        return Err(PortError::NotSupported(format!(
            "peer {} is not a single unicast controller address (no discovery/broadcast)",
            cfg.peer
        )));
    }
    if cfg.queue_capacity == 0 {
        return Err(PortError::Other("queue capacity must be positive".into()));
    }
    Ok(())
}

fn io_error(e: &std::io::Error) -> PortError {
    PortError::Other(e.to_string())
}

fn spawn_link(cfg: &UdpConfig, counters: &Arc<Counters>) -> Result<Link, PortError> {
    let socket = UdpSocket::bind(cfg.local).map_err(|e| io_error(&e))?;
    let reader = socket.try_clone().map_err(|e| io_error(&e))?;
    reader.set_read_timeout(Some(cfg.poll_interval.max(Duration::from_millis(1)))).map_err(|e| io_error(&e))?;
    let (tx, rx) = mpsc::sync_channel(cfg.queue_capacity);
    let stop = Arc::new(AtomicBool::new(false));
    let (peer, worker_stop, worker_counters) = (cfg.peer, Arc::clone(&stop), Arc::clone(counters));
    let worker = std::thread::Builder::new()
        .name(format!("ruida-udp-{peer}"))
        .spawn(move || receive_loop(&reader, peer, &tx, &worker_stop, &worker_counters))
        .map_err(|e| io_error(&e))?;
    Ok(Link { socket, rx, stop, worker: Some(worker) })
}

/// Runs on the receive thread. Never panics; ends on stop, on an unrecoverable socket error,
/// or when the port side is gone (the queue then reports `Disconnected`).
fn receive_loop(
    socket: &UdpSocket,
    peer: SocketAddr,
    tx: &SyncSender<Vec<u8>>,
    stop: &AtomicBool,
    counters: &Counters,
) {
    let mut buf = [0u8; 2048];
    while !stop.load(Ordering::Acquire) {
        match socket.recv_from(&mut buf) {
            Ok((n, from)) => {
                if from != peer {
                    bump(&counters.foreign);
                    continue;
                }
                match tx.try_send(buf[..n].to_vec()) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => bump(&counters.dropped_queue_full),
                    Err(TrySendError::Disconnected(_)) => return,
                }
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::WouldBlock
                | std::io::ErrorKind::TimedOut
                | std::io::ErrorKind::Interrupted
                // ICMP "port unreachable" may surface on the next receive; it is not a reply.
                | std::io::ErrorKind::ConnectionRefused
                | std::io::ErrorKind::ConnectionReset => {}
                _ => return,
            },
        }
    }
}

impl RuidaUdpPort {
    fn datagram_for(&self, request: &Request) -> Result<Vec<u8>, PortError> {
        let scrambled = match request {
            // Already scrambled `.rd` bytes: frame unchanged.
            Request::UploadChunk { bytes, .. } => bytes.clone(),
            other => {
                let logical = self.dialect.encode(other).ok_or_else(|| {
                    PortError::NotSupported(format!("no verified wire form for {other:?} on this controller"))
                })?;
                scramble(&logical, self.cfg.magic)
            }
        };
        frame(&scrambled).map_err(|e| PortError::Other(format!("{e:?}")))
    }

    fn reply_for(&self, request: &Request, datagram: &[u8]) -> Option<Reply> {
        if let [byte] = datagram {
            match reply_from_wire(*byte, self.cfg.magic) {
                Some(ReplyByte::Ack) => return Some(Reply::Ack),
                Some(ReplyByte::Nak) => return Some(Reply::Nak),
                Some(ReplyByte::Err) => return Some(Reply::Err),
                Some(ReplyByte::Enq) => {
                    bump(&self.counters.enq);
                    return None;
                }
                None => {}
            }
        }
        let reply = self.dialect.decode(request, &unscramble(datagram, self.cfg.magic));
        if reply.is_none() {
            bump(&self.counters.unrecognised);
        }
        reply
    }
}

impl ControllerPort for RuidaUdpPort {
    fn send(&mut self, envelope: Envelope) -> Result<(), PortError> {
        if envelope.to != self.machine {
            return Err(PortError::Other(format!("port for {} cannot address {}", self.machine, envelope.to)));
        }
        let datagram = self.datagram_for(&envelope.request)?;
        let link = self.link.as_ref().ok_or(PortError::Disconnected)?;
        // Recorded before sending: a reply can arrive before `send_to` returns.
        self.in_flight = Some((envelope.tag, envelope.request));
        match link.socket.send_to(&datagram, self.cfg.peer) {
            Ok(n) if n == datagram.len() => Ok(()),
            Ok(n) => Err(PortError::Other(format!("short datagram write: {n} of {}", datagram.len()))),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Err(PortError::QueueFull),
            Err(e) => Err(io_error(&e)),
        }
    }

    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
        loop {
            let link = self.link.as_ref().ok_or(PortError::Disconnected)?;
            let datagram = match link.rx.try_recv() {
                Ok(d) => d,
                Err(TryRecvError::Empty) => return Ok(None),
                Err(TryRecvError::Disconnected) => return Err(PortError::Disconnected),
            };
            let Some((tag, request)) = self.in_flight.clone() else {
                bump(&self.counters.unattributed);
                continue;
            };
            if let Some(reply) = self.reply_for(&request, &datagram) {
                return Ok(Some(Incoming { from: self.machine.clone(), tag, reply }));
            }
        }
    }

    /// New socket, new thread, empty queue. Sends nothing.
    fn reconnect(&mut self) -> Result<(), PortError> {
        if let Some(mut old) = self.link.take() {
            old.shutdown();
        }
        self.in_flight = None;
        self.link = Some(spawn_link(&self.cfg, &self.counters)?);
        Ok(())
    }
}

impl Drop for RuidaUdpPort {
    fn drop(&mut self) {
        if let Some(link) = self.link.as_mut() {
            link.shutdown();
        }
    }
}
