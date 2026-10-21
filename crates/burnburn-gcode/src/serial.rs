//! The serial transport port and a driver that couples a port with a [`GrblSession`].
//!
//! No real serial adapter exists in this workspace: opening a device needs OS-specific termios /
//! Win32 calls or a serial crate with a C toolchain dependency, which `docs/design/layers.toml`
//! forbids for core crates. A host adapter implements [`SerialPort`]; until then the port is
//! exercised only by the fake controller in the tests.

use std::fmt;

use crate::grbl::{Command, CommandRefusal, GrblSession, PreparedJob, SessionConfig, SessionEvent, StartRefusal, Step};

/// Longest controller line the driver buffers; longer input is discarded up to the next newline.
pub const MAX_RX_LINE_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerialError {
    Disconnected,
    Other(String),
}

impl fmt::Display for SerialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerialError::Disconnected => f.write_str("serial port disconnected"),
            SerialError::Other(m) => write!(f, "serial port: {m}"),
        }
    }
}

impl std::error::Error for SerialError {}

/// A byte-level, non-blocking serial connection to one controller.
pub trait SerialPort {
    /// Writes all bytes or fails. A failure leaves it unknown how much was written.
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), SerialError>;
    /// Appends the bytes that are available now (possibly none). Must not block.
    fn read_available(&mut self, out: &mut Vec<u8>) -> Result<(), SerialError>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum DriverError {
    Serial(SerialError),
    Start(StartRefusal),
    Command(CommandRefusal),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DriverError {}

impl From<SerialError> for DriverError {
    fn from(e: SerialError) -> Self {
        DriverError::Serial(e)
    }
}

pub struct GrblDriver<P: SerialPort> {
    port: P,
    session: GrblSession,
    rx: Vec<u8>,
    discarding: bool,
}

impl<P: SerialPort> GrblDriver<P> {
    pub fn new(port: P, config: SessionConfig) -> Self {
        GrblDriver { port, session: GrblSession::new(config), rx: Vec::new(), discarding: false }
    }

    pub fn session(&self) -> &GrblSession {
        &self.session
    }

    pub fn port(&self) -> &P {
        &self.port
    }

    /// Reads what the controller sent and feeds complete lines to the session. A failing read
    /// means the connection is gone: a running job becomes `Unknown`.
    pub fn pump(&mut self) -> Result<Vec<SessionEvent>, SerialError> {
        let mut bytes = Vec::new();
        if let Err(e) = self.port.read_available(&mut bytes) {
            self.session.transport_lost();
            return Err(e);
        }
        let mut events = Vec::new();
        for b in bytes {
            if b == b'\n' || b == b'\r' {
                if std::mem::take(&mut self.discarding) {
                    self.rx.clear();
                } else if !self.rx.is_empty() {
                    let line = String::from_utf8_lossy(&self.rx).into_owned();
                    self.rx.clear();
                    let step = self.session.on_line(&line);
                    events.extend(self.apply(step)?);
                }
            } else if !self.discarding {
                if self.rx.len() >= MAX_RX_LINE_BYTES {
                    self.discarding = true;
                    self.rx.clear();
                    events.push(SessionEvent::Unexpected(format!(
                        "controller line longer than {MAX_RX_LINE_BYTES} bytes discarded"
                    )));
                } else {
                    self.rx.push(b);
                }
            }
        }
        Ok(events)
    }

    pub fn start_job(&mut self, job: PreparedJob) -> Result<Vec<SessionEvent>, DriverError> {
        let step = self.session.start_job(job).map_err(DriverError::Start)?;
        Ok(self.apply(step)?)
    }

    pub fn poll_status(&mut self) -> Result<Vec<SessionEvent>, DriverError> {
        let step = self.session.poll_status();
        Ok(self.apply(step)?)
    }

    pub fn request_settings(&mut self) -> Result<Vec<SessionEvent>, DriverError> {
        let step = self.session.request_settings().map_err(DriverError::Command)?;
        Ok(self.apply(step)?)
    }

    pub fn hold(&mut self) -> Result<Vec<SessionEvent>, DriverError> {
        let step = self.session.hold();
        Ok(self.apply(step)?)
    }

    pub fn resume(&mut self) -> Result<Vec<SessionEvent>, DriverError> {
        let step = self.session.resume().map_err(DriverError::Command)?;
        Ok(self.apply(step)?)
    }

    pub fn cancel(&mut self) -> Result<Vec<SessionEvent>, DriverError> {
        let step = self.session.cancel();
        Ok(self.apply(step)?)
    }

    pub fn jog_cancel(&mut self) -> Result<Vec<SessionEvent>, DriverError> {
        let step = self.session.jog_cancel();
        Ok(self.apply(step)?)
    }

    pub fn command(&mut self, command: Command) -> Result<Vec<SessionEvent>, DriverError> {
        let step = self.session.command(command).map_err(DriverError::Command)?;
        Ok(self.apply(step)?)
    }

    /// Writes a step's bytes. A failed write may have delivered part of them: the session is told
    /// the connection is lost (a running job becomes `Unknown`) and nothing is written again.
    fn apply(&mut self, step: Step) -> Result<Vec<SessionEvent>, SerialError> {
        if !step.write.is_empty()
            && let Err(e) = self.port.write_all(&step.write)
        {
            self.session.transport_lost();
            return Err(e);
        }
        Ok(step.events)
    }
}
