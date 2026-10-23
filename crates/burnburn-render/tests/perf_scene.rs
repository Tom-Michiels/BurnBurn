//! X04 / W4-prestaties: scene translation (no GPU) on the 100k-segment fixtures F100K-A/B/C
//! (layout in docs/dev-notes/W4-prestaties.md; same grid as burnburn-scenarios benchmarks_100k).
//! The ignored benchmark prints p50/p95/max; the plain test pins the behaviour of culling.
//! `cargo test -p burnburn-render --release --test perf_scene -- --ignored --nocapture`

#![allow(clippy::unwrap_used, clippy::panic)]

use std::f64::consts::PI;
use std::sync::Arc;
use std::time::Instant;

use burnburn_editor::{Camera, SceneItem, SceneSnapshot, StyleRole, Viewport};
use burnburn_geometry::{Affine, Path, Point};
use burnburn_render::{Primitive, Theme, build_packet};
use burnburn_types::{DocumentId, GeometryId, RevisionId, StableId};

/// (name, objects, sides, spacing, radius): the F100K fixtures.
const FIXTURES: [(&str, usize, usize, f64, f64); 3] = [
    ("F100K-A 1000x100-gon", 1_000, 100, 30.0, 12.0),
    ("F100K-B 10000x10-gon", 10_000, 10, 9.0, 3.5),
    ("F100K-C 100000x1-line", 100_000, 1, 3.0, 1.0),
];

fn scene(objects: usize, sides: usize, spacing: f64, radius: f64, camera: Camera) -> SceneSnapshot {
    let cols = ((objects as f64 * 1.6).sqrt().round() as usize).max(1);
    let items = (0..objects)
        .map(|i| {
            let c = Point::new(10.0 + spacing * (i % cols) as f64, 10.0 + spacing * (i / cols) as f64);
            let path = if sides == 1 {
                Path::line(Point::new(c.x - radius, c.y), Point::new(c.x + radius, c.y)).unwrap()
            } else {
                let pts: Vec<Point> = (0..sides)
                    .map(|k| 2.0 * PI * k as f64 / sides as f64)
                    .map(|a| Point::new(c.x + radius * a.cos(), c.y + radius * a.sin()))
                    .collect();
                Path::polyline(&pts, true).unwrap()
            };
            SceneItem {
                source: GeometryId::from_raw(i as u64 + 1),
                path: Arc::new(path),
                transform: Affine::IDENTITY,
                role: StyleRole::Design,
                fill_slot: None,
            }
        })
        .collect();
    SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        camera,
        viewport: Viewport::new(1300.0, 800.0, 2.0).unwrap(),
        items,
        overlays: Vec::new(),
    }
}

fn strokes(scene: &SceneSnapshot) -> usize {
    build_packet(scene, &Theme::light()).primitives.iter().filter(|p| matches!(p, Primitive::Stroke { .. })).count()
}

#[test]
#[ignore = "benchmark; run in release with --ignored --nocapture"]
fn bench_scene_translation_100k() {
    for (name, objects, sides, spacing, radius) in FIXTURES {
        println!("== {name} ==");
        for (label, ppm, centre) in [
            ("whole bed in view", 1.0, Point::new(600.0, 375.0)),
            ("zoomed 10x on a corner", 10.0, Point::new(60.0, 40.0)),
        ] {
            let s = scene(objects, sides, spacing, radius, Camera::new(centre, ppm).unwrap());
            let mut v: Vec<f64> = (0..10)
                .map(|_| {
                    let t = Instant::now();
                    std::hint::black_box(build_packet(&s, &Theme::light()));
                    t.elapsed().as_secs_f64() * 1e3
                })
                .collect();
            v.sort_by(f64::total_cmp);
            println!("bench build_packet ({label}): p50 {:.2} ms p95 {:.2} ms max {:.2} ms", v[4], v[9], v[9]);
            println!("   strokes emitted: {}", strokes(&s));
        }
    }
}
