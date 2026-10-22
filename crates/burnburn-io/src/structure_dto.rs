//! On-disk form of the IM21 structure layer (additive in schema 2, C-3): modifier chains,
//! references, relations, part definitions/instances, plates, placements, and the evaluation
//! records and edge provenance of their derived outputs. Every list is omitted when empty.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dto::v1::{Num, Subpath, Xy};

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structure {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<Modifier>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<Reference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<Relation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub definitions: Vec<Definition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<Instance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plates: Vec<Plate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub placement: Vec<Placement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<Record>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<Provenance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modifier {
    pub id: u64,
    pub name: String,
    pub source: u64,
    pub tolerance_mm: Num,
    /// Evaluated by a background task (`ApplyPatch`) instead of inline.
    #[serde(default, skip_serializing_if = "is_false")]
    pub background: bool,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub enabled: bool,
    pub modifier: ModifierKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModifierKind {
    Offset {
        distance_mm: Num,
        join: Join,
    },
    Boolean {
        op: BoolOp,
        with: Vec<u64>,
    },
    LinearArray {
        count: u32,
        step_mm: Xy,
    },
    /// `columns × rows` copies (IM26 array as a modifier).
    GridArray {
        columns: u32,
        rows: u32,
        pitch_mm: Xy,
    },
    /// `count` copies around `center` over `sweep_deg`.
    CircularArray {
        count: u32,
        center: Xy,
        sweep_deg: Num,
        rotate_items: bool,
        reference: Xy,
    },
    /// `count` copies along the outline of object `along` (AR02, copy along path).
    PathArray {
        count: u32,
        along: u64,
        orient: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Join {
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoolOp {
    Union,
    Difference,
    Intersection,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeOrigin {
    pub source: u64,
    /// Stable edge key of the source object (DR-20).
    pub edge: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub copy: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeName {
    pub origin: EdgeOrigin,
    pub index: u32,
    pub of: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Sub {
    Edge { edge: EdgeName },
    VertexStart { edge: EdgeName },
    VertexEnd { edge: EdgeName },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometryRef {
    pub object: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub: Option<Sub>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub id: u64,
    pub name: String,
    pub target: GeometryRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RelationKind {
    Tangent { a: u64, b: u64 },
    Symmetric { a: u64, b: u64, axis: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relation {
    pub id: u64,
    pub relation: RelationKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Dim {
    Mm {
        value: Num,
    },
    Param {
        name: String,
    },
    /// A formula over the parameters of the part (G8-editor).
    Expr {
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PartShape {
    Rectangle { x: Dim, y: Dim, width: Dim, height: Dim },
    Circle { x: Dim, y: Dim, radius: Dim },
    Fixed { subpaths: Vec<Subpath> },
    /// Fixed geometry whose box follows parameters and formulas (G9-editor).
    Fitted { subpaths: Vec<Subpath>, x: Dim, y: Dim, width: Dim, height: Dim },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub key: u32,
    pub name: String,
    pub shape: PartShape,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub id: u64,
    pub name: String,
    pub parameters_mm: BTreeMap<String, Num>,
    pub shapes: Vec<Shape>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variants_mm: BTreeMap<String, BTreeMap<String, Num>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub id: u64,
    pub definition: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides_mm: BTreeMap<String, Num>,
    /// `[a, b, c, d, e, f]` as in `geometry::Affine`.
    pub transform: [Num; 6],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plate: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plate {
    pub id: u64,
    pub name: String,
    pub width_mm: Num,
    pub height_mm: Num,
    pub material: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness_mm: Option<Num>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub object: u64,
    pub plate: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Owner {
    Modifier { id: u64 },
    Part { id: u64 },
}

/// Evaluation status of a modifier/part owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Status {
    Valid,
    Pending,
    MissingSources { objects: Vec<u64> },
    ModifierStep { step: usize, failure: Failure },
    InvalidParameters { parameter: Option<String> },
    InvalidDefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    OpenContour,
    Empty,
    OutOfRange,
    InvalidValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub owner: Owner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<String>,
    pub status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceEdge {
    pub subpath: u32,
    pub first: u32,
    pub count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<EdgeOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub also: Option<EdgeOrigin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub owner: Owner,
    pub local: u64,
    pub edges: Vec<ProvenanceEdge>,
}

/// Stable edge keys of a drawn object (DR-20), only when they differ from 0..n.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeKeys {
    pub next: u32,
    pub subpaths: Vec<Vec<u32>>,
}
