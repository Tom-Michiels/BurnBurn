//! Canvas text (LF10): measurement labels, ruler numbers and snap names as glyph outlines of a
//! bundled font (`burnburn-text`), filled like any shape, with a halo in the background colour so
//! that the text stays legible on top of lines.
//!
//! The font is Liberation Sans (SIL OFL 1.1, `assets/fonts/`), compiled in: canvas text never
//! depends on a system font. A character the font lacks is *not* drawn (never as a replacement
//! block). Shaping is cached per string; the outlines are scaled to the requested size on use.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use burnburn_geometry::{DisplayTolerance, Path};
use burnburn_text::{FontData, FontSize, FontStyle, MemoryFontSource, TextObject, shape};
use burnburn_types::Length;

use crate::input::Rgba;
use crate::packet::Primitive;

static FONT_BYTES: &[u8] = include_bytes!("../../../../assets/fonts/LiberationSans-Regular.ttf");

/// Family name the bundled face is registered under.
const FAMILY: &str = "Liberation Sans";
/// The size text is shaped at, in "mm"; outlines are scaled from here to the size asked for.
const SHAPE_EM_MM: f64 = 100.0;
/// Height of a capital letter relative to the em square (Liberation Sans: 1409 / 2048).
const CAP_HEIGHT_EM: f64 = 0.688;
/// Most strings kept in the shaping cache before it is emptied.
const CACHE_LIMIT: usize = 512;

/// A shaped string: its outline at [`SHAPE_EM_MM`] (y up, baseline at 0, starting at x = 0).
struct Shaped {
    outline: Path,
    width_mm: f64,
}

struct Face {
    source: MemoryFontSource,
    font: burnburn_text::FontRef,
}

fn face() -> Option<&'static Face> {
    static FACE: OnceLock<Option<Face>> = OnceLock::new();
    FACE.get_or_init(|| {
        let data = Arc::new(FontData::from_bytes(FONT_BYTES.to_vec()).ok()?);
        let mut source = MemoryFontSource::new();
        let font = source.insert(FAMILY, FontStyle::Regular, data);
        Some(Face { source, font })
    })
    .as_ref()
}

type Cache = Mutex<HashMap<String, Option<Arc<(Shaped, Vec<char>)>>>>;

fn shaped(text: &str) -> Option<Arc<(Shaped, Vec<char>)>> {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = cache.lock()
        && let Some(hit) = map.get(text)
    {
        return hit.clone();
    }
    let computed = shape_uncached(text).map(Arc::new);
    if let Ok(mut map) = cache.lock() {
        if map.len() >= CACHE_LIMIT {
            map.clear();
        }
        map.insert(text.to_owned(), computed.clone());
    }
    computed
}

fn shape_uncached(text: &str) -> Option<(Shaped, Vec<char>)> {
    let face = face()?;
    let size = FontSize::from_mm(Length::from_mm(SHAPE_EM_MM).ok()?).ok()?;
    let single_line: String = text.chars().filter(|c| *c != '\n').collect();
    let object = TextObject::new(single_line.clone(), face.font.clone(), size);
    let out = shape(&object, &face.source).ok()?;
    let missing = out.missing.iter().map(|m| m.ch).collect();
    let width_mm = out.lines.first().map_or(0.0, |l| l.width_mm);
    Some((Shaped { outline: out.outlines, width_mm }, missing))
}

/// The characters of `text` that the bundled canvas font cannot draw (they are left out of the
/// picture, never replaced by a block).
pub fn missing_chars(text: &str) -> Vec<char> {
    shaped(text).map_or_else(|| text.chars().collect(), |s| s.1.clone())
}

/// Where a label sits relative to its anchor point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Anchor {
    /// Centred on the point, horizontally and on the capital height.
    Center,
    /// Starts at the point, vertically centred on the capital height.
    Start,
}

/// How a label is drawn.
#[derive(Debug, Clone, Copy)]
pub(super) struct TextStyle {
    /// Size of the em square in device pixels.
    pub em_px: f64,
    pub ink: Rgba,
    /// Colour of the halo, the surface the text lies on.
    pub halo: Rgba,
    /// Half the width of the halo ring in device pixels (how far it reaches beyond the glyphs).
    pub halo_px: f32,
    /// Quarter turns counter-clockwise on screen (0 horizontal, 1 reads bottom to top).
    pub quarter_turns: u8,
}

/// The label as primitives: the halo ring first, then the filled glyphs.
pub(super) fn label(out: &mut Vec<Primitive>, text: &str, at_px: [f64; 2], anchor: Anchor, style: TextStyle) {
    if text.trim().is_empty() || !(style.em_px.is_finite() && style.em_px > 0.0) {
        return;
    }
    let Some(shaped) = shaped(text) else { return };
    let k = style.em_px / SHAPE_EM_MM;
    let Ok(tolerance) = Length::from_mm(0.2 / k).map_err(|_| ()).and_then(|l| DisplayTolerance::new(l).map_err(|_| ()))
    else {
        return;
    };
    let (x_origin, y_origin) = match anchor {
        Anchor::Center => (-shaped.0.width_mm / 2.0, -CAP_HEIGHT_EM * SHAPE_EM_MM / 2.0),
        Anchor::Start => (0.0, -CAP_HEIGHT_EM * SHAPE_EM_MM / 2.0),
    };
    let (sin, cos) = match style.quarter_turns % 4 {
        0 => (0.0, 1.0),
        1 => (1.0, 0.0),
        2 => (0.0, -1.0),
        _ => (-1.0, 0.0),
    };
    let contours: Vec<Vec<[f32; 2]>> = shaped
        .0
        .outline
        .flatten(&tolerance)
        .into_iter()
        .filter(|pl| pl.closed)
        .map(|pl| {
            pl.points
                .into_iter()
                .map(|p| {
                    // Text space (mm, y up) to a screen offset (px, y down), then turned.
                    let (x, y) = ((p.x + x_origin) * k, -(p.y + y_origin) * k);
                    let (rx, ry) = (x * cos + y * sin, -x * sin + y * cos);
                    [(at_px[0] + rx) as f32, (at_px[1] + ry) as f32]
                })
                .collect()
        })
        .collect();
    if contours.is_empty() {
        return;
    }
    if style.halo_px > 0.0 {
        for contour in &contours {
            out.push(Primitive::Stroke {
                points_px: contour.clone(),
                closed: true,
                width_px: style.halo_px * 2.0,
                color: style.halo,
            });
        }
    }
    out.push(Primitive::Fill { contours_px: contours, color: style.ink });
}
