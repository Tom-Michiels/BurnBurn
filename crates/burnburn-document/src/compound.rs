//! Samengestelde commando's (W4-UI-machine): meerdere bewerkingen als één transactie, dus één
//! revisie en één undo-stap. Beeld importeren (kader + operatie) en Mask Image (toewijzen,
//! loskoppelen, bijwerken) zijn de eerste gebruikers.

use crate::command::CommandPayload;
use crate::process::OperationDraft;

/// Undo/redo label (localisation key) of a compound command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CompoundLabel {
    /// A bitmap frame plus its image operation.
    ImportImage,
    /// Mask Image: the shapes become the targets and the settings are updated.
    MaskImage,
    /// A LightBurn project: shapes, layer operations and bitmaps in one step (P04).
    ImportProject,
    /// An import with the layers its file proposed: the objects and one operation per layer (P04).
    ImportLayers,
    /// Replace to Fit: the frame takes the shape of the new picture, and the pixels change (G7-trace, T09).
    ReplaceToFit,
    /// One text edit applied to several text objects at once (G7-editor, E07).
    EditTexts,
    /// Convert to Path of several texts at once (G8-editor, E07).
    ConvertTexts,
    /// Every instance without a plate onto a plate where it fits (G8-editor, P06).
    DistributeOverPlates,
}

/// One step of a compound command.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CompoundStep {
    /// An ordinary command, applied to the working copy of the transaction.
    Run(CommandPayload),
    /// Adds an operation that targets the geometry created by the earlier steps of this compound
    /// (the caller cannot know those ids in advance).
    AddOperationForCreated { draft: OperationDraft },
    /// Adds an operation that targets some of the geometry created so far by this compound, by
    /// position in creation order (an import maps its items to layers before the ids exist).
    AddOperationForItems { draft: OperationDraft, items: Vec<usize> },
}

/// Several steps applied as one transaction: all of them or none.
#[derive(Debug, Clone, PartialEq)]
pub struct CompoundCommand {
    pub label: CompoundLabel,
    pub steps: Vec<CompoundStep>,
}

/// Why a compound command was refused (nothing changed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompoundError {
    /// A compound without steps.
    Empty,
    /// A compound inside a compound.
    Nested,
    /// `AddOperationForCreated` without geometry created by an earlier step.
    NothingCreated,
    /// `AddOperationForItems` names an item no earlier step created, or none.
    UnknownItem,
}
