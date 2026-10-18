//! Image operations (IM18, W1-L5a): the original source pixels are resampled to the physical
//! output grid of the operation *at compile time* and turned into scan runs. Neither the
//! resampled gray image nor the dither result is stored anywhere: a resized frame gives a new
//! raster from the source, never a resampled earlier dither.
//!
//! Per frame (the bounds of one target geometry) the pipeline is: crop window of the source →
//! gray (colour method, alpha over paper) → resample to the scan grid (area average; nearest for
//! pass-through) → unsharp → tone → negative → mode (`burnburn_image::engrave`) → mask → runs
//! (dot width correction) in the scan frame. A scan angle rotates the grid about the frame;
//! the image stays where it is and is sampled (nearest) at the rotated dot centres.

use std::collections::BTreeMap;

use burnburn_document::process::{DitherMode, GrayMethod, ImageRender, ImageSettings, ScanDirection, ToneSettings};
use burnburn_geometry::{Bounds, Path, Point, ProductionTolerance, math};
use burnburn_image::{
    AlphaPolicy, DitherAlgorithm, EngraveMode, EngraveParams, Engraved, GrayConversion, GrayImage, PixelRect,
    RasterError, Sharpen, SourceImage, ToneCurve, engrave, resample_area_with, resample_nearest,
};
use burnburn_types::{CancellationToken, ContentHash, GeometryId, OperationId, StableId};

use crate::CompileError;
use crate::compile::Step;
use crate::fill::ScanFrame;
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

fn gray_conversion(m: GrayMethod) -> GrayConversion {
    match m {
        GrayMethod::Rec709 => GrayConversion::Rec709,
        GrayMethod::Rec601 => GrayConversion::Rec601,
        GrayMethod::Average => GrayConversion::Average,
        GrayMethod::Lightness => GrayConversion::Lightness,
        GrayMethod::Red => GrayConversion::Red,
        GrayMethod::Green => GrayConversion::Green,
        GrayMethod::Blue => GrayConversion::Blue,
        // `GrayMethod` is exhaustive today; a future method must be mapped here on purpose.
        #[allow(unreachable_patterns)]
        _ => GrayConversion::Rec709,
    }
}

/// `None` for a mode this compiler does not know.
fn engrave_mode(s: &ImageSettings) -> Option<EngraveMode> {
    let cell_px = s.screen().cell_mm() / s.dot_pitch().mm();
    Some(match s.render() {
        ImageRender::Grayscale => EngraveMode::Grayscale,
        ImageRender::Dither(d) => match d {
            DitherMode::Threshold { level } => EngraveMode::Dither(DitherAlgorithm::Threshold { level }),
            DitherMode::FloydSteinberg => EngraveMode::Dither(DitherAlgorithm::FloydSteinberg),
            DitherMode::Jarvis => EngraveMode::Dither(DitherAlgorithm::Jarvis),
            DitherMode::Stucki => EngraveMode::Dither(DitherAlgorithm::Stucki),
            DitherMode::Atkinson => EngraveMode::Dither(DitherAlgorithm::Atkinson),
            DitherMode::Ordered { size } => EngraveMode::Dither(DitherAlgorithm::Ordered { size }),
            DitherMode::Newsprint => EngraveMode::Newsprint { cell_px },
            DitherMode::Halftone => EngraveMode::Halftone { cell_px, angle_deg: s.screen().angle_deg() },
            DitherMode::Sketch { level } => EngraveMode::Sketch { level },
            DitherMode::PassThrough => EngraveMode::PassThrough,
            _ => return None,
        },
    })
}

fn crop_rect(s: &ImageSettings) -> Option<PixelRect> {
    s.crop().map(|c| PixelRect { x: c.x, y: c.y, width: c.width, height: c.height })
}

fn scan_frame(angle_deg: f64) -> ScanFrame {
    if angle_deg == 0.0 {
        ScanFrame::X
    } else if angle_deg == 90.0 {
        ScanFrame::Y
    } else {
        let (sin, cos) = math::sin_cos(angle_deg.to_radians());
        ScanFrame::Rotated { cos, sin }
    }
}

/// Number of cells of `pitch` in `extent` (with the same 1e-12 slack as fills), at least one.
fn cells(extent: f64, pitch: f64) -> u64 {
    let ratio = extent / pitch;
    let n = (ratio + ratio.abs() * 1e-12).floor();
    if n < 1.0 { 1 } else { n as u64 }
}

/// The raster grid of one frame in the scan frame: `cols × rows` cells, centred in the bounds.
struct Grid {
    cols: u32,
    rows: u32,
    x0: f64,
    /// Centre of image row 0 (the top row).
    y_top: f64,
    pitch: f64,
    interval: f64,
}

impl Grid {
    fn x_of(&self, c: u32) -> f64 {
        self.x0 + f64::from(c) * self.pitch
    }
    fn y_of(&self, r: u32) -> f64 {
        self.y_top - f64::from(r) * self.interval
    }
}

/// Why a frame could not be rastered.
enum FrameError {
    Raster(RasterError),
    TooManyRows(u64),
}

impl From<RasterError> for FrameError {
    fn from(e: RasterError) -> Self {
        FrameError::Raster(e)
    }
}

fn grid(b: &Bounds, s: &ImageSettings) -> Result<Grid, FrameError> {
    let (pitch, interval) = (s.dot_pitch().mm(), s.line_interval().mm());
    let (cols, rows) = (cells(b.width(), pitch), cells(b.height(), interval));
    let too_big = |n: u64| u32::try_from(n).map_err(|_| FrameError::TooManyRows(n));
    let (cols, rows) = (too_big(cols)?, too_big(rows)?);
    let x0 = b.min().x + (b.width() - f64::from(cols) * pitch) / 2.0;
    let y_top = b.max().y - (b.height() - f64::from(rows) * interval) / 2.0 - 0.5 * interval;
    Ok(Grid { cols, rows, x0, y_top, pitch, interval })
}

/// The processed raster of one frame, in the scan frame.
struct FrameRaster {
    grid: Grid,
    frame: ScanFrame,
    engraved: Engraved,
}

/// Even-odd spans `(x_start, x_end)` of `edges` on the horizontal line `y` (half-open crossing
/// rule, so a vertex counts once).
fn spans_at(edges: &[(Point, Point)], y: f64) -> Vec<(f64, f64)> {
    let mut xs: Vec<f64> = edges
        .iter()
        .filter(|(a, b)| (a.y <= y && y < b.y) || (b.y <= y && y < a.y))
        .map(|(a, b)| a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y))
        .collect();
    xs.sort_by(f64::total_cmp);
    let (pairs, _) = xs.as_chunks::<2>();
    pairs.iter().filter(|[a, b]| a < b).map(|[a, b]| (*a, *b)).collect()
}

/// Cells of `grid` whose centre lies inside `edges` (even-odd), row-major.
fn inside_cells(grid: &Grid, edges: &[(Point, Point)]) -> Vec<bool> {
    let mut inside = vec![false; grid.cols as usize * grid.rows as usize];
    for r in 0..grid.rows {
        for (xa, xb) in spans_at(edges, grid.y_of(r)) {
            // Cell centres are at x0 + (c + ½) pitch; take those in [xa, xb).
            let first = ((xa - grid.x0) / grid.pitch - 0.5).ceil().max(0.0);
            let end = ((xb - grid.x0) / grid.pitch - 0.5).ceil().min(f64::from(grid.cols));
            let mut c = first;
            while c < end {
                inside[r as usize * grid.cols as usize + c as usize] = true;
                c += 1.0;
            }
        }
    }
    inside
}

/// Rasters the frame of `contours` (all of one geometry, bed view). `None` without any vertex.
fn frame_raster(
    contours: &[&Contour],
    s: &ImageSettings,
    source: &SourceImage,
    cancel: &CancellationToken,
) -> Result<Option<FrameRaster>, FrameError> {
    let mode = engrave_mode(s).ok_or(RasterError::InvalidTone { field: "mode" })?;
    let pass_through = mode == EngraveMode::PassThrough;
    let Some(bed) = contours.iter().filter_map(|c| crate::line::bounds_of(&c.points)).reduce(Bounds::union) else {
        return Ok(None);
    };
    let angle = if pass_through { 0.0 } else { s.scan_angle_deg() };
    let frame = scan_frame(angle);
    let to_local = |p: Point| frame.to_local(p);
    let corners = [bed.min(), Point::new(bed.max().x, bed.min().y), bed.max(), Point::new(bed.min().x, bed.max().y)];
    let local = corners.map(to_local);
    let Some(lb) = crate::line::bounds_of(&local) else { return Ok(None) };
    let g = grid(&lb, s)?;
    let (alpha, conversion, crop) = (AlphaPolicy::default(), gray_conversion(s.gray_method()), crop_rect(s));
    let resample = |w, h| {
        if pass_through {
            resample_nearest(source, crop, alpha, conversion, w, h, cancel)
        } else {
            resample_area_with(source, crop, alpha, conversion, w, h, cancel)
        }
    };

    // The gray raster on the scan grid, and which cells are inside the (rotated) image.
    let mut keep: Option<Vec<bool>> = None;
    let gray = if frame == ScanFrame::X {
        resample(g.cols, g.rows)?
    } else {
        let (bc, br) = (cells(bed.width(), g.pitch), cells(bed.height(), g.interval));
        let too_big = |n: u64| u32::try_from(n).map_err(|_| FrameError::TooManyRows(n));
        let pre = resample(too_big(bc)?, too_big(br)?)?;
        let mut values = Vec::with_capacity(g.cols as usize * g.rows as usize);
        let mut inside = Vec::with_capacity(values.capacity());
        for r in 0..g.rows {
            cancel.check().map_err(RasterError::from)?;
            for c in 0..g.cols {
                let p = frame.to_bed(Point::new(g.x_of(c) + 0.5 * g.pitch, g.y_of(r)));
                let (u, v) = ((p.x - bed.min().x) / bed.width(), (bed.max().y - p.y) / bed.height());
                let within = (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v);
                inside.push(within);
                let (pc, pr) = (
                    (u * bc as f64).floor().clamp(0.0, bc as f64 - 1.0),
                    (v * br as f64).floor().clamp(0.0, br as f64 - 1.0),
                );
                values.push(if within { pre.values[pr as usize * bc as usize + pc as usize] } else { 255.0 });
            }
        }
        keep = Some(inside);
        GrayImage { width: g.cols, height: g.rows, values }
    };

    let params = EngraveParams {
        tone: tone(s.tone())?,
        sharpen: s
            .sharpen()
            .map(|sh| Sharpen { radius_px: (sh.radius_mm() / g.pitch).round().max(1.0) as u32, amount: sh.amount() }),
        negative: s.negative(),
        mode,
        serpentine: s.serpentine(),
    };
    let mut engraved = engrave(&gray, &params, cancel)?;

    if s.mask() {
        let edges: Vec<(Point, Point)> = contours
            .iter()
            .filter(|c| c.closed && c.points.len() >= 3)
            .flat_map(|c| {
                (0..c.points.len()).map(|k| (to_local(c.points[k]), to_local(c.points[(k + 1) % c.points.len()])))
            })
            .collect();
        let inside = inside_cells(&g, &edges);
        keep = Some(match keep {
            Some(k) => k.iter().zip(&inside).map(|(a, b)| *a && *b).collect(),
            None => inside,
        });
    }
    if let Some(keep) = keep {
        engraved = match engraved {
            Engraved::Bits(b) => {
                let bits = b.bits().iter().zip(&keep).map(|(on, k)| *on && *k).collect();
                Engraved::Bits(burnburn_image::BitImage::new(b.width(), b.height(), bits).unwrap_or(b))
            }
            Engraved::Levels { width, height, levels } => Engraved::Levels {
                width,
                height,
                levels: levels.iter().zip(&keep).map(|(l, k)| if *k { *l } else { 255 }).collect(),
            },
        };
    }
    Ok(Some(FrameRaster { grid: g, frame, engraved }))
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
    if engrave_mode(settings).is_none()
        || (settings.scan_angle_deg() != 0.0 && settings.render() == ImageRender::Dither(DitherMode::PassThrough))
    {
        return Err(CompileError::UnsupportedMode { operation });
    }
    let mut frames: Vec<(GeometryId, Vec<&Contour>)> = Vec::new();
    for c in contours {
        match frames.iter_mut().find(|(g, _)| *g == c.geometry) {
            Some((_, v)) => v.push(c),
            None => frames.push((c.geometry, vec![c])),
        }
    }
    let mut steps = Vec::new();
    for (geometry, group) in frames {
        // Always from the original pixels, at this frame's physical grid.
        let raster = match frame_raster(&group, settings, source, cancel) {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(FrameError::Raster(e)) => return Err(raster_err(e)),
            Err(FrameError::TooManyRows(rows)) => return Err(CompileError::TooManyScanRows { operation, rows }),
        };
        let rows = rows_of(&raster);
        let dwc = settings.dot_width_correction_mm();
        let bidirectional = settings.direction() == ScanDirection::Bidirectional;
        emit(&raster, rows, dwc, bidirectional, geometry, head, &mut steps, cancel)?;
    }
    Ok(steps)
}

/// The processed pixels of an image operation on `outline`, exactly as the compiler scans them
/// (same crop, resampling, adjustments, mode and mask; in the scan frame when the operation has
/// a scan angle). Burning cells are 0 in [`Engraved::to_gray8`]; "save processed bitmap" writes
/// these pixels, for example with [`burnburn_image::encode_png_gray8`].
pub fn processed_bitmap(
    settings: &ImageSettings,
    source: &SourceImage,
    outline: &Path,
    tolerance: &ProductionTolerance,
    cancel: &CancellationToken,
) -> Result<Engraved, RasterError> {
    let contours: Vec<Contour> = outline
        .flatten(tolerance)
        .into_iter()
        .map(|pl| Contour::new(GeometryId::from_raw(0), pl.points, pl.closed))
        .collect();
    let group: Vec<&Contour> = contours.iter().collect();
    match frame_raster(&group, settings, source, cancel) {
        Ok(Some(r)) => Ok(r.engraved),
        Ok(None) | Err(FrameError::TooManyRows(_)) => Err(RasterError::BadOutputSize { width: 0, height: 0 }),
        Err(FrameError::Raster(e)) => Err(e),
    }
}

/// Maximal runs of equal "on" values per row.
fn rows_of(r: &FrameRaster) -> Vec<Vec<Run>> {
    let cols = r.grid.cols;
    (0..r.grid.rows)
        .map(|row| {
            let at = |c: u32| -> Option<Option<u8>> {
                match &r.engraved {
                    Engraved::Bits(b) => b.get(c, row).then_some(None),
                    Engraved::Levels { levels, .. } => {
                        let l = levels[row as usize * cols as usize + c as usize];
                        (l < 255).then_some(Some(l))
                    }
                }
            };
            runs(cols, at)
        })
        .collect()
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
    raster: &FrameRaster,
    rows: Vec<Vec<Run>>,
    dot_width_correction: f64,
    bidirectional: bool,
    geometry: GeometryId,
    head: &mut Point,
    steps: &mut Vec<Step>,
    cancel: &CancellationToken,
) -> Result<(), CompileError> {
    let g = &raster.grid;
    let frame = raster.frame;
    let back = |p: Point| frame.to_bed(p);
    let (left, right) = (g.x_of(0), g.x_of(g.cols));
    let (top, bottom) = (g.y_of(0), g.y_of(g.rows.saturating_sub(1)));
    // Nearest corner (ties: bottom-left, bottom-right, top-left, top-right), as for fills.
    let start = frame.to_local(*head);
    let corners = [(left, bottom), (right, bottom), (left, top), (right, top)];
    let mut corner = corners[0];
    for c in corners {
        if start.distance(Point::new(c.0, c.1)) < start.distance(Point::new(corner.0, corner.1)) {
            corner = c;
        }
    }
    let bottom_first = corner.1 == bottom;
    let left_first = corner.0 == left;
    let trim = dot_width_correction / 2.0;
    let mut emitted = 0u64;
    for k in 0..g.rows {
        cancel.check()?;
        let r = if bottom_first { g.rows - 1 - k } else { k };
        // Laser-on extents after dot width correction; runs shorter than the correction vanish.
        let mut row: Vec<(f64, f64, Option<u8>)> = rows
            .get(r as usize)
            .map(|v| v.iter().map(|run| (g.x_of(run.c0) + trim, g.x_of(run.c1 + 1) - trim, run.level)).collect())
            .unwrap_or_default();
        row.retain(|(a, b, _)| b - a > 1e-9);
        if row.is_empty() {
            continue;
        }
        let forward = left_first != (bidirectional && emitted % 2 == 1);
        if !forward {
            row.reverse();
        }
        let y = g.y_of(r);
        for (a, b, level) in row {
            let (from, to) = if forward { (a, b) } else { (b, a) };
            let (from, to) = (back(Point::new(from, y)), back(Point::new(to, y)));
            steps.push(match level {
                None => Step::Scan { from, to, source: geometry },
                Some(level) => Step::PowerScan { from, to, level, source: geometry },
            });
            *head = to;
        }
        emitted += 1;
    }
    Ok(())
}
