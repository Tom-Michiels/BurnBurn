//! The immutable, content-addressed [`JobPlan`] (IM13, ADR 0001).
//!
//! A plan is the single compiled artifact that preview, RD export and upload all read. It is
//! built once through [`JobPlanBuilder`], validated event by event, and frozen by
//! [`JobPlanBuilder::finish`], which computes its [`JobPlanId`] from a canonical projection of
//! the content. There are no public fields and no mutators, so a document undo can never touch
//! a frozen plan. The plan does not depend on the document crate: it carries [`SourceRef`]s
//! (geometry + operation ids) for traceability, nothing more.
//!
//! Units are fixed: positions in machine-frame millimetres (`f64`), speeds in mm/s, power as a
//! ratio of the source's rated power. Quantisation happens only in a backend (for example
//! `burnburn_ruida::lower`).

mod capability;
mod event;
mod frame;
mod plan;
mod preview;
mod profile;
mod rotary;

pub use capability::{Feature, MachineCapabilities, Support};
pub use event::{
    MachinePoint, OperationSettings, PlanError, PlanEvent, PlanMode, PowerRatio, ScanAxis, ScanLine, SettingsIndex,
    SourceRef,
};
pub use frame::{MachineFrame, OriginCorner};
pub use plan::{JobPlan, JobPlanBuilder, JobPlanId, PlanSource, PlanUnits};
pub use preview::{MoveKind, PlanPreview, PreviewMove, preview};
pub use profile::{MachineProfile, ProfileRevision};
pub use rotary::{RotaryConfig, RotaryKind};
