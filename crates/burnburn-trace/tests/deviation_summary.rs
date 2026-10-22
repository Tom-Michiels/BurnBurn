//! G6-werk T06: the few numbers a simplification report needs (largest deviations with their place,
//! 95th percentile) without a per-pixel map. It must agree with the per-segment list of
//! `deviation_map`, which is the independent computation.

#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;

use burnburn_image::{ImageRecipe, PixelScale, Threshold};
use burnburn_trace::{FitSettings, TraceRecipe, TraceRequest, TraceResult, deviation_map, deviation_summary, trace};
use burnburn_types::{CancellationToken, RevisionId, StableId, TaskId};
use support::*;

const S: f64 = 0.1;

fn traced(tolerance_mm: f64) -> TraceResult {
    // An ellipse-ish blob and a rectangle: curved and straight stretches.
    let blob = disc(6.0, 6.0, 4.0);
    let rect = convex(vec![(12.0, 2.0), (19.0, 2.0), (19.0, 9.0), (12.0, 9.0)]);
    let source = rasterize_levels(200, 120, S, &[(&blob, 0), (&rect, 0)]);
    let recipe = TraceRecipe {
        image: ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(S).unwrap()),
            threshold: Threshold::Fixed(127),
            ..ImageRecipe::default()
        },
        fit: FitSettings { tolerance_mm, ..FitSettings::default() },
        ..TraceRecipe::default()
    };
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(source),
        recipe,
    };
    trace(&req, &CancellationToken::never()).unwrap()
}

#[test]
fn the_summary_lists_the_worst_segments_largest_first_and_agrees_with_the_per_segment_map() {
    let result = traced(0.2);
    let summary = deviation_summary(&result, 3).unwrap();
    let map = deviation_map(&result).unwrap();
    assert_eq!(summary.worst.len(), 3);
    assert!(summary.worst.windows(2).all(|w| w[0].max_mm >= w[1].max_mm), "largest first");
    let want = map.worst.unwrap();
    assert_eq!(summary.worst[0].max_mm, want.max_mm);
    assert_eq!(summary.max_mm, want.max_mm);
    assert!(summary.p95_mm <= summary.max_mm && summary.p95_mm >= 0.0);
    // Every listed place is a point of the traced image.
    let (width, height) = (f64::from(result.width_px) * S, f64::from(result.height_px) * S);
    for d in &summary.worst {
        assert!((0.0..=width).contains(&d.worst_at.x) && (0.0..=height).contains(&d.worst_at.y), "{d:?}");
    }
    // The listed segments are really among the segments of the map, with the same numbers.
    for d in &summary.worst {
        assert!(map.segments.contains(d), "{d:?}");
    }
}

#[test]
fn a_looser_fit_budget_gives_a_larger_largest_deviation_and_a_coarse_one_stays_within_it() {
    let tight = deviation_summary(&traced(0.02), 1).unwrap();
    let loose = deviation_summary(&traced(0.4), 1).unwrap();
    assert!(loose.max_mm > tight.max_mm, "{} vs {}", loose.max_mm, tight.max_mm);
    // A budget finer than half a pixel (0.05 mm here) is not reachable on a pixel boundary: the report
    // says what the fit really is instead of repeating the budget. A coarse budget is kept.
    assert!(tight.max_mm <= 0.05 + 1e-6, "{}", tight.max_mm);
    assert!(loose.max_mm <= 0.4 + 1e-6, "{}", loose.max_mm);
}

#[test]
fn asking_for_no_segments_still_gives_the_numbers() {
    let result = traced(0.2);
    let summary = deviation_summary(&result, 0).unwrap();
    assert!(summary.worst.is_empty());
    assert!(summary.max_mm > 0.0);
}
