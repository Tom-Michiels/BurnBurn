//! Non-destructive modifier chains (E06): a source object stays editable; the chain's output
//! is derived geometry (`DerivedOwner::Modifier`) re-evaluated whenever its inputs change.

use burnburn_geometry::array::{CircularArray, GridArray, PathArray, path_array};
use burnburn_geometry::{
    Affine, BooleanError, BooleanOp, BooleanOptions, FillRule, OffsetError, OffsetJoin, Path, Point,
    ProductionTolerance, Segment, Subpath, Vector, boolean, offset,
};
use burnburn_types::{
    CancellationToken, CanonicalEncode, CanonicalWriter, ContentHash, GeometryId, HashDomain, Length, StableId,
};

use super::reference::{Cand, SegmentOrigins, drawn_segments, identity_origins, runs};
use super::{EdgeOrigin, ModifierId};
use crate::{
    DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentPatch, DocumentSnapshot, EvalDiagnosis,
    EvalStatus, Evaluator, GeometryObject, ModifierFailure, ObjectKind, OwnerSolution,
};

/// Version of the modifier evaluator; part of the inputs hash.
pub const MODIFIER_EVALUATOR_VERSION: u64 = 1;

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Modifier {
    /// Positive grows the area.
    Offset { distance: Length, join: OffsetJoin },
    /// The current result `op` each operand in turn (operands are other objects, unchanged).
    Boolean { op: BooleanOp, with: Vec<GeometryId> },
    /// `count` copies (including the original), copy `k` translated by `k · step`.
    LinearArray { count: u32, step: Vector },
    /// `columns × rows` copies, row-major (IM26 array as a modifier; `geometry::array`).
    GridArray(GridArray),
    /// `count` copies around a centre (IM26 array as a modifier; `geometry::array`).
    CircularArray(CircularArray),
    /// `count` copies along another object's first outline (AR02, copy along path): the source
    /// is carried rigidly from the start of `along` to evenly spaced points on it, turning with
    /// the path when `orient` is set. See [`burnburn_geometry::array::path_array`].
    PathArray { count: u32, along: GeometryId, orient: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModifierStep {
    pub modifier: Modifier,
    /// A disabled step is skipped; the source and the other steps are untouched.
    pub enabled: bool,
}

/// When the chain is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvalMode {
    /// In the same transaction as the edit that changed its inputs.
    Inline,
    /// By a background task; the result arrives as `CommandPayload::ApplyPatch` and is refused
    /// when the inputs changed meanwhile (`StalePatch`).
    Background,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModifierChain {
    pub name: String,
    pub source: GeometryId,
    pub steps: Vec<ModifierStep>,
    /// Flattening tolerance for booleans and offsets (output is polygonal there).
    pub tolerance: ProductionTolerance,
    pub mode: EvalMode,
}

impl ModifierChain {
    /// Every object the chain reads: the source, then boolean operands (enabled or not).
    pub fn inputs(&self) -> Vec<GeometryId> {
        let mut out = vec![self.source];
        for s in &self.steps {
            match &s.modifier {
                Modifier::Boolean { with, .. } => out.extend(with.iter().copied()),
                Modifier::PathArray { along, .. } => out.push(*along),
                _ => {}
            }
        }
        out
    }
}

/// Canonical inputs hash of chain `id` (domain `eval-modifier`): evaluator version, chain, and
/// the geometry of every object it reads (missing objects are named as missing).
pub(crate) fn chain_inputs<'a>(
    id: ModifierId,
    chain: &ModifierChain,
    lookup: &dyn Fn(GeometryId) -> Option<&'a GeometryObject>,
) -> ContentHash {
    let mut w = CanonicalWriter::new();
    w.u64(MODIFIER_EVALUATOR_VERSION);
    w.u64(id.raw());
    chain.encode(&mut w);
    for g in chain.inputs() {
        match lookup(g) {
            Some(o) => {
                w.tag("object");
                w.u64(g.raw());
                o.path().as_ref().encode(&mut w);
            }
            None => {
                w.tag("missing");
                w.u64(g.raw());
            }
        }
    }
    // Inputs are finite by construction; a non-finite value hashes as an explicit marker so
    // that the comparison still changes (and evaluation reports the problem).
    let bytes = w.finish().unwrap_or_else(|_| b"non-finite".to_vec());
    ContentHash::of(HashDomain::EvalModifier, &bytes)
}

/// The copy transforms of an array step (identity first); `None` for an invalid array or a
/// step that is not an array.
pub(crate) fn array_transforms(m: &Modifier) -> Option<Vec<Affine>> {
    match m {
        Modifier::LinearArray { count, step } => {
            if *count == 0 || !step.x.is_finite() || !step.y.is_finite() {
                return None;
            }
            (0..*count)
                .map(|k| Affine::translate(Vector::new(step.x * f64::from(k), step.y * f64::from(k))).ok())
                .collect()
        }
        Modifier::GridArray(g) => burnburn_geometry::array::grid_array(g).ok(),
        Modifier::CircularArray(c) => burnburn_geometry::array::circular_array(c).ok(),
        _ => None,
    }
}

/// Evaluates chain `id` against `lookup`. Pure and deterministic.
pub(crate) fn evaluate_chain<'a>(
    id: ModifierId,
    chain: &ModifierChain,
    lookup: &dyn Fn(GeometryId) -> Option<&'a GeometryObject>,
) -> DocumentPatch {
    let owner = DerivedOwner::Modifier(id);
    let inputs = chain_inputs(id, chain, lookup);
    let failed = |diagnosis| DocumentPatch {
        owner,
        inputs,
        outputs: Vec::new(),
        status: EvalStatus::Failed { diagnosis },
        solution: OwnerSolution::Keep,
    };
    let missing: Vec<GeometryId> = chain.inputs().into_iter().filter(|g| lookup(*g).is_none()).collect();
    if !missing.is_empty() {
        return failed(EvalDiagnosis::MissingSources { objects: missing });
    }
    let Some(source) = lookup(chain.source) else {
        return failed(EvalDiagnosis::MissingSources { objects: vec![chain.source] });
    };
    let mut path: Path = (**source.path()).clone();
    let mut origins: SegmentOrigins = identity_origins(source);
    let never = CancellationToken::never();
    let tol = chain.tolerance;
    for (index, step) in chain.steps.iter().enumerate().filter(|(_, s)| s.enabled) {
        let fail = |failure| EvalDiagnosis::ModifierStep { step: index, failure };
        match &step.modifier {
            Modifier::Offset { distance, join } => {
                let out = offset(&path, distance.mm(), *join, &tol, &never).map_err(|e| match e {
                    OffsetError::OpenContour { .. } => fail(ModifierFailure::OpenContour),
                    OffsetError::Collapsed => fail(ModifierFailure::Empty),
                    _ => fail(ModifierFailure::OutOfRange),
                });
                let out = match out {
                    Ok(p) => p,
                    Err(d) => return failed(d),
                };
                origins = offset_origins(&out, &path, &origins, distance.mm().abs(), tol.mm());
                path = out;
            }
            Modifier::Boolean { op, with } => {
                let options = BooleanOptions { fill: FillRule::EvenOdd, tolerance: tol };
                for g in with {
                    let Some(operand) = lookup(*g) else {
                        return failed(EvalDiagnosis::MissingSources { objects: vec![*g] });
                    };
                    let w = operand.path();
                    let out = match boolean(&path, w, *op, &options, &never) {
                        Ok(p) => p,
                        Err(BooleanError::OpenContour { .. }) => return failed(fail(ModifierFailure::OpenContour)),
                        Err(_) => return failed(fail(ModifierFailure::OutOfRange)),
                    };
                    let w_origins = identity_origins(operand);
                    origins = boolean_origins(&out, &[(&path, &origins), (w, &w_origins)], tol.mm());
                    path = out;
                }
            }
            Modifier::LinearArray { .. }
            | Modifier::GridArray(_)
            | Modifier::CircularArray(_)
            | Modifier::PathArray { .. } => {
                let transforms = match &step.modifier {
                    Modifier::PathArray { count, along, orient } => {
                        let Some(carrier) = lookup(*along) else {
                            return failed(EvalDiagnosis::MissingSources { objects: vec![*along] });
                        };
                        path_array(carrier.path(), &PathArray { count: *count, orient: *orient }, &tol).ok()
                    }
                    other => array_transforms(other),
                };
                let Some(transforms) = transforms else {
                    return failed(fail(ModifierFailure::InvalidValue));
                };
                let copies = u32::try_from(transforms.len()).unwrap_or(u32::MAX);
                let mut subpaths = Vec::new();
                let mut next = Vec::new();
                for (k, m) in (0u32..).zip(&transforms) {
                    subpaths.extend(path.transform(m).subpaths().iter().cloned());
                    next.extend(origins.iter().map(|sub| {
                        sub.iter()
                            .map(|c| {
                                let shift = |o: EdgeOrigin| EdgeOrigin {
                                    copy: o.copy.saturating_mul(copies).saturating_add(k),
                                    ..o
                                };
                                c.map(|c| Cand { origin: shift(c.origin), also: c.also.map(shift) })
                            })
                            .collect::<Vec<_>>()
                    }));
                }
                path = Path::from_subpaths(subpaths);
                origins = next;
            }
        }
        if path.is_empty() {
            return failed(fail(ModifierFailure::Empty));
        }
    }
    let edges = runs(&path, &origins);
    let key = DerivedKey { owner, local: 0 };
    DocumentPatch {
        owner,
        inputs,
        outputs: vec![DerivedOutput { key, kind: ObjectKind::Path, path, name: Some(chain.name.clone()) }],
        status: EvalStatus::Valid,
        solution: OwnerSolution::Provenance(vec![(0, edges)]),
    }
}

/// The modifier evaluator as a document [`Evaluator`] (for background tasks: evaluate a
/// snapshot, deliver the patch with `CommandPayload::ApplyPatch`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ModifierEvaluator;

impl Evaluator for ModifierEvaluator {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Modifier
    }

    fn evaluate(&self, snapshot: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        let lookup = |g: GeometryId| snapshot.object(g);
        match owner {
            DerivedOwner::Modifier(id) => match snapshot.structure().modifier(id) {
                Some(chain) => evaluate_chain(id, chain, &lookup),
                None => unknown(owner),
            },
            _ => unknown(owner),
        }
    }
}

fn unknown(owner: DerivedOwner) -> DocumentPatch {
    DocumentPatch {
        owner,
        inputs: ContentHash::of(HashDomain::EvalModifier, b"unknown-owner"),
        outputs: Vec::new(),
        status: EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidDefinition },
        solution: OwnerSolution::Keep,
    }
}

impl CanonicalEncode for ModifierChain {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.str(&self.name);
        out.u64(self.source.raw());
        out.f64(self.tolerance.mm());
        out.tag(match self.mode {
            EvalMode::Inline => "inline",
            EvalMode::Background => "background",
        });
        out.u64(self.steps.len() as u64);
        for s in &self.steps {
            out.bool(s.enabled);
            match &s.modifier {
                Modifier::Offset { distance, join } => {
                    out.tag("offset");
                    out.f64(distance.mm());
                    out.tag(join_tag(*join));
                }
                Modifier::Boolean { op, with } => {
                    out.tag("boolean");
                    out.tag(op_tag(*op));
                    out.u64(with.len() as u64);
                    for g in with {
                        out.u64(g.raw());
                    }
                }
                Modifier::LinearArray { count, step } => {
                    out.tag("linear-array");
                    out.u64(u64::from(*count));
                    out.f64(step.x);
                    out.f64(step.y);
                }
                Modifier::GridArray(g) => {
                    out.tag("grid-array");
                    out.u64(u64::from(g.columns));
                    out.u64(u64::from(g.rows));
                    out.f64(g.pitch_x.mm());
                    out.f64(g.pitch_y.mm());
                }
                Modifier::PathArray { count, along, orient } => {
                    out.tag("path-array");
                    out.u64(u64::from(*count));
                    out.u64(along.raw());
                    out.bool(*orient);
                }
                Modifier::CircularArray(c) => {
                    out.tag("circular-array");
                    out.u64(u64::from(c.count));
                    out.f64(c.center.x);
                    out.f64(c.center.y);
                    out.f64(c.sweep_deg);
                    out.bool(c.rotate_items);
                    out.f64(c.reference.x);
                    out.f64(c.reference.y);
                }
            }
        }
    }
}

/// Distance from `q` to one segment starting at `from`.
fn distance_to(from: Point, seg: Segment, q: Point) -> f64 {
    if let Segment::Line { to } = seg {
        let d = to - from;
        let len2 = d.dot(d);
        let t = if len2 > 0.0 { ((q - from).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
        return from.lerp(to, t).distance(q);
    }
    Subpath::new(from, vec![seg], false)
        .ok()
        .and_then(|sp| Path::from_subpaths(vec![sp]).nearest(q))
        .map_or(f64::INFINITY, |n| n.distance)
}

/// Boolean provenance by containment (contract IM21): an output segment takes the origin of
/// every input segment it lies on (start, middle and end within twice the flattening
/// tolerance). One → that origin; two (coincident input edges) → both, which makes references
/// to it ambiguous; none → no origin. Deterministic: inputs in order, segments in order.
fn boolean_origins(out: &Path, inputs: &[(&Path, &SegmentOrigins)], tol: f64) -> SegmentOrigins {
    let eps = 2.0 * tol + 1e-9;
    out.subpaths()
        .iter()
        .map(|sp| {
            drawn_segments(sp)
                .into_iter()
                .map(|(from, seg)| {
                    let samples = [from, seg.point_at(from, 0.5), seg.end()];
                    let mut found: Vec<Cand> = Vec::new();
                    for (path, origins) in inputs {
                        for (si, isp) in path.subpaths().iter().enumerate() {
                            for (gi, (ifrom, iseg)) in drawn_segments(isp).into_iter().enumerate() {
                                if samples.iter().all(|q| distance_to(ifrom, iseg, *q) <= eps)
                                    && let Some(c) = origins.get(si).and_then(|s| s.get(gi)).copied().flatten()
                                {
                                    found.push(c);
                                }
                            }
                        }
                    }
                    let mut all = found.iter().flat_map(|c| std::iter::once(c.origin).chain(c.also));
                    let first = all.next()?;
                    let also = all.find(|o| *o != first);
                    Some(Cand { origin: first, also })
                })
                .collect()
        })
        .collect()
}

/// Offset provenance: each output segment takes the origin of the input segment its midpoint
/// lies |d| away from (within twice the tolerance), with the closest point strictly inside
/// that segment. Join segments (closest point at a vertex) get no origin.
fn offset_origins(out: &Path, input: &Path, origins: &SegmentOrigins, distance: f64, tol: f64) -> SegmentOrigins {
    let eps = 2.0 * tol + 1e-9;
    out.subpaths()
        .iter()
        .map(|sp| {
            drawn_segments(sp)
                .into_iter()
                .map(|(from, seg)| {
                    let n = input.nearest(seg.point_at(from, 0.5))?;
                    let interior = n.t > 1e-6 && n.t < 1.0 - 1e-6;
                    if (n.distance - distance).abs() <= eps && interior {
                        origins.get(n.at.subpath).and_then(|s| s.get(n.at.segment)).copied().flatten()
                    } else {
                        None
                    }
                })
                .collect()
        })
        .collect()
}

pub(crate) fn join_tag(j: OffsetJoin) -> &'static str {
    match j {
        OffsetJoin::Miter => "miter",
        OffsetJoin::Round => "round",
        OffsetJoin::Bevel => "bevel",
    }
}

pub(crate) fn op_tag(op: BooleanOp) -> &'static str {
    match op {
        BooleanOp::Union => "union",
        BooleanOp::Difference => "difference",
        BooleanOp::Intersection => "intersection",
        BooleanOp::Xor => "xor",
    }
}
