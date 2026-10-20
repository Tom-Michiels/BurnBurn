//! Text to geometry for BurnBurn (IM18 text part, contracts-phase-c §IM18).
//!
//! - [`FontData`] parses font bytes once; [`FontSource`] resolves a [`FontRef`] (no system font
//!   scan in this crate).
//! - [`shape`] shapes with harfrust (kerning, ligatures), takes glyph outlines from skrifa and
//!   returns geometry in millimetres, line by line, with an explicit fallback chain. A character
//!   no font in the chain can draw is *reported* in [`ShapedText::missing`], never drawn as tofu.
//! - [`TextMode::SingleLine`] uses a small built-in stroke set (open engraving paths).
//! - [`TextEvaluator`] implements `burnburn_document::Evaluator` for `DerivedOwner::Text`: the
//!   text source is document data (`burnburn_document::text`, DR-22); a missing font gives
//!   `EvalDiagnosis::FontMissing` and the document keeps the last valid outlines.
//!
//! IME/composition belongs to the host; this crate only sees committed text.

mod evaluate;
mod font;
mod model;
mod outline;
mod shape;
mod single_line;

pub use evaluate::{EVALUATOR_VERSION, TextEvaluator, inputs_hash};
pub use font::{FontData, FontError, FontRef, FontSource, FontStyle, MemoryFontSource, switch_style};
pub use model::{
    Align, FontSize, FontSizeError, MM_PER_POINT, MissingGlyph, TextId, TextLayout, TextMode, TextObject, VAlign,
    VariableText,
};
pub use shape::{LineMetrics, PlacedGlyph, ShapedText, TextError, shape};
pub use single_line::{SINGLE_LINE_CHARS, single_line_supports};
