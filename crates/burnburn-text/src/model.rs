//! The text source is document data (`burnburn_document::text`, DR-22); re-exported here so
//! callers of this crate need one import path.

pub use burnburn_document::text::{
    Align, FontRef, FontSize, FontSizeError, FontStyle, MM_PER_POINT, MissingGlyph, TextId, TextLayout, TextMode,
    TextObject,
};
