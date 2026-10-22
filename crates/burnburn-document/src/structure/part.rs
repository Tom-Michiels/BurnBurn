//! Part definitions, variants and instances (S07): one definition, many placements with their
//! own transform; variants are named parameter sets; an instance override is stored on the
//! instance (visible) and never written back into the definition.

use std::collections::BTreeMap;

use burnburn_geometry::{Affine, Path, Point, Rect, Shape, Vector};
use burnburn_types::{CanonicalEncode, CanonicalWriter, ContentHash, HashDomain, Length, StableId};

use super::{FormulaError, PartDefinitionId, PartInstanceId, PlateId, parse_formula};
use crate::{
    DerivedKey, DerivedOutput, DerivedOwner, DocumentPatch, EvalDiagnosis, EvalStatus, ObjectKind, OwnerSolution,
};

/// Version of the part evaluator; part of the inputs hash.
pub const PART_EVALUATOR_VERSION: u64 = 1;

/// A length in a part shape: a literal or a named parameter of the definition.
#[derive(Debug, Clone, PartialEq)]
pub enum Dim {
    Mm(f64),
    Param(String),
    /// A formula over the parameters of the part (G8-editor).
    Expr(String),
}

/// A shape of a part definition, in the definition's own frame (mm).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PartShape {
    Rectangle {
        x: Dim,
        y: Dim,
        width: Dim,
        height: Dim,
    },
    Circle {
        x: Dim,
        y: Dim,
        radius: Dim,
    },
    /// Fixed geometry (for example imported), not parametric.
    Fixed(Path),
    /// Fixed geometry whose box follows parameters and formulas (G9-editor): `path` keeps its own
    /// shape and is stretched from its own box to the box `x, y, width, height`.
    Fitted {
        path: Path,
        x: Dim,
        y: Dim,
        width: Dim,
        height: Dim,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamedShape {
    pub name: String,
    pub shape: PartShape,
}

/// A reusable component. Shape keys are stable: the output of shape `k` keeps its
/// `GeometryId` in every instance while `k` exists.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PartDefinition {
    pub name: String,
    /// Default parameter values (mm).
    pub parameters: BTreeMap<String, Length>,
    pub shapes: BTreeMap<u32, NamedShape>,
    /// Variant name → parameter overrides (only existing parameters).
    pub variants: BTreeMap<String, BTreeMap<String, Length>>,
}

/// One placement of a definition.
#[derive(Debug, Clone, PartialEq)]
pub struct PartInstance {
    pub definition: PartDefinitionId,
    pub variant: Option<String>,
    /// Local parameter overrides of this instance only (shown in the inspector).
    pub overrides: BTreeMap<String, Length>,
    pub transform: Affine,
    pub plate: Option<PlateId>,
}

/// Default values, then the variant, then the instance overrides.
pub fn effective_parameters(def: &PartDefinition, inst: &PartInstance) -> BTreeMap<String, Length> {
    let mut out = def.parameters.clone();
    if let Some(v) = inst.variant.as_ref().and_then(|v| def.variants.get(v)) {
        out.extend(v.iter().map(|(k, v)| (k.clone(), *v)));
    }
    out.extend(inst.overrides.iter().map(|(k, v)| (k.clone(), *v)));
    out
}

pub(crate) fn instance_inputs(id: PartInstanceId, def: &PartDefinition, inst: &PartInstance) -> ContentHash {
    let mut w = CanonicalWriter::new();
    w.u64(PART_EVALUATOR_VERSION);
    w.u64(id.raw());
    def.encode(&mut w);
    inst.encode(&mut w);
    let bytes = w.finish().unwrap_or_else(|_| b"non-finite".to_vec());
    ContentHash::of(HashDomain::EvalPart, &bytes)
}

/// Evaluates one instance: every shape with the effective parameters, in the definition's frame,
/// then the instance transform. A value that does not fit (non-positive size) fails this
/// instance only, naming the parameter.
pub(crate) fn evaluate_instance(id: PartInstanceId, def: &PartDefinition, inst: &PartInstance) -> DocumentPatch {
    let owner = DerivedOwner::Part(id);
    let inputs = instance_inputs(id, def, inst);
    let values = effective_parameters(def, inst);
    let mut outputs = Vec::with_capacity(def.shapes.len());
    for (key, shape) in &def.shapes {
        match shape_path(&shape.shape, &values) {
            Ok((kind, path)) => outputs.push(DerivedOutput {
                key: DerivedKey { owner, local: u64::from(*key) },
                kind,
                path: path.transform(&inst.transform),
                name: Some(format!("{}/{}", def.name, shape.name)),
            }),
            Err(parameter) => {
                return DocumentPatch {
                    owner,
                    inputs,
                    outputs: Vec::new(),
                    status: EvalStatus::Failed {
                        diagnosis: EvalDiagnosis::InvalidParameters { parameter: Some(parameter) },
                    },
                    solution: OwnerSolution::Keep,
                };
            }
        }
    }
    DocumentPatch { owner, inputs, outputs, status: EvalStatus::Valid, solution: OwnerSolution::Keep }
}

/// A shape's path; `Err(name)` names the parameter (or field) whose value does not fit.
fn shape_path(shape: &PartShape, values: &BTreeMap<String, Length>) -> Result<(ObjectKind, Path), String> {
    let get = |d: &Dim, field: &str| -> Result<f64, String> {
        match d {
            Dim::Mm(v) => Ok(*v),
            Dim::Param(p) => values.get(p).map(|v| v.mm()).ok_or_else(|| p.clone()),
            Dim::Expr(text) => {
                let formula = parse_formula(text).map_err(|_| field.to_owned())?;
                formula.eval(values).map_err(|e| match e {
                    FormulaError::UnknownName(n) => n,
                    _ => formula.names().into_iter().next().unwrap_or_else(|| field.to_owned()),
                })
            }
        }
    };
    let name = |d: &Dim, field: &str| match d {
        Dim::Param(p) => p.clone(),
        // A formula that gives no usable size names the first parameter it reads.
        Dim::Expr(text) => {
            parse_formula(text).ok().and_then(|f| f.names().into_iter().next()).unwrap_or_else(|| field.to_owned())
        }
        Dim::Mm(_) => field.to_owned(),
    };
    let positive = |d: &Dim, field: &str| -> Result<Length, String> {
        let v = get(d, field)?;
        if v > 0.0 { Length::from_mm(v).map_err(|_| name(d, field)) } else { Err(name(d, field)) }
    };
    match shape {
        PartShape::Rectangle { x, y, width, height } => {
            let at = Point::new(get(x, "x")?, get(y, "y")?);
            let rect = Rect::new(at, positive(width, "width")?, positive(height, "height")?)
                .map_err(|_| name(width, "width"))?;
            Ok((ObjectKind::Rectangle, Path::rectangle(rect)))
        }
        PartShape::Circle { x, y, radius } => {
            let center = Point::new(get(x, "x")?, get(y, "y")?);
            let shape = Shape::Circle { center, radius: positive(radius, "radius")? };
            let path = shape.to_path().map_err(|_| name(radius, "radius"))?;
            Ok((ObjectKind::Circle, path))
        }
        PartShape::Fixed(p) => Ok((ObjectKind::Path, p.clone())),
        PartShape::Fitted { path, x, y, width, height } => {
            let own = path.bounds().ok_or_else(|| "path".to_owned())?;
            let (target_x, target_y) = (get(x, "x")?, get(y, "y")?);
            let (target_w, target_h) = (positive(width, "width")?.mm(), positive(height, "height")?.mm());
            if own.width() <= 0.0 || own.height() <= 0.0 {
                return Err("path".to_owned());
            }
            let (sx, sy) = (target_w / own.width(), target_h / own.height());
            // Own corner to the origin, stretch, then to the target corner.
            let to_origin =
                Affine::translate(Vector::new(-own.min().x, -own.min().y)).map_err(|_| "path".to_owned())?;
            let stretch = Affine::scale(sx, sy).map_err(|_| name(width, "width"))?;
            let to_target = Affine::translate(Vector::new(target_x, target_y)).map_err(|_| "x".to_owned())?;
            let fitted = path.transform(&to_origin).transform(&stretch).transform(&to_target);
            Ok((ObjectKind::Path, fitted))
        }
    }
}

fn dim(out: &mut CanonicalWriter, d: &Dim) {
    match d {
        Dim::Mm(v) => {
            out.tag("mm");
            out.f64(*v);
        }
        Dim::Param(p) => {
            out.tag("param");
            out.str(p);
        }
        Dim::Expr(text) => {
            out.tag("expr");
            out.str(text);
        }
    }
}

fn values(out: &mut CanonicalWriter, m: &BTreeMap<String, Length>) {
    out.u64(m.len() as u64);
    for (k, v) in m {
        out.str(k);
        out.f64(v.mm());
    }
}

impl CanonicalEncode for PartDefinition {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.str(&self.name);
        values(out, &self.parameters);
        out.u64(self.shapes.len() as u64);
        for (k, s) in &self.shapes {
            out.u64(u64::from(*k));
            out.str(&s.name);
            match &s.shape {
                PartShape::Rectangle { x, y, width, height } => {
                    out.tag("rectangle");
                    for d in [x, y, width, height] {
                        dim(out, d);
                    }
                }
                PartShape::Circle { x, y, radius } => {
                    out.tag("circle");
                    for d in [x, y, radius] {
                        dim(out, d);
                    }
                }
                PartShape::Fixed(p) => {
                    out.tag("fixed");
                    p.encode(out);
                }
                PartShape::Fitted { path, x, y, width, height } => {
                    out.tag("fitted");
                    path.encode(out);
                    for d in [x, y, width, height] {
                        dim(out, d);
                    }
                }
            }
        }
        out.u64(self.variants.len() as u64);
        for (name, v) in &self.variants {
            out.str(name);
            values(out, v);
        }
    }
}

impl CanonicalEncode for PartInstance {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.u64(self.definition.raw());
        match &self.variant {
            Some(v) => {
                out.tag("variant");
                out.str(v);
            }
            None => out.tag("default"),
        }
        values(out, &self.overrides);
        for c in self.transform.coefficients() {
            out.f64(c);
        }
        match self.plate {
            Some(p) => out.u64(p.raw()),
            None => out.tag("no-plate"),
        }
    }
}
