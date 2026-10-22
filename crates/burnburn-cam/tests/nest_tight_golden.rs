//! Characterisation of `Packing::Tight` (G9-cam-werk): the placements of three fixed scenes, recorded
//! from the implementation before it was made faster (`tests/data/nest_tight_golden.txt`). A speed-up
//! of the exact checks may not move any part: same order, same rotation, same position to 1 µm.
//! The scenes cover a plain rectangular sheet, a sheet with a keep-out (so a non-rectangular usable
//! area) with mirroring, and a part with a hole on two sheets. Regenerate only after reviewing a
//! deliberate change:
//! `BB_PRINT_GOLDEN=1 cargo test -p burnburn-cam --test nest_tight_golden -- --nocapture`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt::Write as _;

use burnburn_cam::nest::{
    MirrorPolicy, NestOptions, NestParams, NestPart, NestResult, Packing, PartKey, RotationSet, Stock, StockId,
    UtilisationBasis, nest_packed_counted,
};
use burnburn_geometry::{Path, Point, ProductionTolerance, Rect};
use burnburn_types::{CancellationToken, Length};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn poly(points: &[(f64, f64)]) -> Path {
    Path::polyline(&points.iter().map(|&(x, y)| Point::new(x, y)).collect::<Vec<_>>(), true).unwrap()
}

fn rectangle_at(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap())
}

fn bracket() -> Path {
    poly(&[(0.0, 0.0), (70.0, 0.0), (70.0, 25.0), (25.0, 25.0), (25.0, 50.0), (0.0, 50.0)])
}

fn disc(diameter: f64) -> Path {
    let r = diameter / 2.0;
    poly(
        &(0..48)
            .map(|k| {
                let a = f64::from(k) * std::f64::consts::TAU / 48.0;
                (r + r * a.cos(), r + r * a.sin())
            })
            .collect::<Vec<_>>(),
    )
}

fn triangle() -> Path {
    poly(&[(0.0, 0.0), (50.0, 0.0), (15.0, 45.0)])
}

fn star() -> Path {
    poly(
        &(0..24)
            .map(|k| {
                let a = f64::from(k) * std::f64::consts::TAU / 24.0;
                let r = if k % 2 == 0 { 28.0 } else { 20.0 };
                (28.0 + r * a.cos(), 28.0 + r * a.sin())
            })
            .collect::<Vec<_>>(),
    )
}

/// A 60 x 40 plate with a 20 x 10 hole.
fn plate_with_hole() -> Path {
    let outer = rectangle_at(0.0, 0.0, 60.0, 40.0);
    let hole = rectangle_at(20.0, 15.0, 20.0, 10.0);
    let mut sub = outer.subpaths().to_vec();
    sub.extend(hole.subpaths().iter().cloned());
    Path::from_subpaths(sub)
}

fn part(key: u32, outline: Path, count: u32, rotations: &[f64]) -> NestPart {
    NestPart {
        key: PartKey(key),
        outline,
        count,
        rotations: RotationSet::new(rotations.to_vec()).unwrap(),
        grain_deg: None,
    }
}

fn params(step: f64, spacing: f64) -> NestParams {
    NestParams {
        spacing: mm(spacing),
        edge_margin: mm(5.0),
        step: mm(step),
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        max_candidates: u64::MAX,
        basis: UtilisationBasis::StockArea,
    }
}

fn rows(r: &NestResult) -> String {
    let mut out = String::new();
    for p in &r.placements {
        let c = p.transform.coefficients();
        writeln!(
            out,
            "part {} copy {} sheet {} rot {:.1} mirrored {} at {:.3} {:.3}",
            p.part.0, p.copy, p.stock.0, p.rotation_deg, p.mirrored, c[4], c[5]
        )
        .unwrap();
    }
    for u in &r.unplaced {
        writeln!(out, "unplaced {} x{} {:?}", u.part.0, u.count, u.reason).unwrap();
    }
    out
}

fn scene_plain() -> String {
    scene_plain_counted().0
}

fn scene_plain_counted() -> (String, u64) {
    let quarter = [0.0, 90.0, 180.0, 270.0];
    let parts = vec![
        part(0, bracket(), 4, &quarter),
        part(1, disc(40.0), 5, &quarter),
        part(2, triangle(), 4, &quarter),
        part(3, rectangle_at(0.0, 0.0, 90.0, 35.0), 3, &quarter),
        part(4, star(), 3, &quarter),
    ];
    let stock =
        Stock { id: StockId(0), outline: rectangle_at(0.0, 0.0, 400.0, 250.0), keep_out: vec![], grain_deg: None };
    let (r, work) = nest_packed_counted(
        &parts,
        std::slice::from_ref(&stock),
        &params(3.0, 3.0),
        &NestOptions::default(),
        Packing::Tight,
        &CancellationToken::never(),
    )
    .unwrap();
    (rows(&r), work.overlays)
}

fn scene_keep_out_and_mirror() -> String {
    let parts = vec![
        part(0, bracket(), 3, &[0.0, 90.0, 180.0, 270.0]),
        part(1, triangle(), 3, &[0.0, 90.0, 180.0, 270.0]),
        part(2, disc(30.0), 3, &[0.0]),
    ];
    let stock = Stock {
        id: StockId(0),
        outline: rectangle_at(0.0, 0.0, 260.0, 220.0),
        keep_out: vec![rectangle_at(100.0, 80.0, 60.0, 50.0)],
        grain_deg: None,
    };
    let r = nest_packed_counted(
        &parts,
        std::slice::from_ref(&stock),
        &params(3.0, 2.0),
        &NestOptions { mirror: MirrorPolicy::Allowed },
        Packing::Tight,
        &CancellationToken::never(),
    )
    .unwrap()
    .0;
    rows(&r)
}

fn scene_holes_and_two_sheets() -> String {
    let parts = vec![part(0, plate_with_hole(), 5, &[0.0, 90.0]), part(1, disc(25.0), 6, &[0.0])];
    let stocks = [
        Stock { id: StockId(0), outline: rectangle_at(0.0, 0.0, 150.0, 100.0), keep_out: vec![], grain_deg: None },
        Stock { id: StockId(1), outline: rectangle_at(200.0, 0.0, 150.0, 100.0), keep_out: vec![], grain_deg: None },
    ];
    let r = nest_packed_counted(
        &parts,
        &stocks,
        &params(2.5, 1.0),
        &NestOptions::default(),
        Packing::Tight,
        &CancellationToken::never(),
    )
    .unwrap()
    .0;
    rows(&r)
}

#[test]
fn tight_placements_of_three_scenes_match_the_recorded_implementation() {
    let got = format!(
        "== plain\n{}== keep-out and mirror\n{}== holes, two sheets\n{}",
        scene_plain(),
        scene_keep_out_and_mirror(),
        scene_holes_and_two_sheets()
    );
    if std::env::var_os("BB_PRINT_GOLDEN").is_some() {
        println!("{got}");
    }
    let want = include_str!("data/nest_tight_golden.txt");
    assert_eq!(got.trim_end(), want.trim_end());
}

/// The exact checks are the cost of a tight nest. Before G9 this scene needed `BASELINE_OVERLAYS`
/// polygon overlays (every candidate: one against the usable area and one per neighbour); clear
/// overlaps are now rejected without an overlay and a rectangular sheet needs none for the sheet
/// edge, so the same placements (see the test above) cost a fraction of it.
#[test]
fn tight_needs_far_fewer_polygon_overlays_for_the_same_placements() {
    const BASELINE_OVERLAYS: u64 = 15_006;
    let (_, overlays) = scene_plain_counted();
    println!("overlays: {overlays}");
    assert!(overlays * 3 <= BASELINE_OVERLAYS, "{overlays} overlays against a baseline of {BASELINE_OVERLAYS}");
}
