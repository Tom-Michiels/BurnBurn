//! PDF and Illustrator (`.ai`) vector import (W2-L6, P03).
//!
//! Own, bounded reader (no `lopdf`: see `docs/dev-notes/IM28.md`): cross-reference tables and
//! streams, object streams, Flate (with PNG/TIFF predictors), ASCIIHex and ASCII85, a rebuild
//! scan for damaged files. Imported: path construction and painting operators (`m l c v y h re`,
//! stroke/fill/`n`) with the graphics-state matrix, form XObjects, and optional-content layers
//! that are switched off. Units: 1 pt = 25.4/72 mm times the page's `UserUnit`; the page frame is
//! the `CropBox` (inside the `MediaBox`) with `Rotate`; page lower-left is the world origin
//! (y up). Reported, never imported silently: text, images, shadings, clipping paths, other
//! XObjects, unknown operators. Encrypted files are refused. Modern `.ai` files *are* PDF; the
//! legacy PostScript form is refused by name.

mod content;
mod file;
mod inflate;
mod object;

use std::collections::HashSet;

use burnburn_document::GeometryCommand;
use burnburn_types::ContentHash;

use self::content::{IDENTITY, Interp, Matrix, then};
use self::file::PdfFile;
use self::object::{Dict, Obj};
use super::{AnalyzeError, ImportDraft, ParseError, ParseLimits, SkipReason, Skipped, UnitDecision, check_size};

const MM_PER_PT: f64 = 25.4 / 72.0;
/// Gap between pages when several are imported side by side (mm).
const PAGE_GAP_MM: f64 = 10.0;
const MAX_PAGES: usize = 20_000;

/// Whether the bytes came in as a PDF or as an Illustrator file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfKind {
    Pdf,
    Illustrator,
}

/// One page's frame after `CropBox`/`MediaBox`, `UserUnit` and `Rotate`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfPage {
    pub width_mm: f64,
    pub height_mm: f64,
    pub rotation_deg: u16,
}

/// Which pages to import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfPages {
    /// Zero-based page index.
    One(usize),
    /// Every page, side by side left to right with a 10 mm gap.
    All,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfDocument {
    kind: PdfKind,
    bytes: Vec<u8>,
    limits: ParseLimits,
    pages: Vec<PdfPage>,
}

/// A page with its inherited attributes resolved.
struct PageDesc {
    resources: Dict,
    contents: Option<Obj>,
    /// Page frame in user space `(x0, y0, x1, y1)`.
    frame: (f64, f64, f64, f64),
    user_unit: f64,
    rotation: u16,
    assumed_box: bool,
}

#[derive(Default, Clone)]
struct Inherited {
    resources: Option<Obj>,
    media_box: Option<Obj>,
    crop_box: Option<Obj>,
    rotate: Option<Obj>,
}

impl PdfDocument {
    pub fn kind(&self) -> PdfKind {
        self.kind
    }

    pub fn pages(&self) -> &[PdfPage] {
        &self.pages
    }

    /// The vector geometry of the chosen pages in world mm (y up). Several pages are placed
    /// left to right.
    pub fn to_draft(&self, which: PdfPages) -> Result<ImportDraft, ParseError> {
        let file = PdfFile::open(&self.bytes, &self.limits)?;
        let descs = collect_pages(&file, &self.limits)?;
        let indices: Vec<usize> = match which {
            PdfPages::One(i) if i < descs.len() => vec![i],
            PdfPages::One(i) => return Err(ParseError::Malformed { at: i, reason: "page does not exist" }),
            PdfPages::All => (0..descs.len()).collect(),
        };
        let hidden = hidden_groups(&file);
        let mut items = Vec::new();
        let mut skipped: Vec<Skipped> = Vec::new();
        let mut x_offset = 0.0;
        let mut unit = None;
        for &i in &indices {
            let desc = &descs[i];
            let (frame, width, _) = page_frame(desc);
            unit.get_or_insert(MM_PER_PT * desc.user_unit);
            let base = then(&frame, &[1.0, 0.0, 0.0, 1.0, x_offset, 0.0]);
            let mut interp = Interp::new(&file, &self.limits, i + 1, &hidden);
            if desc.assumed_box {
                interp.report("page box", SkipReason::Invalid("no usable MediaBox: US Letter assumed"));
            }
            match page_content(&file, desc) {
                Ok(content) => interp.run_page(&content, &desc.resources, base)?,
                Err(e) => interp.stream_problem(e),
            }
            items.append(&mut interp.items);
            for s in interp.skipped {
                if !skipped.iter().any(|k| k.kind == s.kind && k.reason == s.reason) {
                    skipped.push(s);
                }
            }
            x_offset += width + PAGE_GAP_MM;
        }
        Ok(ImportDraft::new(
            items,
            UnitDecision::Pdf { mm_per_unit: unit.unwrap_or(MM_PER_PT), pages: indices.len() },
            skipped,
        ))
    }

    pub fn import_command(&self, asset: ContentHash, pages: PdfPages) -> Result<GeometryCommand, AnalyzeError> {
        let draft = self.to_draft(pages).map_err(AnalyzeError::Parse)?;
        draft.into_command(asset).map_err(AnalyzeError::Import)
    }
}

/// Fuzz entry point with default limits.
pub fn parse_pdf(bytes: &[u8]) -> Result<PdfDocument, ParseError> {
    parse_pdf_with(bytes, &ParseLimits::default())
}

/// Reads the page structure of a PDF within `limits`. Never panics.
pub fn parse_pdf_with(bytes: &[u8], limits: &ParseLimits) -> Result<PdfDocument, ParseError> {
    check_size(bytes, limits)?;
    let file = PdfFile::open(bytes, limits)?;
    let descs = collect_pages(&file, limits)?;
    let pages = descs
        .iter()
        .map(|d| {
            let (_, width_mm, height_mm) = page_frame(d);
            PdfPage { width_mm, height_mm, rotation_deg: d.rotation }
        })
        .collect();
    Ok(PdfDocument { kind: PdfKind::Pdf, bytes: bytes.to_vec(), limits: *limits, pages })
}

/// Reads an Illustrator file (PDF-compatible) within `limits`.
pub fn parse_ai_with(bytes: &[u8], limits: &ParseLimits) -> Result<PdfDocument, ParseError> {
    parse_pdf_with(bytes, limits).map(|d| PdfDocument { kind: PdfKind::Illustrator, ..d })
}

fn numbers(file: &PdfFile<'_>, obj: Option<&Obj>) -> Option<Vec<f64>> {
    let resolved = file.resolve(obj?)?;
    let Obj::Array(a) = &*resolved else { return None };
    a.iter().map(|o| file.resolve(o).and_then(|o| o.as_f64())).collect()
}

fn rect(file: &PdfFile<'_>, obj: Option<&Obj>) -> Option<(f64, f64, f64, f64)> {
    let v = numbers(file, obj)?;
    let [a, b, c, d] = v[..] else { return None };
    let (x0, x1, y0, y1) = (a.min(c), a.max(c), b.min(d), b.max(d));
    (x1 - x0 > 0.0 && y1 - y0 > 0.0 && [x0, x1, y0, y1].iter().all(|v| v.abs() < 1e7)).then_some((x0, y0, x1, y1))
}

fn collect_pages(file: &PdfFile<'_>, limits: &ParseLimits) -> Result<Vec<PageDesc>, ParseError> {
    let catalog = file.catalog().ok_or(ParseError::Malformed { at: 0, reason: "no document catalog" })?;
    let root = catalog
        .dict()
        .and_then(|d| d.get("Pages"))
        .ok_or(ParseError::Malformed { at: 0, reason: "catalog has no page tree" })?
        .clone();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    walk(file, &root, &Inherited::default(), &mut seen, &mut out, 0, limits)?;
    if out.is_empty() {
        return Err(ParseError::Malformed { at: 0, reason: "document has no pages" });
    }
    Ok(out)
}

fn walk(
    file: &PdfFile<'_>,
    node: &Obj,
    inherited: &Inherited,
    seen: &mut HashSet<u32>,
    out: &mut Vec<PageDesc>,
    depth: usize,
    limits: &ParseLimits,
) -> Result<(), ParseError> {
    if depth > limits.max_depth {
        return Err(ParseError::TooDeep { limit: limits.max_depth });
    }
    if let Obj::Ref(n) = node
        && !seen.insert(*n)
    {
        return Ok(());
    }
    let Some(resolved) = file.resolve(node) else { return Ok(()) };
    let Some(dict) = resolved.dict() else { return Ok(()) };
    let mut inh = inherited.clone();
    for (key, slot) in [
        ("Resources", &mut inh.resources),
        ("MediaBox", &mut inh.media_box),
        ("CropBox", &mut inh.crop_box),
        ("Rotate", &mut inh.rotate),
    ] {
        if let Some(v) = dict.get(key) {
            *slot = Some(v.clone());
        }
    }
    if let Some(kids) = dict.get("Kids").and_then(|k| file.resolve(k))
        && let Obj::Array(kids) = &*kids
    {
        for kid in kids {
            walk(file, kid, &inh, seen, out, depth + 1, limits)?;
        }
        return Ok(());
    }
    if out.len() >= MAX_PAGES {
        return Err(ParseError::TooManyEntities { limit: MAX_PAGES });
    }
    let media = rect(file, inh.media_box.as_ref());
    let assumed_box = media.is_none();
    let media = media.unwrap_or((0.0, 0.0, 612.0, 792.0));
    let frame = match rect(file, inh.crop_box.as_ref()) {
        Some(c) => {
            let (x0, y0, x1, y1) = (c.0.max(media.0), c.1.max(media.1), c.2.min(media.2), c.3.min(media.3));
            if x1 > x0 && y1 > y0 { (x0, y0, x1, y1) } else { media }
        }
        None => media,
    };
    let user_unit = dict
        .get("UserUnit")
        .and_then(|u| file.resolve(u))
        .and_then(|u| u.as_f64())
        .filter(|u| *u > 0.0 && *u < 1e6)
        .unwrap_or(1.0);
    let rotation = inh
        .rotate
        .as_ref()
        .and_then(|r| file.resolve(r))
        .and_then(|r| r.as_i64())
        .map(|r| r.rem_euclid(360) as u16)
        .filter(|r| r % 90 == 0)
        .unwrap_or(0);
    let resources =
        inh.resources.as_ref().and_then(|r| file.resolve(r)).and_then(|r| r.dict().cloned()).unwrap_or_default();
    out.push(PageDesc { resources, contents: dict.get("Contents").cloned(), frame, user_unit, rotation, assumed_box });
    Ok(())
}

/// The user-space → world-mm matrix of a page and its width/height in mm after rotation.
fn page_frame(d: &PageDesc) -> (Matrix, f64, f64) {
    let (x0, y0, x1, y1) = d.frame;
    let s = MM_PER_PT * d.user_unit;
    let (w, h) = ((x1 - x0) * s, (y1 - y0) * s);
    let base: Matrix = [s, 0.0, 0.0, s, -x0 * s, -y0 * s];
    let rot: Matrix = match d.rotation {
        90 => [0.0, -1.0, 1.0, 0.0, 0.0, w],
        180 => [-1.0, 0.0, 0.0, -1.0, w, h],
        270 => [0.0, 1.0, -1.0, 0.0, h, 0.0],
        _ => IDENTITY,
    };
    let (rw, rh) = if d.rotation == 90 || d.rotation == 270 { (h, w) } else { (w, h) };
    (then(&base, &rot), rw, rh)
}

fn page_content(file: &PdfFile<'_>, desc: &PageDesc) -> Result<Vec<u8>, file::StreamError> {
    let Some(contents) = desc.contents.as_ref().and_then(|c| file.resolve(c)) else { return Ok(Vec::new()) };
    match &*contents {
        Obj::Stream(..) => file.decode_stream(&contents),
        Obj::Array(parts) => {
            let mut out = Vec::new();
            for part in parts {
                if let Some(obj) = file.resolve(part)
                    && matches!(&*obj, Obj::Stream(..))
                {
                    out.extend_from_slice(&file.decode_stream(&obj)?);
                    out.push(b'\n');
                }
            }
            Ok(out)
        }
        _ => Ok(Vec::new()),
    }
}

/// Optional-content groups switched off in the default configuration.
fn hidden_groups(file: &PdfFile<'_>) -> HashSet<u32> {
    let mut off = HashSet::new();
    let Some(catalog) = file.catalog() else { return off };
    let Some(props) = catalog.dict().and_then(|d| file.get(d, "OCProperties")) else { return off };
    let Some(props) = props.dict() else { return off };
    let refs = |obj: Option<&Obj>| -> Vec<u32> {
        let Some(o) = obj.and_then(|o| file.resolve(o)) else { return Vec::new() };
        match &*o {
            Obj::Array(a) => a.iter().filter_map(|o| if let Obj::Ref(n) = o { Some(*n) } else { None }).collect(),
            _ => Vec::new(),
        }
    };
    let config = file.get(props, "D");
    let Some(config) = config.as_deref().and_then(Obj::dict) else { return off };
    if config.get("BaseState").and_then(Obj::as_name) == Some("OFF") {
        let on: HashSet<u32> = refs(config.get("ON")).into_iter().collect();
        off.extend(refs(props.get("OCGs")).into_iter().filter(|n| !on.contains(n)));
    }
    off.extend(refs(config.get("OFF")));
    off
}
