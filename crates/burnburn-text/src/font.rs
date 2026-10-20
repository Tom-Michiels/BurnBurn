//! Fonts as immutable bytes. Parsing is done once at load; shaping (harfrust) and outlines (skrifa)
//! read the same bytes. No system font scan happens here: a [`FontSource`] (project assets first,
//! then the catalog that presentation/runtime supplies) resolves a [`FontRef`].

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use burnburn_types::{ContentHash, HashDomain};

pub use crate::model::{FontRef, FontStyle};
use skrifa::MetadataProvider;
use skrifa::raw::TableProvider;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FontError {
    /// The bytes are not a font we can read (or have no outline glyphs).
    Unreadable,
    /// The font has a zero or absurd units-per-em value.
    InvalidUnitsPerEm(u16),
    /// No source can provide this font (for example a missing font after reopening elsewhere).
    NotFound(FontRef),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontError::Unreadable => f.write_str("font bytes cannot be read"),
            FontError::InvalidUnitsPerEm(u) => write!(f, "font has invalid units per em {u}"),
            FontError::NotFound(r) => write!(f, "font '{}' ({}) is not available", r.family, r.style.tag()),
        }
    }
}

impl std::error::Error for FontError {}

/// One parsed font face. Cheap to share (`Arc`); the bytes are immutable.
pub struct FontData {
    bytes: Arc<Vec<u8>>,
    hash: ContentHash,
    shaping: harfrust::Font,
    units_per_em: u16,
}

impl fmt::Debug for FontData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontData")
            .field("hash", &self.hash)
            .field("bytes", &self.bytes.len())
            .field("units_per_em", &self.units_per_em)
            .finish()
    }
}

impl FontData {
    /// Parses font bytes (TrueType/OpenType, first face of a collection).
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, FontError> {
        let hash = ContentHash::of(HashDomain::Asset, &bytes);
        let bytes = Arc::new(bytes);
        let units_per_em = {
            let font = skrifa::FontRef::new(&bytes).map_err(|_| FontError::Unreadable)?;
            if font.outline_glyphs().format().is_none() {
                return Err(FontError::Unreadable);
            }
            let upem = font.head().map_err(|_| FontError::Unreadable)?.units_per_em();
            if !(16..=16384).contains(&upem) {
                return Err(FontError::InvalidUnitsPerEm(upem));
            }
            upem
        };
        let blob: Arc<dyn AsRef<[u8]> + Send + Sync> = bytes.clone();
        let shaping = harfrust::Font::new(blob, 0).ok_or(FontError::Unreadable)?;
        Ok(FontData { bytes, hash, shaping, units_per_em })
    }

    /// Asset key of these bytes (`ContentHash` in the asset domain).
    pub fn hash(&self) -> ContentHash {
        self.hash
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether the face maps `ch` to a real glyph (not `.notdef`).
    pub fn has_glyph(&self, ch: char) -> bool {
        match skrifa::FontRef::new(&self.bytes) {
            Ok(font) => font.charmap().map(ch).is_some_and(|g| g.to_u32() != 0),
            Err(_) => false,
        }
    }

    /// Typographic ascent and descent (positive, hhea) in font units.
    pub fn ascent_descent_units(&self) -> (f64, f64) {
        match skrifa::FontRef::new(&self.bytes) {
            Ok(font) => {
                let m = font.metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default());
                (f64::from(m.ascent), f64::from(-m.descent))
            }
            Err(_) => (0.0, 0.0),
        }
    }

    pub(crate) fn shaping(&self) -> &harfrust::Font {
        &self.shaping
    }

    pub(crate) fn outline_font(&self) -> Result<skrifa::FontRef<'_>, FontError> {
        skrifa::FontRef::new(&self.bytes).map_err(|_| FontError::Unreadable)
    }
}

/// Resolves a [`FontRef`] to font bytes. Implementations look at embedded project assets first;
/// they must never substitute an unrelated font silently.
pub trait FontSource {
    fn load(&self, font: &FontRef) -> Result<Arc<FontData>, FontError>;

    /// Styles this source can provide for `family` (empty when the source does not list them).
    /// The toolbar enables Bold/Italic from this; shaping never substitutes a missing style.
    fn available_styles(&self, _family: &str) -> Vec<FontStyle> {
        Vec::new()
    }

    /// Family names this source can list, sorted (empty when the source cannot enumerate its
    /// faces). The text toolbar's font choice is this list; a family is never invented.
    fn families(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The reference for the same family with Bold/Italic switched. It names another face, so an
/// embedded asset of the old face is not carried over: the face is looked up by family and
/// style (and reported missing, never replaced, when the source lacks it).
pub fn switch_style(font: &FontRef, bold: bool, italic: bool) -> FontRef {
    FontRef::new(font.family.clone(), FontStyle::from_flags(bold, italic))
}

/// An in-memory catalog: faces registered by asset hash and by family/style.
#[derive(Debug, Default, Clone)]
pub struct MemoryFontSource {
    by_asset: BTreeMap<ContentHash, Arc<FontData>>,
    by_name: BTreeMap<(String, FontStyle), Arc<FontData>>,
}

impl MemoryFontSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a face under its asset hash and `family`/`style`. Returns the [`FontRef`] that
    /// embeds it.
    pub fn insert(&mut self, family: &str, style: FontStyle, data: Arc<FontData>) -> FontRef {
        let r = FontRef::new(family, style).with_asset(data.hash());
        self.by_asset.insert(data.hash(), data.clone());
        self.by_name.insert((family.to_owned(), style), data);
        r
    }
}

impl FontSource for MemoryFontSource {
    /// An embedded asset wins; a reference whose asset is set but absent is *not* replaced by
    /// another face with the same family name (that would change outlines silently).
    fn load(&self, font: &FontRef) -> Result<Arc<FontData>, FontError> {
        let found = match font.asset {
            Some(hash) => self.by_asset.get(&hash),
            None => self.by_name.get(&(font.family.clone(), font.style)),
        };
        found.cloned().ok_or_else(|| FontError::NotFound(font.clone()))
    }

    fn available_styles(&self, family: &str) -> Vec<FontStyle> {
        self.by_name.keys().filter(|(f, _)| f == family).map(|(_, style)| *style).collect()
    }

    fn families(&self) -> Vec<String> {
        let mut families: Vec<String> = self.by_name.keys().map(|(f, _)| f.clone()).collect();
        families.dedup();
        families
    }
}
