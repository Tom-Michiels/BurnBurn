//! The Testing corpus `test-data/trace/` (manifest + hand-written expectations + vector references).
//! Topology from the manifest; geometry against the vector reference (bidirectional Hausdorff), never
//! against pixel overlap. Stroke cases are centerline fixtures: here only their contour topology counts.

#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;

use burnburn_geometry::Point;
use burnburn_image::{DecodeLimits, ImageRecipe, PixelScale, Threshold, decode};
use burnburn_test_support::fixtures::{fixture_dir, read_fixture};
use burnburn_trace::{FitSettings, TraceRecipe, TraceRequest, trace};
use burnburn_types::{CancellationToken, RevisionId, StableId, TaskId};
use serde_json::Value;
use support::{circle_samples, hausdorff, polygon_samples, sample_path};

/// Release criterion of the tracer plan for synthetic input: maximum distance ≤ 0.10 mm; we hold
/// the stricter p95 target 0.05 mm as maximum on these clean cases.
const MAX_DISTANCE_MM: f64 = 0.05;
const FIT_TOLERANCE_MM: f64 = 0.02;
/// A sharp corner must be a node of the path, not merely passed nearby (a rounded corner at 0.1 mm/px
/// stays within 0.05 mm, so that bound cannot tell sharp from rounded).
const CORNER_NODE_MM: f64 = 0.02;

fn px_to_mm(p: &Value, height: f64, s: f64) -> (f64, f64) {
    (p[0].as_f64().unwrap() * s, (height - p[1].as_f64().unwrap()) * s)
}

/// Reference boundary samples in mm, or `None` if the case contains strokes (centerline fixture).
fn reference_samples(reference: &Value, s: f64) -> Option<Vec<Point>> {
    let h = reference["height"].as_f64().unwrap();
    let mut out = Vec::new();
    for shape in reference["union_of"].as_array().unwrap() {
        let rings = shape.get("rings")?.as_array()?;
        for ring in rings {
            if let Some(c) = ring.get("circle") {
                let (cx, cy) = px_to_mm(&c["center"], h, s);
                out.extend(circle_samples(cx, cy, c["r"].as_f64().unwrap() * s, 0.02));
            } else {
                let poly: Vec<(f64, f64)> =
                    ring["polygon"].as_array().unwrap().iter().map(|p| px_to_mm(p, h, s)).collect();
                out.extend(polygon_samples(&poly, 0.02));
            }
        }
    }
    Some(out)
}

#[test]
fn corpus_cases_match_manifest_topology_and_reference_geometry() {
    // The manifest itself is not a listed fixture; the files it lists are read hash-checked.
    let manifest_path = fixture_dir("trace").join("manifest.json");
    let manifest: Value = serde_json::from_slice(&std::fs::read(manifest_path).unwrap()).unwrap();
    let cases = manifest["cases"].as_array().unwrap();
    assert!(cases.len() >= 8);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let s = case["pixel_size_mm"].as_f64().unwrap();
        let png = read_fixture("trace", case["image"].as_str().unwrap()).unwrap();
        let reference: Value =
            serde_json::from_slice(&read_fixture("trace", case["reference"].as_str().unwrap()).unwrap()).unwrap();
        let image = decode(&png, DecodeLimits::default()).unwrap();
        let req = TraceRequest {
            task: TaskId::from_raw(1),
            source_revision: RevisionId::INITIAL,
            source: Arc::new(image),
            recipe: TraceRecipe {
                image: ImageRecipe {
                    scale: Some(PixelScale::uniform_mm_per_pixel(s).unwrap()),
                    threshold: Threshold::Fixed(127),
                    ..ImageRecipe::default()
                },
                fit: FitSettings { tolerance_mm: FIT_TOLERANCE_MM, ..FitSettings::default() },
                ..TraceRecipe::default()
            },
        };
        let result = trace(&req, &CancellationToken::never()).unwrap();
        let expected = &case["expected"];
        assert_eq!(result.stats.closed_shapes as u64, expected["components"].as_u64().unwrap(), "{name}: components");
        assert_eq!(result.stats.holes as u64, expected["holes"].as_u64().unwrap(), "{name}: holes");

        let traced = sample_path(&result.path(), 0.02);
        if let Some(reference) = reference_samples(&reference, s) {
            let d = hausdorff(&traced, &reference);
            assert!(d <= MAX_DISTANCE_MM, "{name}: bidirectional distance {d:.4} mm");
        }
        let h = reference["height"].as_f64().unwrap();
        for corner in expected.get("corners_px").and_then(Value::as_array).into_iter().flatten() {
            let (x, y) = px_to_mm(corner, h, s);
            let at = Point::new(x, y);
            let nearest = result
                .contours
                .iter()
                .flat_map(|c| c.path.segments().iter().map(|seg| seg.end()))
                .map(|p| p.distance(at))
                .fold(f64::INFINITY, f64::min);
            assert!(nearest <= CORNER_NODE_MM, "{name}: corner {corner} has no sharp node ({nearest:.4} mm)");
        }
    }
}
