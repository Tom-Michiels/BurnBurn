//! The transport port. Adapters (fake in tests, UDP later in `burnburn-runtime`) translate these
//! intents to Ruida bytes via `burnburn-ruida` and attribute each reply to the request it answers.

use std::fmt;

use crate::ids::{ControllerIdentity, ExecutionId, MachineId};

/// Protocol-level intents. Byte encoding is the adapter's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Ask the controller who it is (model/firmware), compared with the profile before any
    /// upload. Idempotent.
    QueryIdentity,
    /// One chunk of an already compiled, scrambled job file. Not idempotent: a resend after a
    /// lost ACK may duplicate job data.
    UploadChunk { index: usize, bytes: Vec<u8> },
    /// Ask whether the uploaded job is stored on the controller (read-back). Idempotent.
    QueryStored,
    /// Start the stored job. Never idempotent; never retried.
    Start,
    /// Read controller status. Idempotent.
    QueryStatus,
    /// Stop the running job. Not retried automatically.
    Stop,
}

impl Request {
    /// Only queries may be resent automatically.
    pub fn is_idempotent(&self) -> bool {
        matches!(self, Request::QueryIdentity | Request::QueryStored | Request::QueryStatus)
    }
}

/// Correlates a reply with the request it answers: execution, connection epoch and sequence.
/// Ruida has no sequence numbers on the wire; the adapter keeps one request in flight and tags
/// the reply with the tag of that request. A reconnect starts a new epoch, so late replies from
/// before the reconnect never match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestTag {
    pub execution: ExecutionId,
    pub epoch: u32,
    pub seq: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub to: MachineId,
    pub tag: RequestTag,
    pub request: Request,
}

/// Controller status as interpreted by the adapter. Status bits carry no job identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerStatus {
    Idle,
    Running,
    /// The controller reports that the job finished.
    Finished,
    Fault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// Transport-level receipt. Proves receipt of one request only.
    Ack,
    Nak,
    Err,
    /// Answer to [`Request::QueryIdentity`].
    Identity(ControllerIdentity),
    /// Answer to [`Request::QueryStored`].
    Stored(bool),
    /// Answer to [`Request::QueryStatus`].
    Status(ControllerStatus),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    pub from: MachineId,
    pub tag: RequestTag,
    pub reply: Reply,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortError {
    Disconnected,
    /// The transport has no (verified) way to carry this request or connection.
    NotSupported(String),
    /// The adapter's bounded queue is full; nothing was sent.
    QueueFull,
    Other(String),
}

impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortError::Disconnected => write!(f, "disconnected"),
            PortError::NotSupported(m) => write!(f, "not supported: {m}"),
            PortError::QueueFull => write!(f, "transport queue full"),
            PortError::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for PortError {}

/// Non-blocking transport port. Implementations must not block in `try_recv`.
pub trait ControllerPort {
    fn send(&mut self, envelope: Envelope) -> Result<(), PortError>;
    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError>;
    fn reconnect(&mut self) -> Result<(), PortError>;
}

/// Lets hosts choose the transport at run time (`Box<dyn ControllerPort>`).
impl<P: ControllerPort + ?Sized> ControllerPort for Box<P> {
    fn send(&mut self, envelope: Envelope) -> Result<(), PortError> {
        (**self).send(envelope)
    }

    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
        (**self).try_recv()
    }

    fn reconnect(&mut self) -> Result<(), PortError> {
        (**self).reconnect()
    }
}
