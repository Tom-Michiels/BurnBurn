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
mod optimize;
mod recipe;
mod settings;

pub use advanced::{
    AdvancedOptions, DotMode, Lead, LeadStyle, LineOptions, Overcut, Perforation, SharedEdgePolicy, SubOperation,
    TabPlacement, TabSpec, ToolLayer, ZOffset, ZStep,
};
pub use command::{ProcessCommand, ProcessLabel};
pub(crate) use command::{ProcessTxn, apply};
pub use error::ProcessError;
pub use layer::{OrderConstraint, ProcessLayer};
pub use operation::{Operation, OperationDraft};
pub use optimize::{CutPriority, OptimizationSettings, OrderKey, PathLock};
pub use recipe::{MaterialRecipe, RecipeDraft, RecipeId, RecipeRef};
pub use settings::{
    DitherMode, FillGrouping, FillSettings, GrayMethod, ImageCrop, ImageRender, ImageSettings, ImageSharpen, Kerf,
    OffsetFillOrder, OffsetFillSettings, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, ScanAxis,
    ScanDirection, ScreenSettings, Speed, ToneSettings,
};
