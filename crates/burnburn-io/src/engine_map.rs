//! Trace/generator DTO ↔ document engine layer. Loading validates numbers, asset keys and
//! hashes and goes through `EngineLayer::from_parts` / `DocumentSnapshot::with_engines`.

use burnburn_document::engine::{EngineLayer, EngineRecipe, ParamValue};
use burnburn_document::generator::{GeneratorDefinition, GeneratorId};
use burnburn_document::trace::{TraceDefinition, TraceId};
use burnburn_document::{DerivedOwner, DerivedRecord, DocumentSnapshot, EvalDiagnosis, EvalStatus};
use burnburn_geometry::Affine;
use burnburn_types::{ContentHash, StableId};

use crate::LoadError;
use crate::dto::v1::{self, Num};
use crate::engine_dto as d;
use crate::map::{asset_key, parse_asset_key};

fn value_to(v: &ParamValue) -> d::Value {
    match v {
        ParamValue::Number(n) => d::Value::Number(Num(*n)),
        ParamValue::Integer(i) => d::Value::Integer(*i),
        ParamValue::Bool(b) => d::Value::Bool(*b),
        ParamValue::Text(t) => d::Value::Text(t.clone()),
        ParamValue::List(l) => d::Value::List(l.iter().map(value_to).collect()),
    }
}

pub(crate) fn recipe_to(r: &EngineRecipe) -> d::Recipe {
    d::Recipe { engine: r.engine.clone(), params: r.params.iter().map(|(k, v)| (k.clone(), value_to(v))).collect() }
}

fn evaluation_to(layer: &EngineLayer, owner: DerivedOwner) -> Option<d::Evaluation> {
    layer.record(owner).map(|r| d::Evaluation {
        inputs: r.inputs.map(|h| h.to_hex()),
        status: match r.status {
            EvalStatus::Valid => d::Status::Valid,
            EvalStatus::Failed { .. } => d::Status::InvalidDefinition,
            _ => d::Status::Pending,
        },
    })
}

pub(crate) fn engines_to_dto(doc: &DocumentSnapshot) -> (Vec<d::Trace>, Vec<d::Generator>) {
    let layer = doc.engine_layer();
    let traces = layer
        .traces()
        .map(|(id, t)| d::Trace {
            id: id.raw(),
            name: t.name.clone(),
            source: asset_key(&t.source),
            recipe: recipe_to(&t.recipe),
            key: t.key.to_hex(),
            placement: t.placement.coefficients().map(Num),
            evaluation: evaluation_to(layer, DerivedOwner::Trace(id)),
        })
        .collect();
    let generators = layer
        .generators()
        .map(|(id, g)| d::Generator {
            id: id.raw(),
            name: g.name.clone(),
            generator: g.generator.clone(),
            recipe: recipe_to(&g.recipe),
            placement: g.placement.coefficients().map(Num),
            evaluation: evaluation_to(layer, DerivedOwner::Generator(id)),
        })
        .collect();
    (traces, generators)
}

fn invalid(path: String, reason: impl Into<String>) -> LoadError {
    LoadError::InvalidValue { path, reason: reason.into() }
}

fn value_from(path: &dyn Fn() -> String, v: &d::Value) -> Result<ParamValue, LoadError> {
    Ok(match v {
        d::Value::Number(n) => ParamValue::Number(crate::map::num(path, *n)?),
        d::Value::Integer(i) => ParamValue::Integer(*i),
        d::Value::Bool(b) => ParamValue::Bool(*b),
        d::Value::Text(t) => ParamValue::Text(t.clone()),
        d::Value::List(items) => ParamValue::List(
            items
                .iter()
                .enumerate()
                .map(|(i, v)| value_from(&|| format!("{}[{i}]", path()), v))
                .collect::<Result<_, _>>()?,
        ),
    })
}

pub(crate) fn recipe_from(path: &dyn Fn() -> String, r: &d::Recipe) -> Result<EngineRecipe, LoadError> {
    let mut out = EngineRecipe::new(r.engine.clone());
    for (k, v) in &r.params {
        out.params.insert(k.clone(), value_from(&|| format!("{}.params.{k}", path()), v)?);
    }
    out.validate().map_err(|e| invalid(path(), e.to_string()))?;
    Ok(out)
}

fn placement_from(path: &dyn Fn() -> String, p: &[Num; 6]) -> Result<Affine, LoadError> {
    let mut c = [0.0; 6];
    for (i, n) in p.iter().enumerate() {
        c[i] = crate::map::num(&|| format!("{}[{i}]", path()), *n)?;
    }
    Affine::new(c).map_err(|e| invalid(path(), e.to_string()))
}

fn hash_from(path: String, hex: &str) -> Result<ContentHash, LoadError> {
    ContentHash::from_hex(hex).map_err(|e| invalid(path, e.to_string()))
}

fn record_from(path: String, e: &d::Evaluation) -> Result<DerivedRecord, LoadError> {
    let inputs = match &e.inputs {
        None => None,
        Some(hex) => Some(hash_from(format!("{path}.inputs"), hex)?),
    };
    let status = match e.status {
        d::Status::Valid => EvalStatus::Valid,
        d::Status::Pending => EvalStatus::Pending,
        d::Status::InvalidDefinition => EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidDefinition },
    };
    Ok(DerivedRecord { inputs, status })
}

/// Rebuilds the engine layer and attaches it to `doc` (after every other layer).
pub(crate) fn engines_from_dto(
    dto: &v1::Document,
    asset_exists: &dyn Fn(&ContentHash) -> bool,
    doc: DocumentSnapshot,
) -> Result<DocumentSnapshot, LoadError> {
    if dto.traces.is_empty() && dto.generators.is_empty() {
        return Ok(doc);
    }
    let mut traces = Vec::with_capacity(dto.traces.len());
    let mut generators = Vec::with_capacity(dto.generators.len());
    let mut records = Vec::new();
    for (i, t) in dto.traces.iter().enumerate() {
        let p = move |f: &str| format!("traces[{i}].{f}");
        let source = parse_asset_key(|| p("source"), &t.source)?;
        if !asset_exists(&source) {
            return Err(LoadError::MissingAsset(source));
        }
        let def = TraceDefinition {
            name: t.name.clone(),
            source,
            recipe: recipe_from(&|| p("recipe"), &t.recipe)?,
            key: hash_from(p("key"), &t.key)?,
            placement: placement_from(&|| p("placement"), &t.placement)?,
        };
        let id = TraceId::from_raw(t.id);
        if let Some(e) = &t.evaluation {
            records.push((DerivedOwner::Trace(id), record_from(p("evaluation"), e)?));
        }
        traces.push((id, def));
    }
    for (i, g) in dto.generators.iter().enumerate() {
        let p = move |f: &str| format!("generators[{i}].{f}");
        let def = GeneratorDefinition {
            name: g.name.clone(),
            generator: g.generator.clone(),
            recipe: recipe_from(&|| p("recipe"), &g.recipe)?,
            placement: placement_from(&|| p("placement"), &g.placement)?,
        };
        let id = GeneratorId::from_raw(g.id);
        if let Some(e) = &g.evaluation {
            records.push((DerivedOwner::Generator(id), record_from(p("evaluation"), e)?));
        }
        generators.push((id, def));
    }
    let layer =
        EngineLayer::from_parts(traces, generators, records).map_err(|e| invalid("traces".into(), e.to_string()))?;
    doc.with_engines(layer).map_err(|e| invalid("traces".into(), e.to_string()))
}
