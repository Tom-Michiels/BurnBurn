//! Definitions of derived geometry produced by an *engine* outside the document (C-2, C-9):
//! traces (`DerivedOwner::Trace`, asynchronous) and generators (`DerivedOwner::Generator`,
//! synchronous). The document stores what produced the outputs — engine name and version plus
//! every parameter — so a result can be reproduced, re-evaluated or audited after reopening.
//!
//! Parameters are a small, typed value tree ([`EngineRecipe`]) so that `burnburn-io` can store
//! them without depending on the engines; each engine maps its own recipe type to and from it.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use burnburn_types::{CanonicalEncode, CanonicalWriter};

use crate::generator::{GeneratorDefinition, GeneratorId};
use crate::patch::{DerivedOwner, DerivedRecord};
use crate::trace::{TraceDefinition, TraceId};

/// One parameter value of an engine recipe.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    /// A finite real number (mm, degrees, …; the parameter name says which).
    Number(f64),
    Integer(i64),
    Bool(bool),
    Text(String),
    List(Vec<ParamValue>),
}

/// Engine identity (`name/version`) and parameters. The engine string is part of every inputs
/// hash, so outputs of another engine version are never mistaken for current ones.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EngineRecipe {
    pub engine: String,
    pub params: BTreeMap<String, ParamValue>,
}

impl EngineRecipe {
    pub fn new(engine: impl Into<String>) -> Self {
        EngineRecipe { engine: engine.into(), params: BTreeMap::new() }
    }

    /// Builder: the same recipe with `name = value`.
    pub fn with(mut self, name: impl Into<String>, value: ParamValue) -> Self {
        self.params.insert(name.into(), value);
        self
    }

    pub fn get(&self, name: &str) -> Option<&ParamValue> {
        self.params.get(name)
    }

    /// The engine string must be non-empty, every name non-blank and every number finite.
    pub fn validate(&self) -> Result<(), EngineError> {
        fn value(name: &str, v: &ParamValue) -> Result<(), EngineError> {
            match v {
                ParamValue::Number(n) if !n.is_finite() => Err(EngineError::InvalidParam(name.to_owned())),
                ParamValue::List(items) => items.iter().try_for_each(|i| value(name, i)),
                _ => Ok(()),
            }
        }
        if self.engine.trim().is_empty() {
            return Err(EngineError::InvalidParam("engine".into()));
        }
        for (name, v) in &self.params {
            if name.trim().is_empty() {
                return Err(EngineError::InvalidParam(name.clone()));
            }
            value(name, v)?;
        }
        Ok(())
    }
}

fn encode_value(v: &ParamValue, out: &mut CanonicalWriter) {
    match v {
        ParamValue::Number(n) => {
            out.tag("n");
            out.f64(*n);
        }
        ParamValue::Integer(i) => {
            out.tag("i");
            out.u64(*i as u64);
        }
        ParamValue::Bool(b) => {
            out.tag("b");
            out.bool(*b);
        }
        ParamValue::Text(s) => {
            out.tag("t");
            out.str(s);
        }
        ParamValue::List(items) => {
            out.tag("l");
            out.u64(items.len() as u64);
            for i in items {
                encode_value(i, out);
            }
        }
    }
}

/// `str` engine, `u64` count, then (`str` name, typed value) in name order.
impl CanonicalEncode for EngineRecipe {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.str(&self.engine);
        out.u64(self.params.len() as u64);
        for (k, v) in &self.params {
            out.str(k);
            encode_value(v, out);
        }
    }
}

/// Why a trace or generator command (or stored data) was refused. The document is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EngineError {
    UnknownTrace(TraceId),
    UnknownGenerator(GeneratorId),
    /// A recipe parameter (or the engine string) is blank or not finite.
    InvalidParam(String),
    /// The placement or an output path cannot be used (non-invertible, duplicate output key).
    InvalidOutput,
    /// A stored record or id does not fit the layer.
    InvalidRecord(u64),
    NothingChanged,
    IdsExhausted,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::UnknownTrace(t) => write!(f, "no trace {t}"),
            EngineError::UnknownGenerator(g) => write!(f, "no generator {g}"),
            EngineError::InvalidParam(p) => write!(f, "invalid recipe parameter '{p}'"),
            EngineError::InvalidOutput => f.write_str("invalid placement or output set"),
            EngineError::InvalidRecord(raw) => write!(f, "invalid stored engine record {raw}"),
            EngineError::NothingChanged => f.write_str("nothing changed"),
            EngineError::IdsExhausted => f.write_str("ids exhausted"),
        }
    }
}

impl std::error::Error for EngineError {}

/// Trace and generator definitions of a document plus their evaluation records (§11).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineLayer {
    pub(crate) traces: BTreeMap<TraceId, Arc<TraceDefinition>>,
    pub(crate) generators: BTreeMap<GeneratorId, Arc<GeneratorDefinition>>,
    pub(crate) derived: BTreeMap<DerivedOwner, DerivedRecord>,
}

impl EngineLayer {
    pub fn new() -> Self {
        EngineLayer::default()
    }

    pub fn trace(&self, id: TraceId) -> Option<&TraceDefinition> {
        self.traces.get(&id).map(|t| t.as_ref())
    }

    pub fn traces(&self) -> impl ExactSizeIterator<Item = (TraceId, &TraceDefinition)> {
        self.traces.iter().map(|(k, v)| (*k, v.as_ref()))
    }

    pub fn generator(&self, id: GeneratorId) -> Option<&GeneratorDefinition> {
        self.generators.get(&id).map(|g| g.as_ref())
    }

    pub fn generators(&self) -> impl ExactSizeIterator<Item = (GeneratorId, &GeneratorDefinition)> {
        self.generators.iter().map(|(k, v)| (*k, v.as_ref()))
    }

    pub fn record(&self, owner: DerivedOwner) -> Option<&DerivedRecord> {
        self.derived.get(&owner)
    }

    pub fn records(&self) -> impl ExactSizeIterator<Item = (DerivedOwner, &DerivedRecord)> {
        self.derived.iter().map(|(k, v)| (*k, v))
    }

    pub fn is_empty(&self) -> bool {
        self.traces.is_empty() && self.generators.is_empty()
    }

    /// Raw ids of every definition (for id-space checks on load).
    pub(crate) fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        use burnburn_types::StableId;
        self.traces.keys().map(|t| t.raw()).chain(self.generators.keys().map(|g| g.raw()))
    }

    /// Stored layer (open/recovery). Recipes are validated; records must belong to a stored
    /// trace or generator; ids must be unique across both.
    pub fn from_parts(
        traces: Vec<(TraceId, TraceDefinition)>,
        generators: Vec<(GeneratorId, GeneratorDefinition)>,
        records: Vec<(DerivedOwner, DerivedRecord)>,
    ) -> Result<Self, EngineError> {
        use burnburn_types::StableId;
        let mut layer = EngineLayer::new();
        let mut seen = std::collections::BTreeSet::new();
        for (id, t) in traces {
            t.validate()?;
            if !seen.insert(id.raw()) {
                return Err(EngineError::InvalidRecord(id.raw()));
            }
            layer.traces.insert(id, Arc::new(t));
        }
        for (id, g) in generators {
            g.recipe.validate()?;
            if !seen.insert(id.raw()) {
                return Err(EngineError::InvalidRecord(id.raw()));
            }
            layer.generators.insert(id, Arc::new(g));
        }
        for (owner, r) in records {
            let known = match owner {
                DerivedOwner::Trace(t) => layer.traces.contains_key(&t),
                DerivedOwner::Generator(g) => layer.generators.contains_key(&g),
                _ => false,
            };
            if !known {
                return Err(EngineError::InvalidRecord(0));
            }
            layer.derived.insert(owner, r);
        }
        Ok(layer)
    }
}
