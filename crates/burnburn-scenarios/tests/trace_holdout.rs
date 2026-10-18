//! Release evaluation of the tracer on the holdout split of `test-data/trace/` (manifest key
//! `holdout`, files in `holdout/`). Tracer development must not iterate these cases; this test is
//! ignored so it only runs on purpose:
//! `cargo test -p burnburn-scenarios --test trace_holdout -- --ignored --nocapture`
//!
//! Criteria (01-test-driven-design.md § Tracer, synthetic input): topology exactly as the hand-
//! written manifest expectation; bidirectional maximum distance to the vector reference ≤ 0.10 mm.
//! Results are reported, never tuned against.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_geometry::{Point, ProductionTolerance};
use burnburn_image::{DecodeLimits, ImageRecipe, PixelScale, Threshold, decode};
use burnburn_test_support::fixtures::{fixture_dir, read_fixture};
use burnburn_trace::{TraceRecipe, TraceRequest, trace};
use burnburn_types::{CancellationToken, Length, RevisionId, StableId, TaskId};
use serde_json::Value;

const MAX_DISTANCE_MM: f64 = 0.10;

fn seg_dist(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

/// Reference boundary as dense polylines in mm (y up).
fn reference_polylines(reference: &Value, s: f64) -> Vec<Vec<Point>> {
    let h = reference["height"].as_f64().unwrap();
    let mm = |p: &Value| Point::new(p[0].as_f64().unwrap() * s, (h - p[1].as_f64().unwrap()) * s);
    let mut out = Vec::new();
    for shape in reference["union_of"].as_array().unwrap() {
        for ring in shape["rings"].as_array().unwrap() {
            if let Some(c) = ring.get("circle") {
                let (cen, r) = (mm(&c["center"]), c["r"].as_f64().unwrap() * s);
                let n = 2000;
                out.push(
                    (0..=n)
                        .map(|k| std::f64::consts::TAU * k as f64 / n as f64)
                        .map(|a| Point::new(cen.x + r * a.cos(), cen.y + r * a.sin()))
                        .collect(),
                );
            } else {
                let mut poly: Vec<Point> = ring["polygon"].as_array().unwrap().iter().map(mm).collect();
                poly.push(poly[0]);
                out.push(poly);
            }
        }
    }
    out
}

fn dist_to(p: Point, lines: &[Vec<Point>]) -> f64 {
    lines.iter().flat_map(|l| l.windows(2)).map(|w| seg_dist(p, w[0], w[1])).fold(f64::INFINITY, f64::min)
}

#[test]
#[ignore = "release evaluation on the holdout split; run on purpose"]
fn holdout_cases_meet_the_release_criteria() {
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(fixture_dir("trace").join("manifest.json")).unwrap()).unwrap();
    let cases = manifest["holdout"].as_array().unwrap();
    assert!(cases.len() >= 2);
    let mut failures = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let s = case["pixel_size_mm"].as_f64().unwrap();
        let image =
            decode(&read_fixture("trace", case["image"].as_str().unwrap()).unwrap(), DecodeLimits::default()).unwrap();
        let reference: Value =
            serde_json::from_slice(&read_fixture("trace", case["reference"].as_str().unwrap()).unwrap()).unwrap();
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
                ..TraceRecipe::default()
            },
        };
        let result = trace(&req, &CancellationToken::never()).unwrap();
        let holes: usize = result.components.iter().map(|c| c.holes.len()).sum();
        let topo = (result.components.len() as u64, holes as u64);
        let want = (case["expected"]["components"].as_u64().unwrap(), case["expected"]["holes"].as_u64().unwrap());
        let tol = ProductionTolerance::new(Length::from_mm(0.001).unwrap()).unwrap();
        let traced: Vec<Vec<Point>> = result
            .path()
            .flatten(&tol)
            .into_iter()
            .map(|pl| {
                let mut p = pl.points;
                if pl.closed {
                    p.push(p[0]);
                }
                p
            })
            .collect();
        let reference = reference_polylines(&reference, s);
        let forward = traced.iter().flatten().map(|p| dist_to(*p, &reference)).fold(0.0, f64::max);
        let backward = reference.iter().flatten().step_by(5).map(|p| dist_to(*p, &traced)).fold(0.0, f64::max);
        let d = forward.max(backward);
        println!("holdout {name}: topology {topo:?} (expected {want:?}), max distance {d:.4} mm at {s} mm/px");
        if topo != want || d > MAX_DISTANCE_MM {
            failures.push(name.to_owned());
        }
    }
    assert!(failures.is_empty(), "holdout cases outside the release criteria: {failures:?}");
}
