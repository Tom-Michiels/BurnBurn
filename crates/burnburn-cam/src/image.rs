//! Image operations (IM18): the original source pixels are resampled to the physical output
//! grid of the operation *at compile time* and turned into scan runs. Neither the resampled
//! gray image nor the dither result is stored anywhere: a resized frame gives a new raster from
//! the source, never a resampled earlier dither.

use std::collections::BTreeMap;

use burnburn_document::process::{DitherMode, ImageRender, ImageSettings, ScanDirection, ToneSettings};
use burnburn_geometry::{Bounds, Point};
use burnburn_image::{
    AlphaPolicy, DitherAlgorithm, RasterError, SourceImage, ToneCurve, dither, resample_area, tone_map,
};
use burnburn_types::{CancellationToken, ContentHash, GeometryId, OperationId};

use crate::CompileError;
use crate::compile::Step;
use crate::line::Contour;

/// Where the compiler finds original source pixels by content hash (project assets).
pub trait RasterSource {
    fn source_image(&self, hash: ContentHash) -> Option<&SourceImage>;
}

/// No images available: every image operation fails with `MissingImageSource`.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRasters;

impl RasterSource for NoRasters {
    fn source_image(&self, _: ContentHash) -> Option<&SourceImage> {
        None
    }
}

impl RasterSource for Vec<SourceImage> {
    fn source_image(&self, hash: ContentHash) -> Option<&SourceImage> {
        self.iter().find(|s| s.content_hash() == hash)
    }
}

impl RasterSource for BTreeMap<ContentHash, SourceImage> {
    fn source_image(&self, hash: ContentHash) -> Option<&SourceImage> {
        self.get(&hash)
    }
}

pub(crate) fn tone(t: ToneSettings) -> Result<ToneCurve, RasterError> {
    ToneCurve::new(t.gamma(), t.brightness(), t.contrast())
}

fn algorithm(d: DitherMode) -> Option<DitherAlgorithm> {
    Some(match d {
        DitherMode::Threshold { level } => DitherAlgorithm::Threshold { level },
        DitherMode::FloydSteinberg => DitherAlgorithm::FloydSteinberg,
        DitherMode::Jarvis => DitherAlgorithm::Jarvis,
        DitherMode::Stucki => DitherAlgorithm::Stucki,
        DitherMode::Atkinson => DitherAlgorithm::Atkinson,
        DitherMode::Ordered { size } => DitherAlgorithm::Ordered { size },
        _ => return None,
    })
}

/// Number of cells of `pitch` in `extent` (with the same 1e-12 slack as fills), at least one.
fn cells(extent: f64, pitch: f64) -> u64 {
    let ratio = extent / pitch;
    let n = (ratio + ratio.abs() * 1e-12).floor();
    if n < 1.0 { 1 } else { n as u64 }
}

/// The raster grid of one target: `cols × rows` cells, centred in the frame bounds.
struct Grid {
    cols: u32,
    rows: u32,
    x0: f64,
    /// Centre of image row 0 (the top row).
    y_top: f64,
    pitch: f64,
    interval: f64,
}

fn grid(b: &Bounds, s: &ImageSettings, operation: OperationId) -> Result<Grid, CompileError> {
    let (pitch, interval) = (s.dot_pitch().mm(), s.line_interval().mm());
    let (cols, rows) = (cells(b.width(), pitch), cells(b.height(), interval));
    let too_big = |n: u64| u32::try_from(n).map_err(|_| CompileError::TooManyScanRows { operation, rows: n });
    let (cols, rows) = (too_big(cols)?, too_big(rows)?);
    let x0 = b.min().x + (b.width() - f64::from(cols) * pitch) / 2.0;
    let y_top = b.max().y - (b.height() - f64::from(rows) * interval) / 2.0 - 0.5 * interval;
    Ok(Grid { cols, rows, x0, y_top, pitch, interval })
}

/// One laser-on run of a row: columns `[c0, c1]` at `level` (`None` = binary on).
struct Run {
    c0: u32,
    c1: u32,
    level: Option<u8>,
}

/// Plans one pass of an image operation over the bounds of each target geometry; updates
/// `head`. Rows start at the frame corner nearest the head and zig-zag when bidirectional.
pub(crate) fn plan(
    contours: &[Contour],
    settings: &ImageSettings,
    rasters: &dyn RasterSource,
    head: &mut Point,
    operation: OperationId,
    cancel: &CancellationToken,
) -> Result<Vec<Step>, CompileError> {
    let raster_err = |error| match error {
        RasterError::Cancelled => CompileError::Cancelled,
        error => CompileError::Raster { operation, error },
    };
    let source = rasters
        .source_image(settings.source())
        .ok_or(CompileError::MissingImageSource { operation, source: settings.source() })?;
    let curve = tone(settings.tone()).map_err(raster_err)?;
    let mut frames: Vec<(GeometryId, Bounds)> = Vec::new();
    for c in contours {
        let Some(b) = crate::line::bounds_of(&c.points) else { continue };
        match frames.iter_mut().find(|(g, _)| *g == c.geometry) {
            Some((_, fb)) => *fb = fb.union(b),
            None => frames.push((c.geometry, b)),
        }
    }
    let mut steps = Vec::new();
    for (geometry, b) in frames {
        let g = grid(&b, settings, operation)?;
        // Always from the original pixels, at this frame's physical grid.
        let gray = resample_area(source, AlphaPolicy::default(), g.cols, g.rows, cancel).map_err(raster_err)?;
        let rows: Vec<Vec<Run>> = match settings.render() {
            ImageRender::Dither(d) => {
                let algo = algorithm(d).ok_or(CompileError::UnsupportedMode { operation })?;
                let bits = dither(&gray, &curve, algo, settings.serpentine(), cancel).map_err(raster_err)?;
                (0..g.rows).map(|r| runs(g.cols, |c| bits.get(c, r).then_some(None))).collect()
            }
            ImageRender::Grayscale => {
                let levels = tone_map(&gray, &curve);
                let at = |c: u32, r: u32| levels[r as usize * g.cols as usize + c as usize];
                (0..g.rows).map(|r| runs(g.cols, |c| (at(c, r) < 255).then_some(Some(at(c, r))))).collect()
            }
        };
        emit(&g, rows, settings.direction() == ScanDirection::Bidirectional, geometry, head, &mut steps, cancel)?;
    }
    Ok(steps)
}

/// Maximal runs of equal "on" values in one row.
fn runs(cols: u32, on: impl Fn(u32) -> Option<Option<u8>>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for c in 0..cols {
        let Some(level) = on(c) else { continue };
        match out.last_mut() {
            Some(r) if r.c1 + 1 == c && r.level == level => r.c1 = c,
            _ => out.push(Run { c0: c, c1: c, level }),
        }
    }
    out
}

fn emit(
    g: &Grid,
    rows: Vec<Vec<Run>>,
    bidirectional: bool,
    geometry: GeometryId,
    head: &mut Point,
    steps: &mut Vec<Step>,
    cancel: &CancellationToken,
) -> Result<(), CompileError> {
    let x_of = |c: u32| g.x0 + f64::from(c) * g.pitch;
    let y_of = |r: u32| g.y_top - f64::from(r) * g.interval;
    let (left, right) = (x_of(0), x_of(g.cols));
    let (top, bottom) = (y_of(0), y_of(g.rows.saturating_sub(1)));
    // Nearest corner (ties: bottom-left, bottom-right, top-left, top-right), as for fills.
    let corners = [(left, bottom), (right, bottom), (left, top), (right, top)];
    let mut corner = corners[0];
    for c in corners {
        if head.distance(Point::new(c.0, c.1)) < head.distance(Point::new(corner.0, corner.1)) {
            corner = c;
        }
    }
    let bottom_first = corner.1 == bottom;
    let left_first = corner.0 == left;
    let mut emitted = 0u64;
    for k in 0..g.rows {
        cancel.check()?;
        let r = if bottom_first { g.rows - 1 - k } else { k };
        let mut row = rows.get(r as usize).map(|v| v.iter().collect::<Vec<_>>()).unwrap_or_default();
        if row.is_empty() {
            continue;
        }
        let forward = left_first != (bidirectional && emitted % 2 == 1);
        if !forward {
            row.reverse();
        }
        let y = y_of(r);
        for run in row {
            let (a, b) = (x_of(run.c0), x_of(run.c1 + 1));
            let (from, to) = if forward { (a, b) } else { (b, a) };
            let (from, to) = (Point::new(from, y), Point::new(to, y));
            steps.push(match run.level {
                None => Step::Scan { from, to, source: geometry },
                Some(level) => Step::PowerScan { from, to, level, source: geometry },
            });
            *head = to;
        }
        emitted += 1;
    }
    Ok(())
}
