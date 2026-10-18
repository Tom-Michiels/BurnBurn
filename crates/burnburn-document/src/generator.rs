//! Generator definitions in the document (C-2, contract §IM22): which generator, its engine
//! version and every parameter, plus a placement. The outputs are derived objects of
//! `DerivedOwner::Generator`, produced synchronously by `burnburn-generators`'
//! `GeneratorEvaluator` inside the same transaction (`apply_with`). Without an evaluator the
//! owner is `Pending` and production is blocked; nominal geometry only (no kerf, C-1).

use std::fmt;
use std::sync::Arc;

use burnburn_geometry::Affine;
use burnburn_types::{CanonicalEncode, CanonicalWriter, ContentHash, HashDomain, IdAllocator, StableId};

use crate::engine::{EngineError, EngineLayer, EngineRecipe};
use crate::patch::DerivedOwner;

/// Identity of a generator in a document (shared id counter); also `DerivedOwner::Generator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GeneratorId(u64);

impl StableId for GeneratorId {
    fn from_raw(raw: u64) -> Self {
        GeneratorId(raw)
    }
    fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for GeneratorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "gen{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratorDefinition {
    pub name: String,
    /// Which generator (for example `"finger-box"`, `"code128"`, `"qr"`).
    pub generator: String,
    /// Engine name/version and the generator's parameters.
    pub recipe: EngineRecipe,
    /// Generator-local millimetres → document.
    pub placement: Affine,
}

/// `str` name, `str` generator, recipe, placement coefficients.
impl CanonicalEncode for GeneratorDefinition {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.str(&self.name);
        out.str(&self.generator);
        self.recipe.encode(out);
        for c in self.placement.coefficients() {
            out.f64(c);
        }
    }
}

/// Inputs hash of generator `id` (domain `eval-generator`, C-5): the owner and its complete
/// definition (engine version included). Evaluators must report exactly this value.
pub fn generator_inputs(id: GeneratorId, def: &GeneratorDefinition) -> ContentHash {
    let mut w = CanonicalWriter::new();
    w.tag("generator");
    w.u64(id.raw());
    def.encode(&mut w);
    // Definitions are validated finite; a non-finite value still changes the hash.
    let bytes = w.finish().unwrap_or_else(|_| b"non-finite".to_vec());
    ContentHash::of(HashDomain::EvalGenerator, &bytes)
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum GeneratorCommand {
    Create {
        definition: Box<GeneratorDefinition>,
    },
    /// New parameters, generator or placement; the outputs follow in the same transaction.
    Set {
        generator: GeneratorId,
        definition: Box<GeneratorDefinition>,
    },
    Delete {
        generator: GeneratorId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GeneratorLabel {
    Create,
    Edit,
    Delete,
}

impl GeneratorCommand {
    pub fn label(&self) -> GeneratorLabel {
        match self {
            GeneratorCommand::Create { .. } => GeneratorLabel::Create,
            GeneratorCommand::Set { .. } => GeneratorLabel::Edit,
            GeneratorCommand::Delete { .. } => GeneratorLabel::Delete,
        }
    }
}

pub(crate) enum GeneratorEffect {
    None,
    Deleted(GeneratorId),
}

pub(crate) fn apply(
    cmd: GeneratorCommand,
    layer: &mut EngineLayer,
    ids: &mut IdAllocator,
    changed: &mut Vec<GeneratorId>,
) -> Result<GeneratorEffect, EngineError> {
    match cmd {
        GeneratorCommand::Create { definition } => {
            definition.recipe.validate()?;
            let id: GeneratorId = ids.allocate().map_err(|_| EngineError::IdsExhausted)?;
            layer.generators.insert(id, Arc::from(definition));
            changed.push(id);
            Ok(GeneratorEffect::None)
        }
        GeneratorCommand::Set { generator, definition } => {
            definition.recipe.validate()?;
            let slot = layer.generators.get_mut(&generator).ok_or(EngineError::UnknownGenerator(generator))?;
            if **slot == *definition {
                return Err(EngineError::NothingChanged);
            }
            *slot = Arc::from(definition);
            changed.push(generator);
            Ok(GeneratorEffect::None)
        }
        GeneratorCommand::Delete { generator } => {
            layer.generators.remove(&generator).ok_or(EngineError::UnknownGenerator(generator))?;
            layer.derived.remove(&DerivedOwner::Generator(generator));
            changed.push(generator);
            Ok(GeneratorEffect::Deleted(generator))
        }
    }
}
