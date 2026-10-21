//! The GRBL 1.1 serial protocol as pure data and state machines (no IO).
//!
//! Everything here follows the public GRBL 1.1 interface description. None of it has been
//! observed on a real controller (`docs/hardware/evidence.md`).

mod codes;
mod message;
mod prepare;
mod session;

pub use codes::{alarm_description, error_description};
pub use message::{MachineState, Message, ParseError, Position, StatusReport, parse_line};
pub use prepare::{MAX_LINE_BYTES, PrepareError, PreparedJob, prepare_job};
pub use session::{
    Command, CommandRefusal, GrblSession, JobFailure, JobProgress, JobState, JogTarget, REALTIME_HOLD,
    REALTIME_JOG_CANCEL, REALTIME_RESUME, REALTIME_SOFT_RESET, REALTIME_STATUS, RX_BUFFER_BYTES, SessionConfig,
    SessionEvent, StartRefusal, Step, UnknownReason,
};
