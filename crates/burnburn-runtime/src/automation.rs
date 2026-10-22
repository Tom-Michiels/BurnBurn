//! Desktop automation over loopback UDP (API01).
//!
//! A small, opt-in, line-oriented protocol with which another program on the *same computer* can
//! ask the application to open a project, import a drawing file or report its state. The scope
//! is documented in `docs/design/desktop-automatisering.md`:
//!
//! - one datagram is one request, one datagram is exactly one reply to the sender;
//! - only `PING`, `STATUS`, `LOADFILE:<path>` and `IMPORT:<path>` exist; machine verbs are
//!   answered with `ERR:FORBIDDEN` and the [`AutomationHost`] trait has no machine method at all;
//! - a changed document is never replaced silently: `LOADFILE` answers `ERR:DIRTY` and there is
//!   no force variant;
//! - the endpoint only accepts a socket bound to loopback and drops datagrams from other
//!   addresses; a disabled service does not answer at all (preference "Desktop-automatisering",
//!   off by default).
//!
//! The protocol is BurnBurn's own. It is modelled on the verbs of the LightBurn automation
//! interface, but byte-for-byte compatibility with LightBurn is not claimed or verified.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Component, Path, PathBuf};

/// The port the application listens on when automation is on (not LightBurn's, so both programs
/// can run side by side).
pub const DEFAULT_PORT: u16 = 19850;
/// Longest request accepted, in bytes.
pub const MAX_REQUEST: usize = 1100;
/// Longest path accepted, in bytes.
pub const MAX_PATH_BYTES: usize = 1024;
/// Extensions `LOADFILE` accepts (lower case).
pub const LOAD_EXTENSIONS: &[&str] = &["burnburn", "lbrn2", "lbrn"];
/// Extensions `IMPORT` accepts (lower case).
pub const IMPORT_EXTENSIONS: &[&str] = &["svg", "dxf", "png", "jpg", "jpeg", "bmp", "tif", "tiff"];
/// Verbs that act on a machine. They exist only to be refused with a clear reply.
pub const MACHINE_VERBS: &[&str] = &[
    "START", "FRAME", "JOG", "HOME", "SEND", "UPLOAD", "DOWNLOAD", "STOP", "PAUSE", "RESUME", "RUN", "BURN", "CONNECT",
    "MOVE", "ORIGIN", "FIRE", "TEST", "CANCEL", "ABORT", "RESET",
];

/// What the application tells the automation layer about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostStatus {
    pub has_document: bool,
    /// The open document has changes that are not saved.
    pub dirty: bool,
    /// The application is busy (for instance compiling or exporting a job).
    pub busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostError {
    NotFound,
    /// The file is not of a kind the application can open or import.
    Unsupported,
    /// The document changed after the service looked: it is not replaced.
    Dirty,
    Failed,
}

/// The application side. Deliberately only document operations: there is no way to reach a
/// machine, a job or an upload through this trait.
pub trait AutomationHost {
    fn status(&self) -> HostStatus;
    /// Opens the project at `path` in place of the current document. Called only when the
    /// current document has no unsaved changes.
    fn load(&mut self, path: &Path) -> Result<(), HostError>;
    /// Adds the drawing file at `path` to the current document (one undo step).
    fn import(&mut self, path: &Path) -> Result<(), HostError>;
}

/// A request that passed syntax and path checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Ping,
    Status,
    Load(PathBuf),
    Import(PathBuf),
}

/// Why a request was not turned into a [`Command`] or not carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Unknown,
    Malformed,
    Forbidden,
    Path,
    Busy,
    Dirty,
    NoDocument,
    NotFound,
    Format,
    Failed,
}

impl Refusal {
    /// The reply code after `ERR:`.
    pub fn code(self) -> &'static str {
        match self {
            Refusal::Unknown => "UNKNOWN",
            Refusal::Malformed => "MALFORMED",
            Refusal::Forbidden => "FORBIDDEN",
            Refusal::Path => "PATH",
            Refusal::Busy => "BUSY",
            Refusal::Dirty => "DIRTY",
            Refusal::NoDocument => "NODOC",
            Refusal::NotFound => "NOTFOUND",
            Refusal::Format => "FORMAT",
            Refusal::Failed => "FAILED",
        }
    }
}

/// Parses one request datagram.
pub fn parse(datagram: &[u8]) -> Result<Command, Refusal> {
    if datagram.len() > MAX_REQUEST {
        return Err(Refusal::Malformed);
    }
    let text = std::str::from_utf8(datagram).map_err(|_| Refusal::Malformed)?;
    let text = text.trim_end_matches(['\r', '\n']);
    let (verb, argument) = match text.split_once(':') {
        Some((verb, argument)) => (verb, Some(argument)),
        None => (text, None),
    };
    let verb = verb.trim().to_ascii_uppercase();
    if MACHINE_VERBS.contains(&verb.as_str()) {
        return Err(Refusal::Forbidden);
    }
    match (verb.as_str(), argument) {
        ("PING", None) => Ok(Command::Ping),
        ("STATUS", None) => Ok(Command::Status),
        ("PING" | "STATUS", Some(_)) => Err(Refusal::Malformed),
        ("LOADFILE", Some(p)) => Ok(Command::Load(checked_path(p, LOAD_EXTENSIONS)?)),
        ("IMPORT", Some(p)) => Ok(Command::Import(checked_path(p, IMPORT_EXTENSIONS)?)),
        ("LOADFILE" | "IMPORT", None) => Err(Refusal::Malformed),
        _ => Err(Refusal::Unknown),
    }
}

fn checked_path(text: &str, extensions: &[&str]) -> Result<PathBuf, Refusal> {
    if text.is_empty() || text.len() > MAX_PATH_BYTES || text.chars().any(char::is_control) {
        return Err(Refusal::Path);
    }
    let path = Path::new(text);
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(Refusal::Path);
    }
    let extension = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    match extension {
        Some(e) if extensions.contains(&e.as_str()) => Ok(path.to_path_buf()),
        _ => Err(Refusal::Path),
    }
}

fn reply(result: Result<String, Refusal>) -> Vec<u8> {
    match result {
        Ok(text) => text.into_bytes(),
        Err(refusal) => format!("ERR:{}", refusal.code()).into_bytes(),
    }
}

/// The protocol on top of an [`AutomationHost`], independent of any socket.
pub struct AutomationService<H: AutomationHost> {
    host: H,
    enabled: bool,
}

impl<H: AutomationHost> AutomationService<H> {
    pub fn new(host: H, enabled: bool) -> Self {
        AutomationService { host, enabled }
    }

    /// The preference "Desktop-automatisering". Off: no reply to anything.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn host(&self) -> &H {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }

    /// The reply to one request, or `None` when automation is off.
    pub fn handle(&mut self, datagram: &[u8]) -> Option<Vec<u8>> {
        if !self.enabled {
            return None;
        }
        Some(reply(parse(datagram).and_then(|command| self.run(command))))
    }

    fn run(&mut self, command: Command) -> Result<String, Refusal> {
        let status = self.host.status();
        match command {
            Command::Ping => Ok("OK:PONG".to_owned()),
            Command::Status => {
                let state = if status.busy { "BUSY" } else { "READY" };
                let document = match (status.has_document, status.dirty) {
                    (false, _) => "NODOC",
                    (true, true) => "DIRTY",
                    (true, false) => "CLEAN",
                };
                Ok(format!("OK:{state}:{document}"))
            }
            Command::Load(path) => {
                if status.busy {
                    return Err(Refusal::Busy);
                }
                if status.has_document && status.dirty {
                    return Err(Refusal::Dirty);
                }
                self.host.load(&path).map(|()| "OK".to_owned()).map_err(host_refusal)
            }
            Command::Import(path) => {
                if status.busy {
                    return Err(Refusal::Busy);
                }
                if !status.has_document {
                    return Err(Refusal::NoDocument);
                }
                self.host.import(&path).map(|()| "OK".to_owned()).map_err(host_refusal)
            }
        }
    }
}

fn host_refusal(e: HostError) -> Refusal {
    match e {
        HostError::NotFound => Refusal::NotFound,
        HostError::Unsupported => Refusal::Format,
        HostError::Dirty => Refusal::Dirty,
        HostError::Failed => Refusal::Failed,
    }
}

/// The datagram socket the endpoint needs (a fake in tests).
pub trait DatagramSocket {
    fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)>;
    fn send_to(&mut self, data: &[u8], to: SocketAddr) -> io::Result<usize>;
    fn local_addr(&self) -> io::Result<SocketAddr>;
}

impl DatagramSocket for UdpSocket {
    fn recv_from(&mut self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        UdpSocket::recv_from(self, buf)
    }

    fn send_to(&mut self, data: &[u8], to: SocketAddr) -> io::Result<usize> {
        UdpSocket::send_to(self, data, to)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        UdpSocket::local_addr(self)
    }
}

/// What [`AutomationEndpoint::serve_one`] did with one datagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Served {
    Replied,
    /// Automation is off: nothing was answered.
    Silent,
    /// The sender is not on this computer: nothing was executed or answered.
    DroppedNonLoopback,
}

#[derive(Debug)]
pub enum EndpointError {
    /// The socket is bound to an address other than loopback.
    NotLoopback,
    Io(io::Error),
}

impl std::fmt::Display for EndpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndpointError::NotLoopback => f.write_str("the automation socket must be bound to loopback"),
            EndpointError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EndpointError {}

/// A [`AutomationService`] on a loopback socket.
pub struct AutomationEndpoint<S: DatagramSocket, H: AutomationHost> {
    socket: S,
    service: AutomationService<H>,
}

impl<S: DatagramSocket, H: AutomationHost> AutomationEndpoint<S, H> {
    /// Refuses a socket that is not bound to a loopback address.
    pub fn new(socket: S, service: AutomationService<H>) -> Result<Self, EndpointError> {
        let local = socket.local_addr().map_err(EndpointError::Io)?;
        if !local.ip().is_loopback() {
            return Err(EndpointError::NotLoopback);
        }
        Ok(AutomationEndpoint { socket, service })
    }

    /// Receives one datagram and answers it. A receive error (timeout, would-block) is returned
    /// as `Io` for the caller's loop to interpret.
    pub fn serve_one(&mut self) -> Result<Served, EndpointError> {
        let mut buf = [0u8; MAX_REQUEST * 2];
        let (n, from) = self.socket.recv_from(&mut buf).map_err(EndpointError::Io)?;
        if !from.ip().is_loopback() {
            return Ok(Served::DroppedNonLoopback);
        }
        let request = buf.get(..n).unwrap_or(&[]);
        match self.service.handle(request) {
            Some(answer) => {
                self.socket.send_to(&answer, from).map_err(EndpointError::Io)?;
                Ok(Served::Replied)
            }
            None => Ok(Served::Silent),
        }
    }

    pub fn service(&self) -> &AutomationService<H> {
        &self.service
    }

    pub fn service_mut(&mut self) -> &mut AutomationService<H> {
        &mut self.service
    }

    pub fn into_socket(self) -> S {
        self.socket
    }
}

/// Binds a UDP socket on 127.0.0.1 only (`port` 0 picks a free one).
pub fn bind_loopback(port: u16) -> io::Result<UdpSocket> {
    UdpSocket::bind((Ipv4Addr::LOCALHOST, port))
}
