//! Stable identities, physical units, revisions and typed errors.
//!
//! This crate has no GUI, GPU, filesystem or network dependency.

mod angle;
mod cancel;
mod canonical;
mod hash;
mod id;
mod length;
mod revision;
mod time;
mod units;

pub use angle::{Angle, AngleError};
pub use cancel::{CancellationSource, CancellationToken, Cancelled};
pub use canonical::{CanonicalEncode, CanonicalWriter, NonFiniteValue};
pub use hash::{ContentHash, HashDomain, HashParseError};
pub use id::{DocumentId, GeometryId, GroupId, IdAllocator, IdExhausted, OperationId, RequestId, StableId, TaskId};
pub use length::{Length, LengthError};
pub use revision::RevisionId;
pub use time::{Clock, MonotonicTime};
pub use units::Unit;
