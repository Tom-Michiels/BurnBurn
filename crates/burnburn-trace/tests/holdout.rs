//! IM24 holdout benchmark: quality on corpus cases that were *not* used to tune defaults.
//! The split (`cases[].split = "tuning" | "holdout"`) is owned by Testing (requested in
//! docs/coordination/trace.md). Run: `cargo test -p burnburn-trace --release --test holdout -- --ignored --nocapture`.

#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;
use std::time::Instant;

use burnburn_geometry::Point;
use burnburn_image::{DecodeLimits, ImageRecipe, PixelScale, Threshold, decode};
use burnburn_test_support::fixtures::{fixture_dir, read_fixture};
use burnburn_trace::{
    CenterlineRequest, CenterlineSettings, FitSettings, TraceRecipe, TraceRequest, trace, trace_centerline,
};
use burnburn_types::{CancellationToken, RevisionId, StableId, TaskId};
use serde_json::Value;
use support::{circle_samples, hausdorff, polygon_samples, sample_path};

/// Release criteria of the tracer plan (02-tracer.md) for synthetic input.
const CONTOUR_MAX_MM: f64 = 0.10;
const NODE_MAX_MM: f64 = 0.2;

#[test]
#[ignore = "needs the tuning/holdout split in test-data/trace/manifest.json (requested from Testing)"]
fn holdout_benchmark_reports_quality_on_cases_not_used_for_tuning() {
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(fixture_dir("trace").join("manifest.json")).unwrap()).unwrap();
    let cases = manifest["cases"].as_array().unwrap();
    let holdout: Vec<&Value> =
        cases.iter().filter(|c| c.get("split").and_then(Value::as_str) == Some("holdout")).collect();
    assert!(
        !holdout.is_empty(),
        "manifest has no holdout split yet (requested from Testing in docs/coordination/trace.md)"
    );
    let tuning = cases.iter().filter(|c| c.get("split").and_then(Value::as_str) == Some("tuning")).count();
    eprintln!("holdout benchmark: {} holdout / {} tuning cases", holdout.len(), tuning);
    for case in holdout {
        let name = case["name"].as_str().unwrap();
        let s = case["pixel_size_mm"].as_f64().unwrap();
        let image = Arc::new(
            decode(&read_fixture("trace", case["image"].as_str().unwrap()).unwrap(), DecodeLimits::default()).unwrap(),
        );
        let reference: Value =
            serde_json::from_slice(&read_fixture("trace", case["reference"].as_str().unwrap()).unwrap()).unwrap();
        let h = reference["height"].as_f64().unwrap();
        let mm = |p: &Value| Point::new(p[0].as_f64().unwrap() * s, (h - p[1].as_f64().unwrap()) * s);
        let image_recipe = ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(s).unwrap()),
            threshold: Threshold::Fixed(127),
            ..ImageRecipe::default()
        };
        let started = Instant::now();
        if let Some(nodes) = case["expected"].get("skeleton_nodes_px").and_then(Value::as_array) {
            let req = CenterlineRequest {
                task: TaskId::from_raw(1),
                source_revision: RevisionId::INITIAL,
                source: image,
                image: image_recipe,
                settings: CenterlineSettings::default(),
            };
            let r = trace_centerline(&req, &CancellationToken::never()).unwrap();
            let mut worst = 0.0f64;
            for want in nodes {
                let degree = want["degree"].as_u64().unwrap() as usize;
                let d = r
                    .nodes
                    .iter()
                    .filter(|n| n.degree() == degree)
                    .map(|n| n.at.distance(mm(&want["at"])))
                    .fold(f64::INFINITY, f64::min);
                worst = worst.max(d);
            }
            eprintln!(
                "{name:24} centerline nodes {}/{} worst {worst:.3} mm  {:?}",
                r.nodes.len(),
                nodes.len(),
                started.elapsed()
            );
            assert_eq!(r.nodes.len(), nodes.len(), "{name}");
            assert!(worst <= NODE_MAX_MM, "{name}: node error {worst:.3} mm");
        } else {
            let req = TraceRequest {
                task: TaskId::from_raw(1),
                source_revision: RevisionId::INITIAL,
                source: image,
                recipe: TraceRecipe {
                    image: image_recipe,
                    fit: FitSettings { tolerance_mm: 0.02, ..FitSettings::default() },
                    ..TraceRecipe::default()
                },
            };
            let r = trace(&req, &CancellationToken::never()).unwrap();
            let mut refs = Vec::new();
            for shape in reference["union_of"].as_array().unwrap() {
                for ring in shape["rings"].as_array().into_iter().flatten() {
                    if let Some(c) = ring.get("circle") {
                        let at = mm(&c["center"]);
                        refs.extend(circle_samples(at.x, at.y, c["r"].as_f64().unwrap() * s, 0.02));
                    } else {
                        let poly: Vec<(f64, f64)> = ring["polygon"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|p| {
                                let q = mm(p);
                                (q.x, q.y)
                            })
                            .collect();
                        refs.extend(polygon_samples(&poly, 0.02));
                    }
                }
            }
            let d = hausdorff(&sample_path(&r.path(), 0.02), &refs);
            eprintln!("{name:24} contour max {d:.4} mm, {} nodes  {:?}", r.stats.nodes, started.elapsed());
            assert_eq!(r.stats.closed_shapes as u64, case["expected"]["components"].as_u64().unwrap(), "{name}");
            assert_eq!(r.stats.holes as u64, case["expected"]["holes"].as_u64().unwrap(), "{name}");
            assert!(d <= CONTOUR_MAX_MM, "{name}: {d:.4} mm");
        }
    }
}
