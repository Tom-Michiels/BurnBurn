//! GRBL 1.1 G-code backend (W4-L4).
//!
//! - [`lower`]: `burnburn_job::JobPlan` -> [`GcodeProgram`], with an explicit capability matrix
//!   ([`grbl_capabilities`]). Features outside the matrix are refused with a typed reason, never
//!   dropped or replaced.
//! - [`grbl`]: the streaming protocol as a pure state machine (character counting, `ok`/`error`/
//!   `ALARM`, status reports, realtime commands); no IO.
//! - [`serial`]: the [`SerialPort`] trait and a driver that couples a port with a session.
//! - [`console`]: command history (M212) and named macros (M216).
//!
//! Nothing here contacts a machine. Whether a concrete GRBL controller behaves as modelled is
//! **unverified** (`docs/hardware/evidence.md`).

mod lower;
mod number;
mod profile;

pub use lower::{GcodeProgram, LowerError, Refusal, RefusalKind, check, lower};
pub use profile::{GrblProfile, PowerMode, ProfileError, WorkCoordinate, grbl_capabilities};
