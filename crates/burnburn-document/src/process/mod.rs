//! The process layer (IM13): operations, references to design geometry, order constraints and
//! material recipes. Owner: CAM/job role.
//!
//! Design and process are separate: an [`Operation`] refers to geometry by [`GeometryId`]
//! and never copies it, so one shape can be marked and cut by two operations, and an edit of
//! the shape reaches both. Every change goes through [`ProcessCommand`] as one document
//! transaction, so undo/redo and revisions behave exactly like geometry edits.
//!
//! [`GeometryId`]: burnburn_types::GeometryId

mod advanced;
mod canonical;
mod command;
mod error;
mod layer;
mod operation;
mod recipe;
mod settings;

pub use advanced::{
    AdvancedOptions, DotMode, Lead, LeadStyle, LineOptions, Overcut, Perforation, SharedEdgePolicy, SubOperation,
    TabPlacement, TabSpec, ZStep,
};
pub use command::{ProcessCommand, ProcessLabel};
pub(crate) use command::{ProcessTxn, apply};
pub use error::ProcessError;
pub use layer::{OrderConstraint, ProcessLayer};
pub use operation::{Operation, OperationDraft};
pub use recipe::{MaterialRecipe, RecipeDraft, RecipeId, RecipeRef};
pub use settings::{
    DitherMode, FillGrouping, FillSettings, ImageRender, ImageSettings, Kerf, OffsetFillOrder, OffsetFillSettings,
    OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, ScanAxis, ScanDirection, Speed, ToneSettings,
};
