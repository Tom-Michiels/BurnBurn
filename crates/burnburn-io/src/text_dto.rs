//! On-disk schema for text objects (DR-22; additive in schema 2: absent = no texts).
//! Owner of this file: Generators/tekst, reviewed by Kern.

use serde::{Deserialize, Serialize};

use crate::dto::v1::Num;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Text {
    pub id: u64,
    pub content: String,
    pub font: Font,
    pub fallback: Vec<Font>,
    /// Em size in millimetres.
    pub em_mm: Num,
    pub align: Align,
    pub line_spacing: Num,
    pub letter_spacing_mm: Num,
    pub kerning: bool,
    pub ligatures: bool,
    /// Affine coefficients `[a, b, c, d, e, f]`.
    pub placement: [Num; 6],
    pub mode: Mode,
    pub evaluation: Option<Evaluation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Font {
    pub family: String,
    pub style: Style,
    /// Asset key (`sha256-<hex>`) of embedded font bytes. Not required to be in the container:
    /// a missing font is an evaluation state, not a load error.
    pub asset: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Outline,
    SingleLine,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evaluation {
    pub inputs: Option<String>,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Status {
    Valid,
    Pending,
    FontMissing { font: Font },
    InvalidDefinition,
}
