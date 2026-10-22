//! G9-trace: a protected corner in a local re-trace with another threshold. The picture has soft edges (a
//! linear ramp of 1.5 mm across each edge), so a darker local threshold moves the whole outline inwards by
//! several pixels and the roof apex of the house moves with it, further from the stored corner than the
//! 3 px reach of a base trace. The apex of the shrunk house is the analytic oracle: the roof lines move
//! inwards by d and the apex by d / sin(half the interior angle) along the vertical bisector.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;

use burnburn_geometry::{Point, Segment};
use burnburn_image::{ImageRecipe, PixelRect, PixelScale, RegionShape, SourceImage, Threshold};
use burnburn_trace::{
    ContourSettings, CorrectionRequest, FitSettings, TraceRecipe, TraceRequest, TraceResult, retrace_region, trace,
};
use burnburn_types::{CancellationToken, RevisionId, StableId, TaskId};

const S: f64 = 0.1;
const W: u32 = 160;
const H: u32 = 130;
const RAMP_MM: f64 = 1.5;
/// The roof apex in mm (y up) and, in adjusted pixels (y down), where the base trace has it.
const APEX_MM: (f64, f64) = (8.0, 8.6);
const APEX_PX: [f64; 2] = [80.0, 44.0];
const HOUSE: [(f64, f64); 5] = [(2.0, 2.0), (14.0, 2.0), (14.0, 8.0), (8.0, 8.6), (2.0, 8.0)];

/// Signed distance (mm, positive inside) to the convex counter-clockwise house.
fn depth(x: f64, y: f64) -> f64 {
    (0..HOUSE.len())
        .map(|i| {
            let (a, b) = (HOUSE[i], HOUSE[(i + 1) % HOUSE.len()]);
            ((b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0)) / (b.0 - a.0).hypot(b.1 - a.1)
        })
        .fold(f64::INFINITY, f64::min)
}

fn soft_house() -> Arc<SourceImage> {
    let mut px = Vec::new();
    for row in 0..H {
        for col in 0..W {
            let (x, y) = ((f64::from(col) + 0.5) * S, (f64::from(H - row - 1) + 0.5) * S);
            let coverage = (0.5 + depth(x, y) / RAMP_MM).clamp(0.0, 1.0);
            px.push((255.0 * (1.0 - coverage)).round() as u8);
        }
    }
    Arc::new(SourceImage::from_gray8(W, H, px).unwrap())
}

fn recipe(threshold: u8, corners: Vec<[f64; 2]>) -> TraceRecipe {
    TraceRecipe {
        image: ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(S).unwrap()),
            threshold: Threshold::Fixed(threshold),
            ..ImageRecipe::default()
        },
        contour: ContourSettings { corners, ..ContourSettings::default() },
        // A generous budget: a smooth curve rounds a mild apex off within 0.6 mm.
        fit: FitSettings { tolerance_mm: 0.6, ..FitSettings::default() },
    }
}

fn base() -> Arc<TraceResult> {
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: soft_house(),
        recipe: recipe(127, Vec::new()),
    };
    Arc::new(trace(&req, &CancellationToken::never()).unwrap())
}

fn local(threshold: u8, corners: Vec<[f64; 2]>) -> TraceResult {
    let req = CorrectionRequest {
        task: TaskId::from_raw(2),
        previous: base(),
        source: soft_house(),
        region: RegionShape::Rect(PixelRect { x: 0, y: 0, width: W, height: H }),
        recipe: recipe(threshold, corners),
        anchors: Vec::new(),
        protected: BTreeSet::new(),
        node_target: None,
    };
    retrace_region(&req, &CancellationToken::never()).unwrap()
}

/// Distance in mm from `p` to the nearest node of the first contour.
fn node_distance(r: &TraceResult, p: Point) -> f64 {
    let sub = &r.contours[0].path;
    std::iter::once(sub.start())
        .chain(sub.segments().iter().map(Segment::end))
        .map(|n| n.distance(p))
        .fold(f64::INFINITY, f64::min)
}

/// Ink is gray below `threshold`, i.e. coverage above `1 - threshold / 255`; that edge lies `shift` mm inside.
fn shift_mm(threshold: u8) -> f64 {
    ((1.0 - f64::from(threshold) / 255.0) - 0.5) * RAMP_MM
}

/// Where the apex of the shrunk house is: the roof lines move in by `d`, the apex down by d / sin(half angle).
fn shrunk_apex(d: f64) -> Point {
    let half = (0.6f64 / 6.0).atan(); // each roof line rises 0.6 mm over 6 mm
    let interior_half = std::f64::consts::FRAC_PI_2 - half;
    Point::new(APEX_MM.0, APEX_MM.1 - d / interior_half.sin())
}

#[test]
fn without_a_protected_corner_the_darker_local_threshold_rounds_the_apex_off() {
    let r = local(40, Vec::new());
    let apex = shrunk_apex(shift_mm(40));
    assert!(node_distance(&r, apex) > 0.15, "the fixture must lose the apex: {}", node_distance(&r, apex));
}

#[test]
fn a_protected_corner_follows_the_apex_that_moved_beyond_the_3_px_reach() {
    let d = shift_mm(40);
    assert!(d / S > 4.5, "the outline moves by {} px, past the 3 px reach", d / S);
    let r = local(40, vec![APEX_PX]);
    let apex = shrunk_apex(d);
    assert!(node_distance(&r, apex) < 0.12, "a node on the shrunk apex: {}", node_distance(&r, apex));
    assert_eq!(r.stats.corners_unmatched, 0);
}

#[test]
fn a_corner_stored_too_far_from_where_the_apex_is_now_is_reported_not_guessed() {
    // The apex is 5 px below its stored place at threshold 40; a corner stored 6 px above it is 11 px away.
    let r = local(40, vec![[APEX_PX[0], APEX_PX[1] - 6.0]]);
    assert_eq!(r.stats.corners_unmatched, 1);
    assert!(node_distance(&r, shrunk_apex(shift_mm(40))) > 0.15, "no node was forced anywhere");
}

#[test]
fn a_corner_that_names_no_outline_at_all_is_still_reported() {
    let r = local(40, vec![[5.0, 5.0]]);
    assert_eq!(r.stats.corners_unmatched, 1);
}
