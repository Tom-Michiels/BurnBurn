//! Import of external vector files at physical scale (IM10, P03).
//!
//! Every parser is pure (bytes in, data out), bounded by [`ParseLimits`] and never panics on
//! any input; [`parse_svg`] and [`parse_dxf`] are the fuzz entry points. A parse yields an
//! [`ImportDraft`]: geometry in world millimetres (y-up) plus an [`ImportReport`] naming the unit
//! decision, every skipped entity and every open contour. Committing is a separate step that
//! builds one `GeometryCommand::Import`, so the document only changes through its command route.

mod dxf;
mod hpgl;
mod lbrn2;
mod pdf;
mod raster;
mod svg;
mod xml;

use std::fmt;

use burnburn_document::{GeometryCommand, ImportItem};
use burnburn_geometry::Path;
use burnburn_types::ContentHash;

pub use dxf::{DxfDocument, DxfScale, parse_dxf, parse_dxf_with};
pub use hpgl::{parse_hpgl, parse_hpgl_with};
pub use lbrn2::{
    AssetNote, Assumption, CutKind, CutSettingProposal, EmbeddedBitmap, FontUse, ImageProposal, ImportNote, Lbrn2Draft,
    Lbrn2Item, Lbrn2Report, NoteKind, ProjectInfo, ProjectNotes, SUPPORTED, UnmappedValue, parse_lbrn2,
    parse_lbrn2_with, supported_attributes,
};
pub use pdf::{PdfDocument, PdfKind, PdfPage, PdfPages, parse_ai_with, parse_pdf, parse_pdf_with};
pub use raster::{RasterError, RasterFormat, RasterInfo, RasterLimits, inspect_raster};
pub use svg::{parse_svg, parse_svg_with};

/// Resource bounds for untrusted files. Exceeding one is a typed error, never a partial import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseLimits {
    pub max_bytes: usize,
    /// Elements (SVG) or entities (DXF).
    pub max_entities: usize,
    /// Element nesting depth (SVG).
    pub max_depth: usize,
    /// Path commands, polyline vertices and spline control points in total.
    pub max_path_commands: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        ParseLimits { max_bytes: 64 << 20, max_entities: 200_000, max_depth: 64, max_path_commands: 4_000_000 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    TooLarge {
        limit: usize,
        actual: usize,
    },
    TooManyEntities {
        limit: usize,
    },
    TooDeep {
        limit: usize,
    },
    TooManyCommands {
        limit: usize,
    },
    NotUtf8,
    /// Syntax error; `at` is a byte offset (SVG) or line number (DXF).
    Malformed {
        at: usize,
        reason: &'static str,
    },
    Unsupported(&'static str),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::TooLarge { limit, actual } => write!(f, "file is {actual} bytes, limit {limit}"),
            ParseError::TooManyEntities { limit } => write!(f, "more than {limit} entities"),
            ParseError::TooDeep { limit } => write!(f, "nesting deeper than {limit}"),
            ParseError::TooManyCommands { limit } => write!(f, "more than {limit} path commands"),
            ParseError::NotUtf8 => f.write_str("file is not UTF-8 text"),
            ParseError::Malformed { at, reason } => write!(f, "malformed input at {at}: {reason}"),
            ParseError::Unsupported(what) => write!(f, "unsupported: {what}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Why an import cannot be committed yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImportError {
    /// The DXF declares no (or an unknown) unit; the user must choose one first.
    ScaleRequired,
    /// Nothing importable was found.
    Empty,
    /// The created ids do not match the draft's items (LBRN2 operation proposals).
    ItemCountMismatch,
    /// The format is recognised but not imported by this build (PDF, IM28).
    NotSupportedYet(ImportFormat),
}

/// File formats the import route knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImportFormat {
    Svg,
    Dxf,
    Lbrn2,
    Png,
    Jpeg,
    /// HP-GL / PLT plotter files.
    Hpgl,
    /// PDF vector paths (own reader, see IM28 W2-L6).
    Pdf,
    /// Illustrator files: PDF-compatible `.ai`; legacy PostScript `.ai` is refused by name.
    Ai,
}

impl ImportFormat {
    /// By file extension (case-insensitive), `None` when unknown.
    pub fn from_extension(ext: &str) -> Option<Self> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "svg" => ImportFormat::Svg,
            "dxf" => ImportFormat::Dxf,
            "lbrn2" => ImportFormat::Lbrn2,
            "png" => ImportFormat::Png,
            "jpg" | "jpeg" => ImportFormat::Jpeg,
            "plt" | "hpgl" | "hgl" => ImportFormat::Hpgl,
            "pdf" => ImportFormat::Pdf,
            "ai" => ImportFormat::Ai,
            _ => return None,
        })
    }

    /// Whether this build can import the format.
    pub fn check_supported(self) -> Result<(), ImportError> {
        Ok(())
    }
}

/// The result of the analyse step for one file (contract §14 two-step form).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Analysis {
    Vector(ImportDraft),
    /// DXF: the scale decision comes before the draft.
    Dxf(DxfDocument),
    Lbrn2(Lbrn2Draft),
    Raster(RasterInfo),
    /// PDF or Illustrator: the page choice comes before the draft.
    Pdf(PdfDocument),
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AnalyzeError {
    Parse(ParseError),
    Raster(RasterError),
    Import(ImportError),
}

/// Analyses `bytes` as `format`.
pub fn analyze(format: ImportFormat, bytes: &[u8]) -> Result<Analysis, AnalyzeError> {
    format.check_supported().map_err(AnalyzeError::Import)?;
    match format {
        ImportFormat::Svg => parse_svg(bytes).map(Analysis::Vector).map_err(AnalyzeError::Parse),
        ImportFormat::Dxf => parse_dxf(bytes).map(Analysis::Dxf).map_err(AnalyzeError::Parse),
        ImportFormat::Lbrn2 => parse_lbrn2(bytes).map(Analysis::Lbrn2).map_err(AnalyzeError::Parse),
        ImportFormat::Png | ImportFormat::Jpeg => {
            inspect_raster(bytes, &RasterLimits::default()).map(Analysis::Raster).map_err(AnalyzeError::Raster)
        }
        ImportFormat::Hpgl => parse_hpgl(bytes).map(Analysis::Vector).map_err(AnalyzeError::Parse),
        ImportFormat::Pdf => parse_pdf(bytes).map(Analysis::Pdf).map_err(AnalyzeError::Parse),
        ImportFormat::Ai => analyze_ai(bytes),
    }
}

fn analyze_ai(bytes: &[u8]) -> Result<Analysis, AnalyzeError> {
    const LEGACY: &str = "legacy PostScript Illustrator file: save it with PDF compatibility or export SVG";
    let head = &bytes[..bytes.len().min(1024)];
    if head.windows(5).any(|w| w == b"%PDF-") {
        return parse_ai_with(bytes, &ParseLimits::default()).map(Analysis::Pdf).map_err(AnalyzeError::Parse);
    }
    if head.starts_with(b"%!PS-Adobe") {
        return Err(AnalyzeError::Parse(ParseError::Unsupported(LEGACY)));
    }
    Err(AnalyzeError::Parse(ParseError::Malformed { at: 0, reason: "not an Illustrator or PDF file" }))
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::ScaleRequired => f.write_str("the drawing has no unit; choose a scale before importing"),
            ImportError::Empty => f.write_str("nothing to import"),
            ImportError::ItemCountMismatch => f.write_str("created objects do not match the imported items"),
            ImportError::NotSupportedYet(fmt) => write!(f, "{fmt:?} import is not supported yet"),
        }
    }
}

impl std::error::Error for ImportError {}

/// How SVG user units were turned into millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvgUnitBasis {
    /// `width`/`height` with an absolute unit and a `viewBox`.
    ViewBoxToPhysicalSize,
    /// Absolute `width`/`height` without `viewBox`: user units are CSS pixels by the spec.
    CssPixelsBySpec,
    /// No absolute size at all: 96 user units per inch assumed (reported to the user).
    AssumedCssPixels,
}

/// The unit decision of an import, shown in the report and the scale preview.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum UnitDecision {
    Svg {
        mm_per_user_unit: f64,
        basis: SvgUnitBasis,
    },
    /// `$INSUNITS` code from the file and its millimetre factor.
    DxfDeclared {
        insunits: i32,
        mm_per_unit: f64,
    },
    /// The user's explicit choice (required for unitless files, an override otherwise).
    DxfExplicit {
        mm_per_unit: f64,
    },
    /// PDF user space: `mm_per_unit` is 25.4/72 times the first imported page's `UserUnit`.
    Pdf {
        mm_per_unit: f64,
        pages: usize,
    },
    /// HP-GL plotter units (0.025 mm) or user units scaled by `IP`/`SC`.
    Hpgl {
        mm_per_unit: f64,
        basis: HpglUnitBasis,
    },
}

/// How HP-GL coordinates were turned into millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HpglUnitBasis {
    /// Plotter units: 1016 per inch, 0.025 mm each.
    PlotterUnits,
    /// `SC` user units mapped onto the `IP` reference points (result still in plotter units).
    ScaledByIpSc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkipReason {
    /// An element/entity type we do not convert (text, images, block references, …).
    UnsupportedElement,
    UnsupportedEntity,
    /// Imported, but an effect on it was not applied (clip-path, mask, filter, …).
    UnsupportedEffect(&'static str),
    /// Not rendered (`display:none`, `visibility:hidden`, inside `<defs>`).
    Hidden,
    /// Present but unusable (degenerate or out-of-range data).
    Invalid(&'static str),
    /// A curve type we cannot convert exactly (rational/periodic spline, fit points only, …).
    UnsupportedCurve(&'static str),
}

/// One reported entity, with a reference back into the source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub entity: String,
    pub kind: String,
    pub reason: SkipReason,
}

/// An open subpath of an imported item (index into `ImportDraft::items`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenContour {
    pub item: usize,
    pub subpath: usize,
    pub gap_mm: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportReport {
    pub units: UnitDecision,
    pub skipped: Vec<Skipped>,
    pub open_contours: Vec<OpenContour>,
}

/// One imported object in world millimetres (y-up).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedPath {
    /// Display name (SVG id or element, DXF layer).
    pub name: String,
    /// Source reference: SVG `id` (or `element#n`), DXF `TYPE#handle` (or `TYPE#n`).
    pub entity: String,
    pub path: Path,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDraft {
    pub items: Vec<ImportedPath>,
    pub report: ImportReport,
}

impl ImportDraft {
    pub(crate) fn new(items: Vec<ImportedPath>, units: UnitDecision, skipped: Vec<Skipped>) -> Self {
        let mut open_contours = Vec::new();
        for (i, item) in items.iter().enumerate() {
            for (s, sp) in item.path.subpaths().iter().enumerate() {
                if !sp.is_closed() {
                    open_contours.push(OpenContour { item: i, subpath: s, gap_mm: sp.start().distance(sp.end()) });
                }
            }
        }
        ImportDraft { items, report: ImportReport { units, skipped, open_contours } }
    }

    /// The single document command that commits this import (one undo step).
    pub fn into_command(self, asset: ContentHash) -> Result<GeometryCommand, ImportError> {
        if self.items.is_empty() {
            return Err(ImportError::Empty);
        }
        let items =
            self.items.into_iter().map(|i| ImportItem { name: i.name, entity: Some(i.entity), path: i.path }).collect();
        Ok(GeometryCommand::Import { asset, items })
    }
}

pub(crate) fn check_size(bytes: &[u8], limits: &ParseLimits) -> Result<(), ParseError> {
    if bytes.len() > limits.max_bytes {
        return Err(ParseError::TooLarge { limit: limits.max_bytes, actual: bytes.len() });
    }
    Ok(())
}
