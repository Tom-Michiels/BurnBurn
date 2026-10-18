//! Machine execution state machine (IM16, first part). No live IO.
//!
//! An [`ExecutionActor`] drives one job execution on one identified machine through a
//! [`ControllerPort`]. Every state change is backed by recorded [`Evidence`]; an ACK is never
//! read as "stored" or "completed", and a start whose outcome is uncertain becomes
//! [`ExecutionState::Unknown`] and is never sent again automatically.
//!
//! Timeouts use the injected [`burnburn_types::Clock`]. Only idempotent requests (queries) are
//! retried, and only a bounded number of times.

mod actor;
mod artifact;
pub mod calibration;
pub mod controller_files;
mod frame;
mod ids;
mod port;
pub mod settings;
mod state;

pub use actor::{ActorError, ExecutionActor, Policy};
pub use artifact::{ArtifactError, RdArtifact, rd_hash};
pub use calibration::{CalibratedProfile, CalibrationError, CalibrationSet, MaterialKey, ScanOffsetPoint};
pub use controller_files::{
    Capability, ControllerFileIntent, ControllerFileName, FileCapabilities, FileOpError, authorize,
};
pub use frame::{FRAME_GENERATOR_VERSION, FrameError, FrameOptions, FramePlan, frame_operation, frame_plan};
pub use ids::{ControllerIdentity, ExecutionId, MachineId, MachineIdentity};
pub use port::{ControllerPort, ControllerStatus, Envelope, Incoming, PortError, Reply, Request, RequestTag};
pub use settings::{ControllerSettingsSnapshot, SettingChange, SettingKey, SettingValue, diff};
pub use state::{Evidence, EvidenceEntry, Execution, ExecutionState, FailReason, UnknownReason};
