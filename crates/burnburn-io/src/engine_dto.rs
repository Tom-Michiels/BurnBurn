//! On-disk form of traces and generators (C-2, C-9; additive in schema 2: absent = none).
//! A recipe is `{ "engine": "name/version", "params": { name: value } }` with typed values
//! (`{"number": 0.05}`, `{"integer": 3}`, `{"bool": true}`, `{"text": "otsu"}`, `{"list": [..]}`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dto::v1::Num;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Value {
    Number(Num),
    Integer(i64),
    Bool(bool),
    Text(String),
    List(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub engine: String,
    pub params: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub id: u64,
    pub name: String,
    /// Asset key (`sha256-<hex>`) of the source image; must be in the container.
    pub source: String,
    pub recipe: Recipe,
    /// The engine's reproducibility key (hex), the trace's inputs hash.
    pub key: String,
    /// Affine coefficients `[a, b, c, d, e, f]`.
    pub placement: [Num; 6],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<Evaluation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generator {
    pub id: u64,
    pub name: String,
    pub generator: String,
    pub recipe: Recipe,
    pub placement: [Num; 6],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<Evaluation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evaluation {
    pub inputs: Option<String>,
    pub status: Status,
}

/// Failures other than "invalid definition" are stored as such: still blocked, re-evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Valid,
    Pending,
    InvalidDefinition,
}
