//! Shaping and layout: content → positioned glyphs → outlines in document millimetres.

use std::fmt;
use std::ops::Range;

use std::sync::Arc;

use burnburn_geometry::{
    BooleanOp, BooleanOptions, FillRule, GeometryError, Path, Point, ProductionTolerance, Subpath,
};
use burnburn_types::{CancellationToken, ContentHash, Length};
use harfrust::{Buffer, Direction, Feature, ShapeOptions, ShaperFont, Tag};
use unicode_bidi::BidiInfo;

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
/// the chain that maps the character); every run is shaped with harfrust in the direction the Unicode Bidirectional Algorithm (UAX #9) gives it, and the runs sit in visual order.
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
    let shape_one = |line: &str, base: usize, kashida: usize| -> Result<RawLine, TextError> {
        Ok(match obj.mode {
            TextMode::Outline => shape_line(line, base, &chain, layout, em, kashida)?,
            TextMode::SingleLine => single_line_glyphs(line, base, em, layout.uppercase),
        })
    };
    // Justified text: every line but the last is stretched to the width of the widest one.
    let mut naturals: Vec<Option<RawLine>> = Vec::new();
    let mut target = None;
    if layout.align == Align::Justify && line_count > 1 {
        let mut at = 0;
        let mut widest: f64 = 0.0;
        for line in obj.content.split('\n') {
            let raw = shape_one(line, at, 0)?;
            at += line.len() + 1;
            widest = widest.max(pens_of(&raw, &obj.content, layout, &Stretch::NONE).1);
            naturals.push(Some(raw));
        }
        target = Some(widest);
    }
    for (line_index, line) in obj.content.split('\n').enumerate() {
        let bytes = offset..offset + line.len();
        offset += line.len() + 1;
        let mut raw = match naturals.get_mut(line_index).and_then(Option::take) {
            Some(raw) => raw,
            None => shape_one(line, bytes.start, 0)?,
        };
        for m in std::mem::take(&mut raw.missing) {
            out.missing.push(MissingGlyph { ch: m.0, byte: m.1, line: line_index });
        }
        let mut stretch = Stretch::NONE;
        if let Some(target) = target.filter(|_| line_index + 1 < line_count) {
            let natural = pens_of(&raw, &obj.content, layout, &stretch).1;
            if target - natural > STRETCH_EPS {
                if obj.mode == TextMode::Outline {
                    let widths = (natural, target);
                    let better = kashida_stretch(&shape_one, line, bytes.start, &obj.content, layout, &chain, widths)?;
                    if let Some(better) = better {
                        raw.glyphs = better.glyphs;
                    }
                }
                let extra = target - pens_of(&raw, &obj.content, layout, &stretch).1;
                if extra > STRETCH_EPS {
                    let (gaps, boundaries) = stretch_points(&raw, &obj.content);
                    if gaps > 0 {
                        stretch.gap = extra / gaps as f64;
                    } else if boundaries > 0 {
                        stretch.letter = extra / boundaries as f64;
                    }
                }
            }
        }
        let (pens, width) = pens_of(&raw, &obj.content, layout, &stretch);
        let start_x = match layout.align {
            Align::Left | Align::Justify => 0.0,
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

/// Smaller stretches than this (mm) are not worth a second pass.
const STRETCH_EPS: f64 = 1e-6;
/// At most this many kashida (tatweel) per word.
const MAX_KASHIDA_PER_WORD: usize = 6;

/// Extra space a justified line adds: per word gap, or per letter boundary where there is no gap.
struct Stretch {
    gap: f64,
    letter: f64,
}

impl Stretch {
    const NONE: Stretch = Stretch { gap: 0.0, letter: 0.0 };
}

/// Pen position of every glyph of a line (letter spacing between clusters of letters that do not join,
/// never after the last glyph) and the width of the line.
fn pens_of(raw: &RawLine, content: &str, layout: &crate::model::TextLayout, stretch: &Stretch) -> (Vec<f64>, f64) {
    let mut pens = Vec::with_capacity(raw.glyphs.len());
    let mut pen = 0.0;
    for (i, g) in raw.glyphs.iter().enumerate() {
        pens.push(pen);
        pen += g.advance;
        if let Some(next) = raw.glyphs.get(i + 1) {
            if next.cluster != g.cluster && !joined_letters(content, g.cluster, next.cluster) {
                pen += layout.letter_spacing_mm + stretch.letter;
            }
            if is_gap(content, g.cluster) {
                pen += stretch.gap;
            }
        }
    }
    (pens, pen)
}

/// Whether the character at byte `at` is a space between words.
fn is_gap(content: &str, at: usize) -> bool {
    content.get(at..).and_then(|s| s.chars().next()).is_some_and(|c| c.is_whitespace() && c != '\r')
}

/// (word gaps, letter boundaries that take letter spacing) of a line, counting only those with a glyph after.
fn stretch_points(raw: &RawLine, content: &str) -> (usize, usize) {
    let (mut gaps, mut boundaries) = (0, 0);
    for pair in raw.glyphs.windows(2) {
        if is_gap(content, pair[0].cluster) {
            gaps += 1;
        }
        if pair[1].cluster != pair[0].cluster && !joined_letters(content, pair[0].cluster, pair[1].cluster) {
            boundaries += 1;
        }
    }
    (gaps, boundaries)
}

/// The line shaped again with kashida added so that it gets as close to `target` as it can without
/// passing it, or `None` when it has no place for one (no Arabic join that allows it, or the font has no
/// tatweel).
fn kashida_stretch(
    shape_one: &dyn Fn(&str, usize, usize) -> Result<RawLine, TextError>,
    line: &str,
    base: usize,
    content: &str,
    layout: &crate::model::TextLayout,
    chain: &[Arc<FontData>],
    (natural, target): (f64, f64),
) -> Result<Option<RawLine>, TextError> {
    if !chain.iter().any(|f| f.has_glyph(TATWEEL)) {
        return Ok(None);
    }
    let (text, _) = case_mapped(line, layout.uppercase);
    let sites = kashida_sites(&text).len();
    if sites == 0 {
        return Ok(None);
    }
    let width = |raw: &RawLine| pens_of(raw, content, layout, &Stretch::NONE).1;
    // What one tatweel adds, measured on the line itself (its neighbours decide the glyph).
    let probe = shape_one(line, base, sites)?;
    let one = (width(&probe) - natural) / sites as f64;
    if one <= STRETCH_EPS {
        return Ok(None);
    }
    let cap = sites * MAX_KASHIDA_PER_WORD;
    let mut count = (((target - natural) / one).floor().max(0.0) as usize).min(cap);
    for _ in 0..4 {
        if count == 0 {
            return Ok(None);
        }
        let raw = shape_one(line, base, count)?;
        let w = width(&raw);
        if w <= target + STRETCH_EPS {
            return Ok(Some(raw));
        }
        count = count.saturating_sub((((w - target) / one).ceil() as usize).max(1));
    }
    Ok(None)
}

const TATWEEL: char = '\u{640}';

/// Per word of `text` the byte offset where a kashida may go: between a letter that joins forward and a
/// letter that joins backward (marks in between stay with the first), never inside the lam-alef
/// ligature. Of a word's places the one nearest the middle is taken. Letters outside the basic Arabic
/// block and the Persian additions are not known to join and get none.
fn kashida_sites(text: &str) -> Vec<usize> {
    let mut sites = Vec::new();
    let mut word_start = 0;
    for (i, ch) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        if !ch.is_whitespace() {
            continue;
        }
        let chars: Vec<(usize, char)> = text[word_start..i].char_indices().map(|(j, c)| (word_start + j, c)).collect();
        let mut places = Vec::new();
        for (k, &(_, x)) in chars.iter().enumerate() {
            if !joins_forward(x) {
                continue;
            }
            let mut n = k + 1;
            while chars.get(n).is_some_and(|&(_, m)| is_arabic_mark(m)) {
                n += 1;
            }
            if let Some(&(at, y)) = chars.get(n)
                && joins_backward(y)
                && !(x == '\u{644}' && matches!(y, '\u{627}' | '\u{623}' | '\u{625}' | '\u{622}'))
            {
                places.push((k, at));
            }
        }
        let middle = chars.len() / 2;
        if let Some(&(_, at)) = places.iter().min_by_key(|(k, _)| k.abs_diff(middle)) {
            sites.push(at);
        }
        word_start = i + ch.len_utf8();
    }
    sites
}

fn is_arabic_mark(c: char) -> bool {
    matches!(
        u32::from(c),
        0x064B..=0x065F | 0x0670 | 0x06D6..=0x06DC | 0x06DF..=0x06E4 | 0x06E7..=0x06E8 | 0x06EA..=0x06ED
    )
}

/// Letters that connect to the letter after them (Joining_Type dual-joining), for the Arabic block and
/// the common Persian and Urdu additions.
fn joins_forward(c: char) -> bool {
    matches!(u32::from(c),
        0x0626 | 0x0628 | 0x062A..=0x062E | 0x0633..=0x063A | 0x0641..=0x0647 | 0x0649 | 0x064A | 0x066E | 0x066F
            | 0x067E | 0x0686 | 0x06A9 | 0x06AF | 0x06BE | 0x06C1 | 0x06CC | 0x0640)
}

/// Letters that connect to the letter before them (dual- and right-joining).
fn joins_backward(c: char) -> bool {
    joins_forward(c)
        || matches!(
            u32::from(c),
            0x0622..=0x0625 | 0x0627 | 0x0629 | 0x062F..=0x0632 | 0x0648 | 0x0698 | 0x06D2 | 0x06D3
        )
}

/// `line` with `kashida` tatweels spread over its words' sites, and the source offset of every byte.
fn with_kashida(line: &str, map: Vec<usize>, kashida: usize) -> (String, Vec<usize>) {
    let sites = kashida_sites(line);
    if kashida == 0 || sites.is_empty() {
        return (line.to_owned(), map);
    }
    let (each, extra) = (kashida / sites.len(), kashida % sites.len());
    let mut text = String::with_capacity(line.len() + kashida * TATWEEL.len_utf8());
    let mut out_map = Vec::with_capacity(map.len() + kashida * TATWEEL.len_utf8());
    let mut done = 0;
    for (n, &site) in sites.iter().enumerate() {
        text.push_str(&line[done..site]);
        out_map.extend_from_slice(&map[done..site]);
        // The tatweels belong to the letter before the site.
        let owner = line[..site].char_indices().next_back().map_or(0, |(j, _)| map.get(j).copied().unwrap_or(0));
        for _ in 0..each + usize::from(n < extra) {
            text.push(TATWEEL);
            out_map.extend(std::iter::repeat_n(owner, TATWEEL.len_utf8()));
        }
        done = site;
    }
    text.push_str(&line[done..]);
    out_map.extend_from_slice(&map[done..]);
    (text, out_map)
}

/// Whether the clusters starting at byte `a` and `b` of `content` are letters of a cursive script
/// (Arabic, Syriac, N'Ko, Mandaic, Mongolian): their letters join, so spacing between them would
/// tear the word apart. Spaces, digits and punctuation are not letters and take the spacing.
fn joined_letters(content: &str, a: usize, b: usize) -> bool {
    let cursive = |at: usize| content.get(at..).and_then(|s| s.chars().next()).is_some_and(is_cursive_letter);
    cursive(a) && cursive(b)
}

fn is_cursive_letter(ch: char) -> bool {
    let c = u32::from(ch);
    let in_script = matches!(
        c,
        0x0600..=0x06FF | 0x0700..=0x074F | 0x0750..=0x077F | 0x07C0..=0x07FF | 0x0840..=0x085F | 0x0860..=0x086F
            | 0x08A0..=0x08FF | 0x1800..=0x18AF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF
    );
    in_script && ch.is_alphabetic() && !ch.is_numeric()
}

/// Characters that are never drawn and that fonts rarely map: joiners, directional marks, word
/// joiner, zero width space, byte order mark, variation selectors, soft hyphen. They go to the
/// shaper with the text around them (a joiner changes how its neighbours are drawn) and are never
/// reported as missing, whichever font draws the run.
fn is_default_ignorable(ch: char) -> bool {
    matches!(u32::from(ch),
        0x00AD | 0x034F | 0x061C | 0x115F..=0x1160 | 0x17B4..=0x17B5 | 0x180B..=0x180F
            | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F | 0x3164
            | 0xFE00..=0xFE0F | 0xFEFF | 0xFFA0 | 0xE0000..=0xE0FFF)
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
    kashida: usize,
) -> Result<RawLine, TextError> {
    let mut raw = RawLine { glyphs: Vec::new(), missing: Vec::new() };
    // The text to draw (upper-cased on request) and, per byte, its source offset in `line`.
    let (line, map) = case_mapped(line, layout.uppercase);
    let (line, map) = with_kashida(&line, map, kashida);
    let line = line.as_str();
    // Directional runs in visual (left-to-right) order, each (byte range, right-to-left).
    let bidi = BidiInfo::new(line, None);
    let mut directional: Vec<(Range<usize>, bool)> = Vec::new();
    if bidi.has_rtl() {
        for para in &bidi.paragraphs {
            let (levels, runs) = bidi.visual_runs(para, para.range.clone());
            for run in runs {
                let rtl = levels.get(run.start).is_some_and(|l| l.is_rtl());
                directional.push((run, rtl));
            }
        }
    } else {
        directional.push((0..line.len(), false));
    }
    // Per directional run: runs of (font index, byte range), in logical order.
    let mut runs: Vec<(usize, Range<usize>, bool)> = Vec::new();
    for (span, rtl) in directional {
        let mut logical: Vec<(usize, Range<usize>, bool)> = Vec::new();
        let slice = line.get(span.clone()).ok_or(TextError::Shaping)?;
        for (j, ch) in slice.char_indices() {
            if ch == '\r' {
                continue;
            }
            let i = span.start + j;
            let end = i + ch.len_utf8();
            // An invisible format character stays in the run before it (or, first in the line, in
            // the run of the main font) whatever the fonts map.
            let font = if is_default_ignorable(ch) {
                Some(logical.last().map_or(0, |l| l.0))
            } else {
                chain.iter().position(|f| f.has_glyph(ch))
            };
            match font {
                Some(font) => match logical.last_mut() {
                    Some((f, r, _)) if *f == font && r.end == i => r.end = end,
                    _ => logical.push((font, i..end, rtl)),
                },
                None => raw.missing.push((ch, base + map.get(i).copied().unwrap_or(0))),
            }
        }
        // Inside a right-to-left run the logically first piece is drawn right-most.
        if rtl {
            logical.reverse();
        }
        runs.extend(logical);
    }
    raw.missing.sort_by_key(|m| m.1);
    let mut features = Vec::new();
    if !layout.kerning {
        features.push(Feature::new(Tag::new(b"kern"), 0, ..));
    }
    if !layout.ligatures {
        features.push(Feature::new(Tag::new(b"liga"), 0, ..));
        features.push(Feature::new(Tag::new(b"clig"), 0, ..));
    }
    for (font_index, range, rtl) in runs {
        let font = chain.get(font_index).ok_or(TextError::Shaping)?;
        let text = line.get(range.clone()).ok_or(TextError::Shaping)?;
        let mut buffer = Buffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        buffer.set_direction(if rtl { Direction::RightToLeft } else { Direction::LeftToRight });
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
