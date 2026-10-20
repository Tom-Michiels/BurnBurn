//! Shaping and layout: content → positioned glyphs → outlines in document millimetres.

use std::fmt;
use std::ops::Range;

use std::sync::Arc;

use burnburn_geometry::{
    BooleanOp, BooleanOptions, FillRule, GeometryError, Path, Point, ProductionTolerance, Subpath,
};
use burnburn_types::{CancellationToken, ContentHash, Length};
use harfrust::{Buffer, Direction, Feature, ShapeOptions, ShaperFont, Tag};

use crate::font::{FontData, FontError, FontSource};
use crate::model::{Align, MissingGlyph, TextMode, TextObject, VAlign};
use crate::outline::{UnitMap, glyph_subpaths, path_of};
use crate::single_line::{self, glyph_strokes};

/// One shaped glyph in *text space* (mm, y-up, first baseline at y = 0, before `placement`).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedGlyph {
    /// Index in the font chain: 0 = `font`, 1.. = `fallback[i - 1]`. Always 0 for single-line text.
    pub font: usize,
    pub glyph_id: u32,
    /// Byte offset in `content` where this glyph's cluster starts.
    pub cluster: usize,
    pub line: usize,
    /// Pen position of the glyph origin (offsets included).
    pub origin: Point,
    pub advance_mm: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineMetrics {
    /// Byte range of the line in `content` (without the newline).
    pub bytes: Range<usize>,
    /// Advance width including letter spacing between clusters, in mm.
    pub width_mm: f64,
    /// Baseline y in text space (0 for the first line, negative below).
    pub baseline_y: f64,
    /// Pen x where the line starts after alignment.
    pub start_x: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapedText {
    pub glyphs: Vec<PlacedGlyph>,
    pub lines: Vec<LineMetrics>,
    /// Outlines per line in document mm (after `placement`). Empty lines give an empty path.
    pub line_outlines: Vec<Path>,
    /// All outlines in document mm.
    pub outlines: Path,
    /// Outline per glyph (same order as `glyphs`) in *text space*, before `placement`; for
    /// layouts that place glyphs themselves (text on a path).
    pub glyph_outlines: Vec<Path>,
    /// Asset hashes of the faces that contributed glyphs, in chain order, without duplicates.
    pub used_fonts: Vec<ContentHash>,
    pub missing: Vec<MissingGlyph>,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TextError {
    Font(FontError),
    /// Line spacing must be finite and positive; letter spacing finite.
    InvalidLayout,
    /// The shaper rejected the run (malformed font tables).
    Shaping,
    Geometry(GeometryError),
    /// Welding the glyphs of a line failed (coordinates out of range for the boolean).
    Weld,
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextError::Font(e) => write!(f, "{e}"),
            TextError::InvalidLayout => f.write_str("line spacing must be positive and letter spacing finite"),
            TextError::Shaping => f.write_str("the font could not shape this text"),
            TextError::Geometry(e) => write!(f, "{e}"),
            TextError::Weld => f.write_str("the glyph outlines could not be welded"),
        }
    }
}

impl std::error::Error for TextError {}

impl From<FontError> for TextError {
    fn from(e: FontError) -> Self {
        TextError::Font(e)
    }
}

impl From<GeometryError> for TextError {
    fn from(e: GeometryError) -> Self {
        TextError::Geometry(e)
    }
}

/// Shapes `obj` with its font chain. Errors only when a font in the chain cannot be loaded or
/// the layout values are invalid; unsupported characters are reported in `missing`.
///
/// Each line is split into runs of consecutive characters drawn by the same font (the first in
/// the chain that maps the character); every run is shaped left-to-right with harfrust.
/// Bidirectional reordering is not part of v1.
pub fn shape(obj: &TextObject, fonts: &dyn FontSource) -> Result<ShapedText, TextError> {
    let layout = &obj.layout;
    if !(layout.line_spacing.is_finite() && layout.line_spacing > 0.0)
        || !layout.letter_spacing_mm.is_finite()
        || !layout.v_space_mm.is_finite()
    {
        return Err(TextError::InvalidLayout);
    }
    let em = obj.size.em_mm();
    let chain: Vec<Arc<FontData>> = match obj.mode {
        TextMode::Outline => {
            std::iter::once(&obj.font).chain(obj.fallback.iter()).map(|r| fonts.load(r)).collect::<Result<_, _>>()?
        }
        TextMode::SingleLine => Vec::new(),
    };

    let mut out = ShapedText {
        glyphs: Vec::new(),
        lines: Vec::new(),
        line_outlines: Vec::new(),
        outlines: Path::default(),
        glyph_outlines: Vec::new(),
        used_fonts: Vec::new(),
        missing: Vec::new(),
    };
    let mut used = vec![false; chain.len()];
    let mut all = Vec::new();
    let mut offset = 0;
    let pitch = layout.line_spacing * em + layout.v_space_mm;
    let line_count = obj.content.split('\n').count();
    let (ascent, descent) = match (obj.mode, chain.first()) {
        (TextMode::Outline, Some(f)) => {
            let scale = em / f64::from(f.units_per_em());
            let (a, d) = f.ascent_descent_units();
            (a * scale, d * scale)
        }
        _ => (single_line::CAP_HEIGHT_EM * em, 0.0),
    };
    // Shift of the whole block so that it hangs on y = 0 as `valign` asks.
    let last_baseline = -((line_count - 1) as f64) * pitch;
    let block_dy = match layout.valign {
        VAlign::Baseline => 0.0,
        VAlign::Top => -ascent,
        VAlign::Middle => -(ascent + (last_baseline - descent)) / 2.0,
        VAlign::Bottom => descent - last_baseline,
    };
    for (line_index, line) in obj.content.split('\n').enumerate() {
        let bytes = offset..offset + line.len();
        offset += line.len() + 1;
        let raw = match obj.mode {
            TextMode::Outline => shape_line(line, bytes.start, &chain, layout, em)?,
            TextMode::SingleLine => single_line_glyphs(line, bytes.start, em, layout.uppercase),
        };
        for m in raw.missing {
            out.missing.push(MissingGlyph { ch: m.0, byte: m.1, line: line_index });
        }
        // Pen positions with letter spacing between clusters (never after the last glyph).
        let mut pens = Vec::with_capacity(raw.glyphs.len());
        let mut pen = 0.0;
        for (i, g) in raw.glyphs.iter().enumerate() {
            pens.push(pen);
            pen += g.advance;
            if raw.glyphs.get(i + 1).is_some_and(|next| next.cluster != g.cluster) {
                pen += layout.letter_spacing_mm;
            }
        }
        let width = pen;
        let start_x = match layout.align {
            Align::Left => 0.0,
            Align::Center => -width / 2.0,
            Align::Right => -width,
        };
        let baseline_y = -(line_index as f64) * pitch + block_dy;
        let mut subpaths = Vec::new();
        for (g, pen) in raw.glyphs.iter().zip(pens) {
            let origin = Point::new(start_x + pen + g.offset.0, baseline_y + g.offset.1);
            let glyph = match obj.mode {
                TextMode::Outline => {
                    let font = chain.get(g.font).ok_or(TextError::Shaping)?;
                    if let Some(u) = used.get_mut(g.font) {
                        *u = true;
                    }
                    let scale = em / f64::from(font.units_per_em());
                    glyph_subpaths(font, g.glyph_id, UnitMap { origin, scale })?
                }
                TextMode::SingleLine => {
                    let ch = char::from_u32(g.glyph_id).unwrap_or(' ');
                    glyph_strokes(ch, origin, em / single_line::UNITS_PER_EM)?
                }
            };
            subpaths.extend(glyph.iter().cloned());
            out.glyph_outlines.push(path_of(glyph));
            out.glyphs.push(PlacedGlyph {
                font: g.font,
                glyph_id: g.glyph_id,
                cluster: g.cluster,
                line: line_index,
                origin,
                advance_mm: g.advance,
            });
        }
        if layout.welded && obj.mode == TextMode::Outline {
            subpaths = weld(subpaths, em)?;
        }
        let line_path = path_of(subpaths).transform(&obj.placement);
        all.extend(line_path.subpaths().iter().cloned());
        out.line_outlines.push(line_path);
        out.lines.push(LineMetrics { bytes, width_mm: width, baseline_y, start_x });
    }
    out.outlines = path_of(all);
    for (font, used) in chain.iter().zip(used) {
        if used && !out.used_fonts.contains(&font.hash()) {
            out.used_fonts.push(font.hash());
        }
    }
    Ok(out)
}

struct RawGlyph {
    font: usize,
    glyph_id: u32,
    cluster: usize,
    advance: f64,
    offset: (f64, f64),
}

struct RawLine {
    glyphs: Vec<RawGlyph>,
    missing: Vec<(char, usize)>,
}

fn single_line_glyphs(line: &str, base: usize, em: f64, uppercase: bool) -> RawLine {
    let advance = single_line::ADVANCE_UNITS * em / single_line::UNITS_PER_EM;
    let mut raw = RawLine { glyphs: Vec::new(), missing: Vec::new() };
    let (text, map) = case_mapped(line, uppercase);
    for (i, ch) in text.char_indices() {
        let source = base + map.get(i).copied().unwrap_or(0);
        if single_line::single_line_supports(ch) {
            raw.glyphs.push(RawGlyph {
                font: 0,
                glyph_id: u32::from(ch),
                cluster: source,
                advance,
                offset: (0.0, 0.0),
            });
        } else if ch != '\r' {
            raw.missing.push((ch, source));
        }
    }
    raw
}

/// The text to draw and, per byte of it, the byte offset in `line` of the source character it
/// came from (`ß` → `SS` gives both `S` the offset of `ß`). Without `uppercase` it is the line.
fn case_mapped(line: &str, uppercase: bool) -> (String, Vec<usize>) {
    if !uppercase {
        return (line.to_owned(), (0..line.len()).collect());
    }
    let mut text = String::with_capacity(line.len());
    let mut map = Vec::with_capacity(line.len());
    for (i, ch) in line.char_indices() {
        for up in ch.to_uppercase() {
            text.push(up);
            map.extend(std::iter::repeat_n(i, up.len_utf8()));
        }
    }
    (text, map)
}

/// Union of overlapping contours (non-zero fill: counters wind opposite to their outer contour
/// and stay open). Polygonal result within a tenth of the em's thousandth.
fn weld(subpaths: Vec<Subpath>, em: f64) -> Result<Vec<Subpath>, TextError> {
    if subpaths.is_empty() {
        return Ok(subpaths);
    }
    let tolerance = ProductionTolerance::new(Length::from_mm((em / 2000.0).max(1e-4)).map_err(|_| TextError::Weld)?)
        .map_err(|_| TextError::Weld)?;
    let options = BooleanOptions { fill: FillRule::NonZero, tolerance };
    let merged = burnburn_geometry::boolean(
        &Path::from_subpaths(subpaths),
        &Path::default(),
        BooleanOp::Union,
        &options,
        &CancellationToken::never(),
    )
    .map_err(|_| TextError::Weld)?;
    Ok(merged.subpaths().to_vec())
}

fn shape_line(
    line: &str,
    base: usize,
    chain: &[Arc<FontData>],
    layout: &crate::model::TextLayout,
    em: f64,
) -> Result<RawLine, TextError> {
    let mut raw = RawLine { glyphs: Vec::new(), missing: Vec::new() };
    // The text to draw (upper-cased on request) and, per byte, its source offset in `line`.
    let (line, map) = case_mapped(line, layout.uppercase);
    let line = line.as_str();
    // Runs of (font index, byte range within the drawn line).
    let mut runs: Vec<(usize, Range<usize>)> = Vec::new();
    for (i, ch) in line.char_indices() {
        if ch == '\r' {
            continue;
        }
        let end = i + ch.len_utf8();
        match chain.iter().position(|f| f.has_glyph(ch)) {
            Some(font) => match runs.last_mut() {
                Some((f, r)) if *f == font && r.end == i => r.end = end,
                _ => runs.push((font, i..end)),
            },
            None => raw.missing.push((ch, base + map.get(i).copied().unwrap_or(0))),
        }
    }
    let mut features = Vec::new();
    if !layout.kerning {
        features.push(Feature::new(Tag::new(b"kern"), 0, ..));
    }
    if !layout.ligatures {
        features.push(Feature::new(Tag::new(b"liga"), 0, ..));
        features.push(Feature::new(Tag::new(b"clig"), 0, ..));
    }
    for (font_index, range) in runs {
        let font = chain.get(font_index).ok_or(TextError::Shaping)?;
        let text = line.get(range.clone()).ok_or(TextError::Shaping)?;
        let mut buffer = Buffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        buffer.set_direction(Direction::LeftToRight);
        let shaper = ShaperFont::new(font.shaping());
        harfrust::shape(&shaper, &mut buffer, ShapeOptions::new().features(&features))
            .map_err(|_| TextError::Shaping)?;
        let scale = em / f64::from(font.units_per_em());
        for (info, pos) in buffer.glyph_infos().iter().zip(buffer.glyph_positions()) {
            raw.glyphs.push(RawGlyph {
                font: font_index,
                glyph_id: info.glyph_id,
                cluster: base + map.get(range.start + info.cluster as usize).copied().unwrap_or(0),
                advance: f64::from(pos.x_advance) * scale,
                offset: (f64::from(pos.x_offset) * scale, f64::from(pos.y_offset) * scale),
            });
        }
    }
    Ok(raw)
}
