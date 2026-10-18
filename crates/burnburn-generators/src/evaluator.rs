//! `document::Evaluator` for generator definitions (IM22, C-2): reads the definition from the
//! snapshot, maps its parameters onto a generator function and returns the placed outputs as a
//! `DocumentPatch` whose inputs hash is `document::generator::generator_inputs`.
//!
//! Supported generators (parameter names = the generator's own parameter names, lengths in mm):
//! - `qr`: `data` (text), `ecc` (`L|M|Q|H`), `module`, `quiet_modules` (integer), `min_module`.
//! - `code128`: `data`, `module`, `height`, `quiet_modules`, `min_module`.
//! - `finger-box`: `width`, `depth`, `height`, `thickness`, `finger_width`, `lid` (`closed|open`),
//!   `spacing`; one output per panel, keyed by panel role.
//!
//! A bad value fails with `InvalidParameters { parameter }` (last valid outputs stay, production
//! blocked); an unknown generator or another engine version is `InvalidDefinition`.

use burnburn_document::engine::{EngineRecipe, ParamValue};
use burnburn_document::generator::{GeneratorDefinition, generator_inputs};
use burnburn_document::{
    DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentPatch, DocumentSnapshot, EvalDiagnosis,
    EvalStatus, Evaluator, ObjectKind, OwnerSolution,
};
use burnburn_types::ContentHash;

use crate::{
    BoxParams, Code128Params, GenError, GeneratedPart, Lid, PanelRole, QrEcc, QrParams, code128, finger_box, qr,
};

/// Engine identity written into every generator recipe.
pub const GENERATOR_ENGINE: &str = "burnburn-generators/1";

/// Synchronous evaluator for `DocumentSession::apply_with`. Pure and deterministic.
#[derive(Debug, Clone, Copy, Default)]
pub struct GeneratorEvaluator;

enum Failure {
    Definition,
    Param(String),
}

impl From<GenError> for Failure {
    fn from(e: GenError) -> Self {
        match e {
            GenError::NotFinite { param }
            | GenError::NotPositive { param }
            | GenError::Negative { param }
            | GenError::TooSmall { param, .. }
            | GenError::Count { param, .. } => Failure::Param(param.into()),
            GenError::UnsupportedChar { .. } => Failure::Param("data".into()),
            _ => Failure::Definition,
        }
    }
}

struct Params<'a>(&'a EngineRecipe);

impl Params<'_> {
    fn get(&self, name: &str) -> Result<&ParamValue, Failure> {
        self.0.get(name).ok_or_else(|| Failure::Param(name.into()))
    }
    fn number(&self, name: &str) -> Result<f64, Failure> {
        match self.get(name)? {
            ParamValue::Number(n) => Ok(*n),
            ParamValue::Integer(i) => Ok(*i as f64),
            _ => Err(Failure::Param(name.into())),
        }
    }
    fn count(&self, name: &str) -> Result<u32, Failure> {
        match self.get(name)? {
            ParamValue::Integer(i) => u32::try_from(*i).map_err(|_| Failure::Param(name.into())),
            _ => Err(Failure::Param(name.into())),
        }
    }
    fn text(&self, name: &str) -> Result<&str, Failure> {
        match self.get(name)? {
            ParamValue::Text(t) => Ok(t),
            _ => Err(Failure::Param(name.into())),
        }
    }
}

const ROLES: [PanelRole; 6] =
    [PanelRole::Bottom, PanelRole::Top, PanelRole::Front, PanelRole::Back, PanelRole::Left, PanelRole::Right];

fn generate(def: &GeneratorDefinition) -> Result<Vec<GeneratedPart>, Failure> {
    if def.recipe.engine != GENERATOR_ENGINE {
        return Err(Failure::Definition);
    }
    let p = Params(&def.recipe);
    match def.generator.as_str() {
        "qr" => {
            let ecc = match p.text("ecc")? {
                "L" => QrEcc::L,
                "M" => QrEcc::M,
                "Q" => QrEcc::Q,
                "H" => QrEcc::H,
                _ => return Err(Failure::Param("ecc".into())),
            };
            let params = QrParams {
                module: p.number("module")?,
                quiet_modules: p.count("quiet_modules")?,
                min_module: p.number("min_module")?,
            };
            let sym = qr(p.text("data")?.as_bytes(), ecc, &params)?;
            Ok(vec![GeneratedPart { local: 0, name: def.name.clone(), path: sym.path }])
        }
        "code128" => {
            let params = Code128Params {
                module: p.number("module")?,
                height: p.number("height")?,
                quiet_modules: p.count("quiet_modules")?,
                min_module: p.number("min_module")?,
            };
            let bar = code128(p.text("data")?, &params)?;
            Ok(vec![GeneratedPart { local: 0, name: def.name.clone(), path: bar.path }])
        }
        "finger-box" => {
            let lid = match p.text("lid")? {
                "closed" => Lid::Closed,
                "open" => Lid::Open,
                _ => return Err(Failure::Param("lid".into())),
            };
            let params = BoxParams {
                width: p.number("width")?,
                depth: p.number("depth")?,
                height: p.number("height")?,
                thickness: p.number("thickness")?,
                finger_width: p.number("finger_width")?,
                lid,
                spacing: p.number("spacing")?,
            };
            let panels = finger_box(&params)?;
            Ok(panels
                .into_iter()
                .map(|panel| GeneratedPart {
                    local: ROLES.iter().position(|r| *r == panel.role).unwrap_or(ROLES.len()) as u64,
                    name: format!("{} {:?}", def.name, panel.role),
                    path: panel.path,
                })
                .collect())
        }
        _ => Err(Failure::Definition),
    }
}

impl Evaluator for GeneratorEvaluator {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Generator
    }

    fn evaluate(&self, snapshot: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        let failed = |inputs, diagnosis| DocumentPatch {
            owner,
            inputs,
            outputs: Vec::new(),
            status: EvalStatus::Failed { diagnosis },
            solution: OwnerSolution::Keep,
        };
        let none = ContentHash::from_bytes([0; 32]);
        let DerivedOwner::Generator(id) = owner else {
            return failed(none, EvalDiagnosis::InvalidDefinition);
        };
        let Some(def) = snapshot.generator(id) else {
            return failed(none, EvalDiagnosis::InvalidDefinition);
        };
        let inputs = generator_inputs(id, def);
        match generate(def) {
            Ok(parts) => DocumentPatch {
                owner,
                inputs,
                outputs: parts
                    .into_iter()
                    .map(|part| DerivedOutput {
                        key: DerivedKey { owner, local: part.local },
                        kind: ObjectKind::Path,
                        path: part.path.transform(&def.placement),
                        name: Some(part.name),
                    })
                    .collect(),
                status: EvalStatus::Valid,
                solution: OwnerSolution::Keep,
            },
            Err(Failure::Definition) => failed(inputs, EvalDiagnosis::InvalidDefinition),
            Err(Failure::Param(p)) => failed(inputs, EvalDiagnosis::InvalidParameters { parameter: Some(p) }),
        }
    }
}
