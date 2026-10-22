//! Text object *data* (DR-22, C-2). Module owner: Generators/tekst.
//!
//! The source (content, explicit font chain with optional embedded asset hashes, size, layout,
//! placement, mode) and the evaluation record live in the document. `burnburn-text` shapes it
//! and returns the outlines as a [`crate::DocumentPatch`]; when a font is missing the last valid
//! outlines stay and the record says `Failed` (production is blocked, never a silent fallback).

pub mod canonical;
mod command;
mod model;

pub use command::{TextCommand, TextError, TextLabel, TextLayer};
pub(crate) use command::{TextEffect, TextTxn, apply};
pub use model::{
    Align, FontRef, FontSize, FontSizeError, FontStyle, MM_PER_POINT, MissingGlyph, PathBinding, TextId, TextLayout,
    TextMode, TextObject, VAlign, VariableText,
};
