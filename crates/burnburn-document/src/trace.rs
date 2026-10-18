//! Traces in the document (C-9, contract §11): the source asset, the full recipe with engine
//! version, the engine's reproducibility key and a placement; the traced contours are derived
//! objects of `DerivedOwner::Trace`. Tracing itself is asynchronous (`burnburn-trace`); its
//! result enters as one command ("Pas trace toe"), one undo step.
//!
//! Outputs are delivered in *trace-local* millimetres; the document places them with the
//! definition's `placement`. Moving a trace is `SetPlacement` (C-8: derived geometry is never
//! transformed directly), which keeps every `GeometryId` and operation target.

use std::fmt;
use std::sync::Arc;

use burnburn_geometry::{Affine, Path};
use burnburn_types::{CanonicalEncode, CanonicalWriter, ContentHash, IdAllocator, StableId};

use crate::ObjectKind;
use crate::engine::{EngineError, EngineLayer, EngineRecipe};
use crate::patch::DerivedOwner;

/// Identity of a trace in a document (shared id counter); also `DerivedOwner::Trace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraceId(u64);

impl StableId for TraceId {
    fn from_raw(raw: u64) -> Self {
        TraceId(raw)
    }
    fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tr{}", self.0)
    }
}

/// What produced a trace's outputs.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceDefinition {
    pub name: String,
    /// Content hash of the source image bytes (an asset of the project).
    pub source: ContentHash,
    /// Engine name/version and every recipe parameter.
    pub recipe: EngineRecipe,
    /// The engine's reproducibility key over engine version, source and recipe (for
    /// `burnburn-trace`: `TraceResult.recipe_key`). This is the owner's *inputs* hash: an
    /// `ApplyPatch` computed for another key is stale.
    pub key: ContentHash,
    /// Trace-local millimetres → document.
    pub placement: Affine,
}

impl TraceDefinition {
    pub fn validate(&self) -> Result<(), EngineError> {
        self.recipe.validate()?;
        self.placement.inverse().map(|_| ()).map_err(|_| EngineError::InvalidOutput)
    }
}

/// `str` name, `bytes` source, recipe, `bytes` key, placement coefficients.
impl CanonicalEncode for TraceDefinition {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.str(&self.name);
        out.bytes(self.source.as_bytes());
        self.recipe.encode(out);
        out.bytes(self.key.as_bytes());
        for c in self.placement.coefficients() {
            out.f64(c);
        }
    }
}

/// One traced output in trace-local millimetres; `local` is its stable key within the trace
/// (for `burnburn-trace`: the outer contour id of a component).
#[derive(Debug, Clone, PartialEq)]
pub struct TraceOutput {
    pub local: u64,
    pub kind: ObjectKind,
    pub path: Path,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TraceCommand {
    /// Adds a trace with its complete output set (one undo step).
    Create { definition: Box<TraceDefinition>, outputs: Vec<TraceOutput> },
    /// A new result for an existing trace (other recipe or correction): definition and complete
    /// output set together; outputs whose key survives keep their `GeometryId`.
    Replace { trace: TraceId, definition: Box<TraceDefinition>, outputs: Vec<TraceOutput> },
    /// Moves the trace's outputs to a new placement; ids, names and targets stay.
    SetPlacement { trace: TraceId, placement: Affine },
    /// Removes the trace and its outputs (the source asset stays in the project).
    Delete { trace: TraceId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TraceLabel {
    Apply,
    Retrace,
    Move,
    Delete,
}

impl TraceCommand {
    pub fn label(&self) -> TraceLabel {
        match self {
            TraceCommand::Create { .. } => TraceLabel::Apply,
            TraceCommand::Replace { .. } => TraceLabel::Retrace,
            TraceCommand::SetPlacement { .. } => TraceLabel::Move,
            TraceCommand::Delete { .. } => TraceLabel::Delete,
        }
    }
}

/// What the session must do after a trace command.
pub(crate) enum TraceEffect {
    /// Write this complete output set (trace-local) for the trace.
    Outputs(TraceId, Vec<TraceOutput>),
    /// Transform the existing outputs by this delta.
    Move(TraceId, Affine),
    /// Remove every output of the trace.
    Deleted(TraceId),
}

fn check_outputs(outputs: &[TraceOutput]) -> Result<(), EngineError> {
    let mut keys: Vec<u64> = outputs.iter().map(|o| o.local).collect();
    keys.sort_unstable();
    let before = keys.len();
    keys.dedup();
    if keys.len() == before { Ok(()) } else { Err(EngineError::InvalidOutput) }
}

pub(crate) fn apply(
    cmd: TraceCommand,
    layer: &mut EngineLayer,
    ids: &mut IdAllocator,
    changed: &mut Vec<TraceId>,
) -> Result<TraceEffect, EngineError> {
    match cmd {
        TraceCommand::Create { definition, outputs } => {
            definition.validate()?;
            check_outputs(&outputs)?;
            let id: TraceId = ids.allocate().map_err(|_| EngineError::IdsExhausted)?;
            layer.traces.insert(id, Arc::from(definition));
            changed.push(id);
            Ok(TraceEffect::Outputs(id, outputs))
        }
        TraceCommand::Replace { trace, definition, outputs } => {
            definition.validate()?;
            check_outputs(&outputs)?;
            let slot = layer.traces.get_mut(&trace).ok_or(EngineError::UnknownTrace(trace))?;
            *slot = Arc::from(definition);
            changed.push(trace);
            Ok(TraceEffect::Outputs(trace, outputs))
        }
        TraceCommand::SetPlacement { trace, placement } => {
            let slot = layer.traces.get_mut(&trace).ok_or(EngineError::UnknownTrace(trace))?;
            if slot.placement == placement {
                return Err(EngineError::NothingChanged);
            }
            let old = slot.placement.inverse().map_err(|_| EngineError::InvalidOutput)?;
            placement.inverse().map_err(|_| EngineError::InvalidOutput)?;
            let delta = old.then(&placement);
            let mut next = (**slot).clone();
            next.placement = placement;
            *slot = Arc::new(next);
            changed.push(trace);
            Ok(TraceEffect::Move(trace, delta))
        }
        TraceCommand::Delete { trace } => {
            layer.traces.remove(&trace).ok_or(EngineError::UnknownTrace(trace))?;
            layer.derived.remove(&DerivedOwner::Trace(trace));
            changed.push(trace);
            Ok(TraceEffect::Deleted(trace))
        }
    }
}
