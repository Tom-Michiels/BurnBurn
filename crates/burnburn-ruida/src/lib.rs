//! Pure Ruida RD codec (IM14).
//!
//! No sockets, clocks or filesystem: bytes in, typed records out, and back. The confirmed subset
//! follows `research/lightburn/ruida/02-binaire-specificatie.md` and the three LightBurn fixtures.
//! Command names from the public registry are labels, not proof of firmware semantics.
//!
//! Layers:
//! - [`scramble`]/[`unscramble`]: the reversible byte permutation with a configurable [`Magic`].
//! - [`field`]: 7-bit payload fields with range-checked types.
//! - [`decode`]: unscrambled record stream → [`Record`]s; never panics, bounded input.
//! - [`decode_rd_file`]/[`encode_rd_file`]: whole `.rd` files with file sum and end record.
//! - [`udp`]: the OSS-described datagram checksum framing (pure; no socket).
//! - [`lower`]: `burnburn_job::JobPlan` → [`RdProgram`] (owner CAM/job role, DR-09).

mod command;
mod decode;
mod encode;
pub mod field;
mod file;
mod lower;
mod registry;
pub use registry::registry_entries;
mod scramble;
pub mod udp;

pub use command::{
    BoundsKind, Command, DelayEdge, Laser, ModeFlag, PartBoundsKind, PowerBound, ProcessAction, RefPoint, ReplyByte,
};
pub use decode::{DecodeError, MAX_DECODE_INPUT, Malformation, Record, decode};
pub use encode::{EncodeError, encode, encode_command};
pub use field::{AbsCoord, Byte7, FieldError, Power, RelCoord, Speed, U35};
pub use file::{FileError, RdFile, decode_rd_file, encode_rd_file, file_sum};
pub use lower::{LowerError, OverscanPolicy, RdProgram, RuidaProfile, lower};
pub use scramble::{Magic, scramble, scramble_byte, unscramble, unscramble_byte};
pub use udp::UdpError;
