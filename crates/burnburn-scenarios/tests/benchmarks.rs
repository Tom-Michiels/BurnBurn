//! Benchmarks as ignored tests with fixed, generated-in-test fixtures. They print wall-clock times
//! and check a correctness invariant, but never assert a time limit (one machine, no baseline).
//! Run in release:
//! `cargo test -p burnburn-scenarios --release --test benchmarks -- --ignored --nocapture --test-threads=1`
//! Results: docs/dev-notes/benchmarks.md.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::f64::consts::PI;
use std::sync::Arc;
use std::time::Instant;

use burnburn_document::{CommandPayload, GeometryCommand, ImportItem};
use burnburn_geometry::{BooleanOp, BooleanOptions, FillRule, Path, Point, ProductionTolerance, boolean};
use burnburn_image::{ImageRecipe, PixelScale, SourceImage, Threshold};
use burnburn_trace::{TraceRecipe, TraceRequest, trace};
use burnburn_types::{CancellationToken, ContentHash, RevisionId, StableId, TaskId};
use common::*;

fn regular_polygon(c: Point, r: f64, n: usize) -> Path {
    let pts: Vec<Point> = (0..n)
        .map(|k| 2.0 * PI * k as f64 / n as f64)
        .map(|a| Point::new(c.x + r * a.cos(), c.y + r * a.sin()))
        .collect();
    Path::polyline(&pts, true).unwrap()
}

fn time<T>(label: &str, f: impl FnOnce() -> T) -> T {
    let t = Instant::now();
    let out = f();
    println!("bench {label}: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);
    out
}

/// 1000 closed 100-gons on a 40 × 25 grid = 100 000 line segments, one line operation.
#[test]
#[ignore = "benchmark; run in release with --ignored --nocapture"]
fn bench_compile_100k_segments() {
    let mut s = new_session();
    let items: Vec<ImportItem> = (0..1000)
        .map(|i| {
            let c = Point::new(20.0 + 30.0 * (i % 40) as f64, 20.0 + 30.0 * (i / 40) as f64);
            ImportItem { name: format!("p{i}"), entity: None, path: regular_polygon(c, 12.0, 100) }
        })
        .collect();
    let asset = ContentHash::from_bytes([1; 32]);
    let ids = apply(&mut s, CommandPayload::Edit(GeometryCommand::Import { asset, items }), &[]).changes.created;
    add_cut(&mut s, ids);
    let snap = s.snapshot();
    let plan = time("compile 100k segments", || compile_plan(&snap));
    let report = time("preflight", || burnburn_cam::preflight(&snap, &plan, &profile()));
    assert!(!report.has_errors());
    let bytes = time("lower + encode RD", || rd_bytes(&plan));
    let rd = time("decode RD", || decode(&bytes));
    let n = cuts(&rd).len();
    println!("bench compile: {n} cut records, {} RD bytes", bytes.len());
    assert_eq!(n, 100_000, "every polygon edge is one cut");
}

/// 2000 × 2000 gray image: 10 × 10 anti-aliasing-free rings (outer r 80 px, inner r 40 px).
#[test]
#[ignore = "benchmark; run in release with --ignored --nocapture"]
fn bench_trace_2000_square() {
    let (w, h) = (2000u32, 2000u32);
    let mut px = vec![255u8; (w * h) as usize];
    for j in 0..h {
        for i in 0..w {
            let (x, y) = (f64::from(i) + 0.5, f64::from(j) + 0.5);
            let (cx, cy) = ((x / 200.0).floor() * 200.0 + 100.0, (y / 200.0).floor() * 200.0 + 100.0);
            let d2 = (x - cx).powi(2) + (y - cy).powi(2);
            if (1600.0..=6400.0).contains(&d2) {
                px[(j * w + i) as usize] = 0;
            }
        }
    }
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(SourceImage::from_gray8(w, h, px).unwrap()),
        recipe: TraceRecipe {
            image: ImageRecipe {
                scale: Some(PixelScale::uniform_mm_per_pixel(0.05).unwrap()),
                threshold: Threshold::Fixed(127),
                ..ImageRecipe::default()
            },
            ..TraceRecipe::default()
        },
    };
    let result = time("trace 2000x2000 (100 rings)", || trace(&req, &CancellationToken::never()).unwrap());
    let holes: usize = result.components.iter().map(|c| c.holes.len()).sum();
    println!("bench trace: {} components, {holes} holes, {} contours", result.components.len(), result.contours.len());
    assert_eq!((result.components.len(), holes), (100, 100));
}

/// Union of two 5000-gons (10 000 segments) approximating circles r = 50 at distance 50.
/// Oracle: area of two overlapping circles, 2πr² − lens, lens = 2r²·acos(d/2r) − (d/2)·√(4r² − d²).
#[test]
#[ignore = "benchmark; run in release with --ignored --nocapture"]
fn bench_boolean_10k_segments() {
    let (r, d) = (50.0, 50.0);
    let a = regular_polygon(Point::new(100.0, 100.0), r, 5000);
    let b = regular_polygon(Point::new(100.0 + d, 100.0), r, 5000);
    let options = BooleanOptions { fill: FillRule::NonZero, tolerance: ProductionTolerance::new(mm(0.001)).unwrap() };
    let union = time("boolean union 10k segments", || {
        boolean(&a, &b, BooleanOp::Union, &options, &CancellationToken::never()).unwrap()
    });
    let lens = 2.0 * r * r * (d / (2.0 * r)).acos() - (d / 2.0) * (4.0 * r * r - d * d).sqrt();
    let expected = 2.0 * PI * r * r - lens;
    let area = union.signed_area().abs();
    println!("bench boolean: area {area:.3} vs circles {expected:.3}, {} subpaths", union.subpaths().len());
    // A 5000-gon inscribed in r = 50 loses ≈ 2π³r²/(3n²) ≈ 0.0066 mm² per circle.
    assert!((area - expected).abs() < 0.05, "{area} vs {expected}");
    assert_eq!(union.subpaths().len(), 1);
}
