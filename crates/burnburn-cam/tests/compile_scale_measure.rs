//! Compile cost against the number of contours (G9-cam-werk): `n` separate lines on a grid, one cut
//! operation, the whole `compile`. The ignored test prints the time for doubling `n` so that a
//! quadratic step shows as a factor 4 instead of 2; the small test checks the plan is complete.
//!
//! `cargo test --release -p burnburn-cam --test compile_scale_measure -- --ignored --nocapture`

#![allow(clippy::unwrap_used)] // test helpers

mod common;

use std::time::Instant;

use burnburn_cam::compile;
use burnburn_document::{DocumentSession, DocumentSnapshot, GeometryObject, GeometrySource, ObjectKind};
use burnburn_geometry::{Path, Point};
use burnburn_job::PlanEvent;
use burnburn_types::{CancellationToken, DocumentId, GeometryId, StableId};
use common::*;

/// `n` horizontal lines of 2 mm on a 3 mm grid, as the F100K-C fixture of the scenarios benchmark.
fn lines(n: usize) -> (DocumentSession, Vec<GeometryId>) {
    let cols = ((n as f64 * 1.6).sqrt().round() as usize).max(1);
    let objects: Vec<GeometryObject> = (0..n)
        .map(|i| {
            let (x, y) = (10.0 + 3.0 * (i % cols) as f64, 10.0 + 3.0 * (i / cols) as f64);
            let path = Path::line(Point::new(x - 1.0, y), Point::new(x + 1.0, y)).unwrap();
            GeometryObject::new(
                GeometryId::from_raw(i as u64 + 1),
                format!("L{i}"),
                ObjectKind::Path,
                GeometrySource::Drawn,
                path,
            )
        })
        .collect();
    let ids: Vec<GeometryId> = objects.iter().map(|o| o.id()).collect();
    let snapshot = DocumentSnapshot::from_parts(objects, n as u64 + 1).unwrap();
    let mut s = DocumentSession::from_snapshot(DocumentId::from_raw(1), snapshot);
    add_op(&mut s, line_op("Snijden", 100.0, 20.0), ids.clone());
    (s, ids)
}

#[test]
fn a_few_hundred_lines_compile_to_one_cut_each() {
    let (s, ids) = lines(400);
    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    let cuts = plan.events().iter().filter(|e| matches!(e, PlanEvent::Cut { .. })).count();
    assert_eq!(cuts, ids.len(), "one cut per line");
}

#[test]
#[ignore = "measurement"]
fn measure_compile_against_the_number_of_contours() {
    let mut last = None;
    for n in [12_500usize, 25_000, 50_000, 100_000, 200_000] {
        let (s, _) = lines(n);
        let snapshot = s.snapshot();
        let started = Instant::now();
        let plan = compile(&snapshot, &profile(), &options(), &CancellationToken::never()).unwrap();
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let factor = last.map_or(String::new(), |l: f64| format!(", x{:.2} against the half", ms / l));
        println!("MEASURE compile {n} contours: {ms:.1} ms{factor} ({} events)", plan.events().len());
        last = Some(ms);
    }
}
