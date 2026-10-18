//! Evaluator patches (contract §11): derived geometry from a definition (sketch now; trace and
//! text later), keyed so a `GeometryId` survives re-evaluation as long as its key does.
//!
//! First implemented for the Sketch evaluator (IM11); generic parts are Kern's to extend.

use burnburn_geometry::Path;
use burnburn_types::ContentHash;

use crate::sketch::{ConstraintId, SketchId, SketchSolution};
use crate::structure::{ModifierId, PartInstanceId, ProvenanceEdge};
use crate::{DocumentSnapshot, ObjectKind};

/// Who owns a piece of derived geometry. One owner regenerates only its own outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum DerivedOwner {
    Sketch(SketchId),
    /// A non-destructive modifier chain (IM21).
    Modifier(ModifierId),
    /// One placed instance of a part definition (IM21).
    Part(PartInstanceId),
    /// A text object's outlines (IM18, DR-22).
    Text(crate::text::TextId),
    /// A trace's contours (C-9); delivered asynchronously.
    Trace(crate::trace::TraceId),
    /// A generator's output (IM22, C-2).
    Generator(crate::generator::GeneratorId),
}

impl DerivedOwner {
    pub fn kind(self) -> DerivedOwnerKind {
        match self {
            DerivedOwner::Sketch(_) => DerivedOwnerKind::Sketch,
            DerivedOwner::Modifier(_) => DerivedOwnerKind::Modifier,
            DerivedOwner::Part(_) => DerivedOwnerKind::Part,
            DerivedOwner::Text(_) => DerivedOwnerKind::Text,
            DerivedOwner::Trace(_) => DerivedOwnerKind::Trace,
            DerivedOwner::Generator(_) => DerivedOwnerKind::Generator,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DerivedOwnerKind {
    Sketch,
    Modifier,
    Part,
    Text,
    Trace,
    Generator,
}

/// Stable key of one output within its owner (for a sketch: the lowest entity id of the
/// contour). The session reuses the output's `GeometryId` while the key survives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DerivedKey {
    pub owner: DerivedOwner,
    pub local: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DerivedOutput {
    pub key: DerivedKey,
    pub kind: ObjectKind,
    pub path: Path,
    /// Name for a newly created object (existing objects keep theirs).
    pub name: Option<String>,
}

/// Why an evaluation failed. Localisable: no free text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EvalDiagnosis {
    /// Constraints proven to contradict each other.
    Conflict { constraints: Vec<ConstraintId> },
    /// No solution found; these constraints are unmet (no conflict proven).
    NotConverged { unmet: Vec<ConstraintId> },
    /// Dimension values that do not evaluate (syntax, unit, unknown name, negative length).
    InvalidDimensions { constraints: Vec<ConstraintId> },
    /// The parameter table does not evaluate (syntax, unit, cycle, unknown name).
    InvalidParameters { parameter: Option<String> },
    /// The definition cannot be posed to the solver (wrong entity kinds, degenerate entities).
    InvalidDefinition,
    /// Objects the definition refers to no longer exist (modifier source or operand, IM21).
    MissingSources { objects: Vec<burnburn_types::GeometryId> },
    /// Step `step` of a modifier chain cannot be computed (IM21).
    ModifierStep { step: usize, failure: ModifierFailure },
    /// A font of a text object's chain cannot be loaded (IM18): the last valid outlines stay.
    FontMissing { font: crate::text::FontRef },
}

/// Why one modifier step failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModifierFailure {
    /// Booleans and offsets need closed contours.
    OpenContour,
    /// An inward offset or a boolean removed everything.
    Empty,
    /// Coordinates are out of range for the overlay precision.
    OutOfRange,
    /// A step value is out of range (array count, …).
    InvalidValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EvalStatus {
    Valid,
    /// The definition changed in a transaction without an evaluator: outputs are older than it.
    Pending,
    /// Evaluation failed: the outputs are the last valid ones, marked stale.
    Failed {
        diagnosis: EvalDiagnosis,
    },
}

/// Owner-specific state an evaluation stores with its outputs.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum OwnerSolution {
    /// Leave the stored state as it is (always on failure).
    Keep,
    /// New last valid sketch solution (next solve's starting point).
    Sketch(SketchSolution),
    /// Edge provenance of each output (by output key), used to resolve associative references.
    Provenance(Vec<(u64, Vec<ProvenanceEdge>)>),
    /// Characters of a valid text evaluation that no font of the chain can draw (IM18).
    Text { missing: Vec<crate::text::MissingGlyph> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentPatch {
    pub owner: DerivedOwner,
    /// Hash over the canonical evaluator inputs (definition, solution, evaluator version).
    pub inputs: ContentHash,
    /// Complete set for this owner when valid; keys not listed are removed. Ignored on failure.
    pub outputs: Vec<DerivedOutput>,
    pub status: EvalStatus,
    pub solution: OwnerSolution,
}

/// The stored evaluation state of one owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedRecord {
    pub inputs: Option<ContentHash>,
    pub status: EvalStatus,
}

/// A synchronous evaluator passed into [`crate::DocumentSession::apply_with`]. Pure and
/// deterministic; bounded by an iteration budget, never by wall-clock time.
pub trait Evaluator {
    fn owner_kind(&self) -> DerivedOwnerKind;
    fn evaluate(&self, snapshot: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch;
}
