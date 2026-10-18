use burnburn_geometry::{DisplayTolerance, Path, Point};
use burnburn_types::Length;

use crate::fit::dist_point_to_segment;
use burnburn_image::{BinaryMask, PixelScale};

use crate::error::TraceError;

/// Chord tolerance for the display polylines (not a production budget).
const DISPLAY_TOLERANCE_MM: f64 = 0.005;
use crate::result::{ContourId, TraceResult};

/// Deviation of one fitted segment from the sub-pixel source contour it replaces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentDeviation {
    pub contour: usize,
    pub contour_id: ContourId,
    pub segment: usize,
    pub max_mm: f64,
    pub mean_mm: f64,
    /// Source point with the largest deviation (for a marker in the deviation map).
    pub worst_at: Point,
}

/// Pure data for the tracer's comparison view: source mask, source outline and trace in one
/// millimetre frame, plus a per-segment deviation map. No rendering types.
#[derive(Debug, Clone, PartialEq)]
pub struct ComparisonPreview {
    pub width_px: u32,
    pub height_px: u32,
    pub scale: PixelScale,
    pub mask: BinaryMask,
    pub source_outlines: Vec<Vec<Point>>,
    /// Fitted contours flattened for display.
    pub traced: Vec<Vec<Point>>,
    pub deviations: Vec<SegmentDeviation>,
    pub max_deviation_mm: f64,
    pub p95_deviation_mm: f64,
}

/// Builds the comparison data. Deviation is the distance from each sub-pixel source point to the
/// fitted segment that replaces it.
pub fn comparison_preview(result: &TraceResult) -> Result<ComparisonPreview, TraceError> {
    let display = DisplayTolerance::new(Length::from_mm(DISPLAY_TOLERANCE_MM).map_err(|_| TraceError::NonFinite)?)
        .map_err(|_| TraceError::NonFinite)?;
    let mut deviations = Vec::new();
    let mut all = Vec::new();
    let mut traced = Vec::with_capacity(result.contours.len());
    for (ci, c) in result.contours.iter().enumerate() {
        let n = c.source_points.len();
        let mut from = c.path.start();
        for (si, (seg, &(a, b))) in c.path.segments().iter().zip(&c.segment_spans).enumerate() {
            let (mut max, mut sum, mut worst_at, mut count) = (0.0f64, 0.0, from, 0usize);
            for idx in a..=b {
                let Some(&p) = c.source_points.get(idx % n.max(1)) else { continue };
                let d = dist_point_to_segment(p, from, seg);
                all.push(d);
                sum += d;
                count += 1;
                if d > max {
                    max = d;
                    worst_at = p;
                }
            }
            let mean = if count > 0 { sum / count as f64 } else { 0.0 };
            deviations.push(SegmentDeviation {
                contour: ci,
                contour_id: c.id,
                segment: si,
                max_mm: max,
                mean_mm: mean,
                worst_at,
            });
            from = seg.end();
        }
        let path = Path::from_subpaths(vec![c.path.clone()]);
        traced.extend(path.flatten(&display).into_iter().map(|p| p.points));
    }
    all.sort_by(f64::total_cmp);
    let p95 = if all.is_empty() { 0.0 } else { all[((all.len() - 1) as f64 * 0.95).round() as usize] };
    Ok(ComparisonPreview {
        width_px: result.width_px,
        height_px: result.height_px,
        scale: result.scale,
        mask: result.mask.clone(),
        source_outlines: result.contours.iter().map(|c| c.source_points.clone()).collect(),
        traced,
        deviations: deviations.clone(),
        max_deviation_mm: deviations.iter().map(|d| d.max_mm).fold(0.0, f64::max),
        p95_deviation_mm: p95,
    })
}

/// Deviation map for the overlay: per pixel cell (adjusted pixels, row-major, y-down) the largest
/// distance (mm) between a sub-pixel source point in that cell and its fitted segment; 0 where no
/// boundary passes. Plus the per-segment list and the worst segment.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviationMap {
    pub width_px: u32,
    pub height_px: u32,
    pub cells_mm: Vec<f32>,
    pub segments: Vec<SegmentDeviation>,
    pub worst: Option<SegmentDeviation>,
}

pub fn deviation_map(result: &TraceResult) -> Result<DeviationMap, TraceError> {
    let (w, h) = (result.width_px, result.height_px);
    let (sx, sy) = (result.scale.mm_per_pixel_x(), result.scale.mm_per_pixel_y());
    let mut cells = vec![0.0f32; w as usize * h as usize];
    let mut segments = Vec::new();
    for (ci, c) in result.contours.iter().enumerate() {
        let n = c.source_points.len();
        let mut from = c.path.start();
        for (si, (seg, &(a, b))) in c.path.segments().iter().zip(&c.segment_spans).enumerate() {
            let (mut max, mut sum, mut worst_at, mut count) = (0.0f64, 0.0, from, 0usize);
            for idx in a..=b {
                let Some(&p) = c.source_points.get(idx % n.max(1)) else { continue };
                let d = dist_point_to_segment(p, from, seg);
                if !d.is_finite() {
                    return Err(TraceError::NonFinite);
                }
                let (x, y) = (p.x / sx, f64::from(h) - p.y / sy);
                if x >= 0.0 && y >= 0.0 && x < f64::from(w) && y < f64::from(h) {
                    let cell = &mut cells[y as usize * w as usize + x as usize];
                    *cell = cell.max(d as f32);
                }
                sum += d;
                count += 1;
                if d > max {
                    max = d;
                    worst_at = p;
                }
            }
            let mean_mm = if count > 0 { sum / count as f64 } else { 0.0 };
            segments.push(SegmentDeviation {
                contour: ci,
                contour_id: c.id,
                segment: si,
                max_mm: max,
                mean_mm,
                worst_at,
            });
            from = seg.end();
        }
    }
    let worst = segments.iter().copied().max_by(|a, b| a.max_mm.total_cmp(&b.max_mm));
    Ok(DeviationMap { width_px: w, height_px: h, cells_mm: cells, segments, worst })
}
