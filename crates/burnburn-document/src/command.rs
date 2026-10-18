use std::fmt;

use burnburn_geometry::{Affine, GeometryError, Rect};
use burnburn_types::{DocumentId, GeometryId, Length, OperationId, RequestId, RevisionId};

use crate::engine::EngineError;
use crate::generator::{GeneratorCommand, GeneratorId, GeneratorLabel};
use crate::process::{ProcessCommand, ProcessError, ProcessLabel, RecipeId};
use crate::sketch::{SketchCommand, SketchError, SketchId, SketchLabel};
use crate::structure::{
    BreakReason, EdgeName, RefHolder, ReferenceId, StructureChange, StructureCommand, StructureError, StructureLabel,
};
use crate::text::{TextCommand, TextError, TextId, TextLabel};
use crate::trace::{TraceCommand, TraceId, TraceLabel};

/// A request to change one document, valid only against the revision it was made for.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentCommand {
    pub document: DocumentId,
    /// Optimistic concurrency: must equal the session's current revision.
    pub expected: RevisionId,
    pub request: RequestId,
    pub payload: CommandPayload,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CommandPayload {
    CreateRectangle {
        rect: Rect,
        name: String,
    },
    /// Resize one object's bounding box along `axis` to `value`, keeping `anchor` fixed.
    SetDimension {
        target: GeometryId,
        axis: Axis,
        value: Length,
        anchor: Anchor,
    },
    /// One history entry for all targets (for example one drag commit).
    Transform {
        targets: Vec<GeometryId>,
        transform: Affine,
    },
    Delete {
        targets: Vec<GeometryId>,
    },
    Rename {
        target: GeometryId,
        name: String,
    },
    /// A process-layer edit (operations, order constraints, recipes; IM13).
    Process(ProcessCommand),
    /// A sketch definition edit (IM11). With `apply_with`, derived geometry follows in the same transaction.
    Sketch(SketchCommand),
    /// Drawing, node, boolean, repair, arrangement, group and import edits (IM09/IM10).
    Edit(crate::edit::GeometryCommand),
    /// Modifiers, references, relations, parts and plates (IM21).
    Structure(StructureCommand),
    /// A text object edit (IM18). With `apply_with`, the outlines follow in the same transaction.
    Text(TextCommand),
    /// The result of an asynchronous evaluation (contract §11). Refused with `StalePatch` when
    /// the owner's inputs changed since the evaluation started; an edit elsewhere is no reason.
    ApplyPatch(crate::DocumentPatch),
    /// A trace result applied, re-traced, moved or removed (C-9).
    Trace(TraceCommand),
    /// A generator definition edit (IM22). With `apply_with`, the outputs follow in the same transaction.
    Generator(GeneratorCommand),
    /// Parametric basic shapes: create, edit parameters, convert to path (W1-L2b).
    Shape(crate::shape_edit::ShapeCommand),
}

impl CommandPayload {
    pub fn undo_label(&self) -> UndoLabel {
        match self {
            CommandPayload::CreateRectangle { .. } => UndoLabel::CreateRectangle,
            CommandPayload::SetDimension { .. } => UndoLabel::SetDimension,
            CommandPayload::Transform { .. } => UndoLabel::Transform,
            CommandPayload::Delete { .. } => UndoLabel::Delete,
            CommandPayload::Rename { .. } => UndoLabel::Rename,
            CommandPayload::Process(p) => UndoLabel::Process(p.label()),
            CommandPayload::Sketch(s) => UndoLabel::Sketch(s.label()),
            CommandPayload::Edit(e) => UndoLabel::Edit(e.label()),
            CommandPayload::Structure(s) => UndoLabel::Structure(s.label()),
            CommandPayload::Text(t) => UndoLabel::Text(t.label()),
            CommandPayload::ApplyPatch(_) => UndoLabel::ApplyPatch,
            CommandPayload::Trace(t) => UndoLabel::Trace(t.label()),
            CommandPayload::Generator(g) => UndoLabel::Generator(g.label()),
            CommandPayload::Shape(s) => UndoLabel::Shape(s.label()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    Width,
    Height,
}

/// Which side of the bounding box stays fixed when resizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anchor {
    Min,
    Center,
    Max,
}

/// Localisation key for "Undo …" / "Redo …" menu items. Presentation translates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UndoLabel {
    CreateRectangle,
    SetDimension,
    Transform,
    Delete,
    Rename,
    Process(ProcessLabel),
    Sketch(SketchLabel),
    Edit(crate::edit::EditLabel),
    Structure(StructureLabel),
    Text(TextLabel),
    ApplyPatch,
    Trace(TraceLabel),
    Generator(GeneratorLabel),
    Shape(crate::shape_edit::ShapeLabel),
}

/// Which objects a committed command (or undo/redo) created, modified or deleted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChangeSet {
    pub created: Vec<GeometryId>,
    pub modified: Vec<GeometryId>,
    pub deleted: Vec<GeometryId>,
    /// Operations created, modified or removed (process layer).
    pub operations: Vec<OperationId>,
    /// Material recipes created or modified.
    pub recipes: Vec<RecipeId>,
    /// Sketch definitions created, modified or deleted.
    pub sketches: Vec<SketchId>,
    /// Groups created or dissolved (membership changes also list the objects as modified).
    pub groups: Vec<burnburn_types::GroupId>,
    /// Modifiers, references, relations, parts and plates created, changed or removed (IM21).
    pub structure: Vec<StructureChange>,
    pub texts: Vec<TextId>,
    pub traces: Vec<TraceId>,
    pub generators: Vec<GeneratorId>,
}

impl ChangeSet {
    pub fn is_empty(&self) -> bool {
        self.created.is_empty()
            && self.modified.is_empty()
            && self.deleted.is_empty()
            && self.operations.is_empty()
            && self.recipes.is_empty()
            && self.sketches.is_empty()
            && self.groups.is_empty()
            && self.structure.is_empty()
            && self.texts.is_empty()
            && self.traces.is_empty()
            && self.generators.is_empty()
    }
}

/// A non-fatal remark about a committed command.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CommandWarning {
    /// Another object already has this name; names are labels, not identities.
    DuplicateName { name: String, other: GeometryId },
    /// A reference that resolved before this command no longer does (contract §11): nothing was
    /// rebound; the user decides (rebind, remove).
    ReferenceBroken { holder: RefHolder, reason: BreakReason },
    /// A reference whose origin now yields several edges; the user must choose one.
    ReferenceAmbiguous { reference: ReferenceId, candidates: Vec<EdgeName> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandOutcome {
    pub previous: RevisionId,
    pub revision: RevisionId,
    pub changes: ChangeSet,
    pub warnings: Vec<CommandWarning>,
    pub undo_label: UndoLabel,
}

/// Why a value in a command was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValueError {
    /// Must be strictly positive.
    NotPositive,
    /// The object has zero extent along the axis, so it cannot be scaled to a size.
    ZeroExtent,
    /// The object has no geometry.
    Empty,
    Geometry(GeometryError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CommandError {
    StaleRevision {
        expected: RevisionId,
        actual: RevisionId,
    },
    WrongDocument {
        expected: DocumentId,
        actual: DocumentId,
    },
    UnknownGeometry(GeometryId),
    InvalidValue {
        field: &'static str,
        reason: ValueError,
    },
    IdsExhausted,
    RevisionsExhausted,
    NothingToUndo,
    NothingToRedo,
    /// A process-layer command was rejected (IM13).
    Process(ProcessError),
    /// A sketch command was rejected (IM11).
    Sketch(SketchError),
    /// An evaluator patch for an owner that does not exist (any more).
    UnknownOwner(crate::DerivedOwner),
    /// An evaluator patch is invalid (for example a solution that does not fit the sketch).
    InvalidPatch(crate::DerivedOwner),
    /// A geometry edit was rejected (IM09); nothing changed.
    Edit(crate::edit::EditError),
    /// A structure command was rejected (IM21); nothing changed.
    Structure(StructureError),
    /// An `ApplyPatch` computed from inputs that are no longer current (contract §11).
    StalePatch {
        owner: crate::DerivedOwner,
    },
    /// Derived geometry cannot be transformed, resized or node-edited directly (C-8): edit its
    /// definition (sketch, modifier, part, text) or detach it first.
    DerivedObject {
        owner: crate::DerivedOwner,
    },
    Text(TextError),
    /// A trace or generator command was rejected (C-9, IM22); nothing changed.
    Engine(EngineError),
    /// A parametric shape command was rejected (W1-L2b); nothing changed.
    Shape(crate::shape_edit::ShapeError),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::StaleRevision { expected, actual } => {
                write!(f, "command expected revision {expected} but the document is at {actual}")
            }
            CommandError::WrongDocument { expected, actual } => {
                write!(f, "command for document {expected} sent to {actual}")
            }
            CommandError::UnknownGeometry(id) => write!(f, "no geometry object {id}"),
            CommandError::InvalidValue { field, reason } => write!(f, "invalid {field}: {reason:?}"),
            CommandError::IdsExhausted => f.write_str("geometry ids exhausted"),
            CommandError::RevisionsExhausted => f.write_str("document revisions exhausted"),
            CommandError::NothingToUndo => f.write_str("nothing to undo"),
            CommandError::NothingToRedo => f.write_str("nothing to redo"),
            CommandError::Process(e) => write!(f, "process: {e}"),
            CommandError::Sketch(e) => write!(f, "sketch: {e}"),
            CommandError::UnknownOwner(o) => write!(f, "no derived-geometry owner {o:?}"),
            CommandError::InvalidPatch(o) => write!(f, "invalid patch for {o:?}"),
            CommandError::Edit(e) => write!(f, "edit: {e}"),
            CommandError::Structure(e) => write!(f, "structure: {e}"),
            CommandError::StalePatch { owner } => write!(f, "patch for {owner:?} was computed from older inputs"),
            CommandError::DerivedObject { owner } => {
                write!(f, "derived geometry of {owner:?} is edited through its definition")
            }
            CommandError::Text(e) => write!(f, "text: {e}"),
            CommandError::Engine(e) => write!(f, "{e}"),
            CommandError::Shape(e) => write!(f, "shape: {e}"),
        }
    }
}

impl std::error::Error for CommandError {}
