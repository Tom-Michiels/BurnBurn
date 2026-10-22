//! Public contract of the trace as a background task: reproducible recipe, cancellation through
//! the token, stale results rejected by task/revision identity, comparison preview data.

#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;

use burnburn_image::{ImageRecipe, Morphology, PixelScale, Threshold};
use burnburn_trace::{
    ContourSettings, DiagonalPolicy, FitSettings, StaleTrace, TraceError, TraceObserver, TraceRecipe, TraceRequest,
    TraceStage, comparison_preview, trace, trace_observed,
};
use burnburn_types::{CancellationSource, CancellationToken, CanonicalWriter, RevisionId, StableId, TaskId};
use support::*;

fn blobs() -> burnburn_image::SourceImage {
    rasterize(200, 120, 0.1, |x, y| {
        disc(4.0, 6.0, 3.0)(x, y) || disc(12.0, 6.0, 2.5)(x, y) || disc(17.0, 4.0, 1.5)(x, y)
    })
}

fn recipe() -> TraceRecipe {
    TraceRecipe {
        image: ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(0.1).unwrap()),
            threshold: Threshold::Otsu,
            ..ImageRecipe::default()
        },
        ..TraceRecipe::default()
    }
}

fn request(task: u64, revision: RevisionId) -> TraceRequest {
    TraceRequest {
        task: TaskId::from_raw(task),
        source_revision: revision,
        source: Arc::new(blobs()),
        recipe: recipe(),
    }
}

fn geometry_bytes(path: &burnburn_geometry::Path) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    burnburn_types::CanonicalEncode::encode(path, &mut w);
    w.finish().unwrap()
}

#[test]
fn same_source_and_recipe_reproduce_identical_geometry_and_key() {
    let a = trace(&request(1, RevisionId::INITIAL), &CancellationToken::never()).unwrap();
    let b = trace(&request(2, RevisionId::INITIAL), &CancellationToken::never()).unwrap();
    assert_eq!(geometry_bytes(&a.path()), geometry_bytes(&b.path()), "bit-identical geometry");
    assert_eq!(a.recipe_key, b.recipe_key);
    assert_eq!(a.engine, burnburn_trace::ENGINE_VERSION);
    assert_eq!(a.source_hash, blobs().content_hash());

    // Every recipe parameter takes part in the key.
    let base = recipe();
    let variants = [
        TraceRecipe { fit: FitSettings { tolerance_mm: 0.03, ..base.fit }, ..base.clone() },
        TraceRecipe { fit: FitSettings { corner_angle_deg: 45.0, ..base.fit }, ..base.clone() },
        TraceRecipe { fit: FitSettings { corner_window_px: 3.0, ..base.fit }, ..base.clone() },
        TraceRecipe { fit: FitSettings { curves: false, ..base.fit }, ..base.clone() },
        TraceRecipe {
            contour: ContourSettings { diagonal: DiagonalPolicy::Separate, ..base.contour.clone() },
            ..base.clone()
        },
        TraceRecipe { contour: ContourSettings { min_area_mm2: 0.5, ..base.contour.clone() }, ..base.clone() },
        TraceRecipe {
            image: ImageRecipe { morphology: vec![Morphology::Open(1)], ..base.image.clone() },
            ..base.clone()
        },
        TraceRecipe { image: ImageRecipe { invert: true, ..base.image.clone() }, ..base.clone() },
    ];
    let source = blobs().content_hash();
    let key = base.key(source).unwrap();
    for v in &variants {
        assert_ne!(v.key(source).unwrap(), key, "{v:?}");
    }
    let other = rasterize(10, 10, 0.1, disc(0.5, 0.5, 0.3)).content_hash();
    assert_ne!(base.key(other).unwrap(), key, "source identity is part of the key");
}

#[test]
fn invalid_recipe_values_are_typed_errors() {
    for bad in [0.0, -1.0, f64::NAN] {
        let mut r = recipe();
        r.fit.tolerance_mm = bad;
        assert_eq!(r.validate(), Err(TraceError::InvalidRecipe("fit.tolerance_mm")));
        let req = TraceRequest { recipe: r, ..request(1, RevisionId::INITIAL) };
        assert_eq!(
            trace(&req, &CancellationToken::never()).unwrap_err(),
            TraceError::InvalidRecipe("fit.tolerance_mm")
        );
    }
    let mut r = recipe();
    r.contour.min_area_mm2 = -0.1;
    assert_eq!(r.validate(), Err(TraceError::InvalidRecipe("contour.min_area_mm2")));
}

struct CancelAfterFirstFit {
    source: CancellationSource,
    stages: Vec<TraceStage>,
}

impl TraceObserver for CancelAfterFirstFit {
    fn stage(&mut self, stage: TraceStage) {
        if matches!(stage, TraceStage::Fitted { index: 0, .. }) {
            self.source.cancel();
        }
        self.stages.push(stage);
    }
}

#[test]
fn cancellation_in_the_middle_of_fitting_returns_cancelled() {
    let source = CancellationSource::new();
    let token = source.token();
    let mut observer = CancelAfterFirstFit { source, stages: Vec::new() };
    let err = trace_observed(&request(1, RevisionId::INITIAL), &token, &mut observer).unwrap_err();
    assert!(matches!(err, TraceError::Cancelled(_)), "{err:?}");
    assert!(observer.stages.contains(&TraceStage::Contoured { contours: 3 }), "{:?}", observer.stages);
    assert!(
        !observer.stages.iter().any(|s| matches!(s, TraceStage::Fitted { index: 1, .. })),
        "work stops at the next check: {:?}",
        observer.stages
    );

    let before = CancellationSource::new();
    before.cancel();
    assert!(matches!(trace(&request(1, RevisionId::INITIAL), &before.token()), Err(TraceError::Cancelled(_))));
}

#[test]
fn newer_task_supersedes_older_result() {
    let result = trace(&request(7, RevisionId::INITIAL), &CancellationToken::never()).unwrap();
    assert_eq!((result.task, result.source_revision), (TaskId::from_raw(7), RevisionId::INITIAL));
    assert_eq!(
        result.check_fresh(TaskId::from_raw(8), result.recipe_key),
        Err(StaleTrace::Superseded { result: TaskId::from_raw(7), latest: TaskId::from_raw(8) }),
        "a newer submission for the same trace supersedes this one"
    );
}

#[test]
fn changed_threshold_rejects_stale_trace_result() {
    let result = trace(&request(7, RevisionId::INITIAL), &CancellationToken::never()).unwrap();
    let mut now = recipe();
    now.image.threshold = Threshold::Fixed(90);
    let current = now.key(blobs().content_hash()).unwrap();
    assert_eq!(
        result.check_fresh(TaskId::from_raw(7), current),
        Err(StaleTrace::InputsChanged { traced: result.recipe_key, current })
    );
    let mut cropped = recipe();
    cropped.image.crop = Some(burnburn_image::PixelRect { x: 0, y: 0, width: 100, height: 120 });
    let current = cropped.key(blobs().content_hash()).unwrap();
    assert!(result.check_fresh(TaskId::from_raw(7), current).is_err(), "crop is an input");
}

#[test]
fn comparison_preview_reports_overlay_and_per_segment_deviation() {
    let result = trace(&request(1, RevisionId::INITIAL), &CancellationToken::never()).unwrap();
    let preview = comparison_preview(&result).unwrap();
    assert_eq!(result.contours.len(), 3, "three discs");
    assert!(!preview.deviations.is_empty() && preview.traced.iter().all(|t| t.len() > 8));
    assert_eq!((preview.width_px, preview.height_px), (200, 120));
    assert_eq!(preview.mask.count(), result.mask.count());
    assert_eq!(preview.source_outlines.len(), result.contours.len());
    assert_eq!(preview.traced.len(), result.contours.len());
    let segments: usize = result.contours.iter().map(|c| c.path.segments().len()).sum();
    assert_eq!(preview.deviations.len(), segments, "one entry per fitted segment");
    let tol = recipe().fit.tolerance_mm;
    for d in &preview.deviations {
        assert!(d.max_mm >= d.mean_mm && d.mean_mm >= 0.0);
        assert!(d.max_mm <= tol + 1e-9, "segment {d:?} exceeds the fit budget {tol}");
    }
    let max = preview.deviations.iter().map(|d| d.max_mm).fold(0.0, f64::max);
    assert_eq!(preview.max_deviation_mm, max);
    assert!(preview.p95_deviation_mm <= preview.max_deviation_mm);
    // Overlay outlines are in the same millimetre frame as the trace.
    let src = sample_path(&result.path(), 0.05);
    let outline: Vec<_> = preview.source_outlines.iter().flatten().copied().collect();
    assert!(directed(&outline, &src) <= tol + 0.01);
}

#[test]
#[ignore = "timing benchmark; run with --ignored --nocapture"]
fn benchmark_2000_square_pixels() {
    let img = rasterize(2000, 2000, 0.05, |x, y| {
        let (u, v) = ((x / 4.0).floor() as i64, (y / 4.0).floor() as i64);
        (u + v) % 2 == 0 && disc(x - x.rem_euclid(4.0) + 2.0, y - y.rem_euclid(4.0) + 2.0, 1.6)(x, y)
    });
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(img),
        recipe: TraceRecipe {
            image: ImageRecipe {
                scale: Some(PixelScale::uniform_mm_per_pixel(0.05).unwrap()),
                ..ImageRecipe::default()
            },
            ..TraceRecipe::default()
        },
    };
    let start = std::time::Instant::now();
    let result = trace(&req, &CancellationToken::never()).unwrap();
    println!(
        "2000×2000 px: {} contours, {} segments in {:?}",
        result.contours.len(),
        result.stats.nodes,
        start.elapsed()
    );
}

#[test]
fn unrelated_document_edit_keeps_trace_result_valid() {
    // DR-14: another object moved (revision 1 → 2), the trace inputs did not change.
    let r1 = RevisionId::INITIAL;
    let r2 = r1.next().unwrap();
    let result = trace(&request(7, r1), &CancellationToken::never()).unwrap();
    let current = recipe().key(blobs().content_hash()).unwrap();
    assert_ne!(r1, r2);
    assert_eq!(result.check_fresh(TaskId::from_raw(7), current), Ok(()));
}
