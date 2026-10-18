//! Text *source* data (DR-22, C-2; module owner: Generators/tekst): content, font chain, size,
//! layout and mode. It lives in the document so undo, the project file and the project hash see
//! it. Shaping and glyph outlines are `burnburn-text`'s evaluator, which reads a snapshot and
//! returns a [`crate::DocumentPatch`] with the derived outlines (contract §11).

use std::fmt;

use burnburn_geometry::Affine;
use burnburn_types::{ContentHash, Length, StableId};

/// Identity of a text object in a document. Allocated from the project's shared id counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextId(u64);

impl StableId for TextId {
    fn from_raw(raw: u64) -> Self {
        TextId(raw)
    }
    fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for TextId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tx{}", self.0)
    }
}

/// Style axis that a [`FontRef`] asks for (family + style selects a face in a catalog).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FontStyle {
    pub fn tag(self) -> &'static str {
        match self {
            FontStyle::Regular => "regular",
            FontStyle::Bold => "bold",
            FontStyle::Italic => "italic",
            FontStyle::BoldItalic => "bold-italic",
        }
    }
}

/// What a text object stores about its font: a family/style and optionally the embedded asset
/// (content hash of the font bytes, the same key the project container uses for assets).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontRef {
    pub family: String,
    pub style: FontStyle,
    pub asset: Option<ContentHash>,
}

impl FontRef {
    pub fn new(family: impl Into<String>, style: FontStyle) -> Self {
        FontRef { family: family.into(), style, asset: None }
    }

    pub fn with_asset(mut self, asset: ContentHash) -> Self {
        self.asset = Some(asset);
        self
    }
}

/// Millimetres per typographic point (1 pt = 1/72 in).
pub const MM_PER_POINT: f64 = 25.4 / 72.0;

/// The em size of the text. Stored in millimetres; [`FontSize::from_points`] converts explicitly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSize {
    em_mm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontSizeError {
    /// Size must be finite and strictly positive.
    NotPositive,
}

impl fmt::Display for FontSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("font size must be a positive finite value")
    }
}

impl std::error::Error for FontSizeError {}

impl FontSize {
    /// Em square of `em` millimetres.
    pub fn from_mm(em: Length) -> Result<Self, FontSizeError> {
        if em.mm() > 0.0 { Ok(FontSize { em_mm: em.mm() }) } else { Err(FontSizeError::NotPositive) }
    }

    /// Em square of `points` typographic points (1 pt = 25.4/72 mm).
    pub fn from_points(points: f64) -> Result<Self, FontSizeError> {
        if points.is_finite() && points > 0.0 {
            Ok(FontSize { em_mm: points * MM_PER_POINT })
        } else {
            Err(FontSizeError::NotPositive)
        }
    }

    pub fn em_mm(self) -> f64 {
        self.em_mm
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Outline text (closed glyph contours) or single-line stroke text (open engraving paths).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextMode {
    Outline,
    /// Built-in BurnBurn stroke set (digits, A–Z and a few signs); never closed or filled.
    SingleLine,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLayout {
    pub align: Align,
    /// Baseline-to-baseline distance as a multiple of the em size.
    pub line_spacing: f64,
    /// Extra space after every cluster (not after the last of a line), in mm. May be negative.
    pub letter_spacing_mm: f64,
    /// OpenType `kern` feature.
    pub kerning: bool,
    /// Standard ligatures (OpenType `liga`/`clig`).
    pub ligatures: bool,
}

impl Default for TextLayout {
    fn default() -> Self {
        TextLayout { align: Align::Left, line_spacing: 1.2, letter_spacing_mm: 0.0, kerning: true, ligatures: true }
    }
}

/// A text object as document data. `placement` maps text space (mm, y-up, first baseline at
/// y = 0, alignment point at x = 0) to document millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct TextObject {
    pub content: String,
    pub font: FontRef,
    /// Explicit fallback chain, tried in order per character after `font`.
    pub fallback: Vec<FontRef>,
    pub size: FontSize,
    pub layout: TextLayout,
    pub placement: Affine,
    pub mode: TextMode,
}

impl TextObject {
    pub fn new(content: impl Into<String>, font: FontRef, size: FontSize) -> Self {
        TextObject {
            content: content.into(),
            font,
            fallback: Vec::new(),
            size,
            layout: TextLayout::default(),
            placement: Affine::IDENTITY,
            mode: TextMode::Outline,
        }
    }
}

/// A character of a text object that no font in its chain can draw. It gets no geometry (never
/// tofu) and is shown to the user; the outlines of the other characters are current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingGlyph {
    pub ch: char,
    /// Byte offset in `content`.
    pub byte: usize,
    pub line: usize,
}
