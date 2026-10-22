//! Measurement of `Packing::Tight` against `Packing::Grid` on a realistic sheet (G8-cam-werk): a
//! 600 x 400 mm sheet of mixed parts (brackets, discs, slots, triangles, plates, stars) with 3 mm
//! between parts and 5 mm to the edge, quarter turns allowed. The numbers (placed, utilisation,
//! time) are printed by the ignored test and quoted in `docs/dev-notes/G8-cam-werk.md`; the small
//! variant runs always and checks the invariants a measurement may not break.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Instant;

use burnburn_cam::nest::{
    NestOptions, NestParams, NestPart, NestResult, Packing, PartKey, RotationSet, Stock, StockId, UtilisationBasis,
    nest_packed,
};
use burnburn_geometry::{Bounds, Path, Point, ProductionTolerance, Rect};
use burnburn_types::{CancellationToken, Length};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn poly(points: &[(f64, f64)]) -> Path {
    Path::polyline(&points.iter().map(|&(x, y)| Point::new(x, y)).collect::<Vec<_>>(), true).unwrap()
}

fn rectangle(w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(0.0, 0.0), mm(w), mm(h)).unwrap())
}

/// An L-shaped bracket, 70 x 50 with 25 mm arms.
fn bracket() -> Path {
    poly(&[(0.0, 0.0), (70.0, 0.0), (70.0, 25.0), (25.0, 25.0), (25.0, 50.0), (0.0, 50.0)])
}

fn disc(diameter: f64) -> Path {
    let r = diameter / 2.0;
    poly(&(0..72)
        .map(|k| {
            let a = f64::from(k) * std::f64::consts::TAU / 72.0;
            (r + r * a.cos(), r + r * a.sin())
        })
        .collect::<Vec<_>>())
}

/// A slot (stadium) of 60 x 20.
fn slot() -> Path {
    let r = 10.0;
    let mut pts = Vec::new();
    for k in 0..=18 {
        let a = -std::f64::consts::FRAC_PI_2 + f64::from(k) * std::f64::consts::PI / 18.0;
        pts.push((50.0 + r * a.cos(), r + r * a.sin()));
    }
    for k in 0..=18 {
        let a = std::f64::consts::FRAC_PI_2 + f64::from(k) * std::f64::consts::PI / 18.0;
        pts.push((10.0 + r * a.cos(), r + r * a.sin()));
    }
    poly(&pts)
}

fn triangle() -> Path {
    poly(&[(0.0, 0.0), (50.0, 0.0), (15.0, 45.0)])
}

/// A 12-pointed star of 56 mm across.
fn star() -> Path {
    poly(&(0..24)
        .map(|k| {
            let a = f64::from(k) * std::f64::consts::TAU / 24.0;
            let r = if k % 2 == 0 { 28.0 } else { 20.0 };
            (28.0 + r * a.cos(), 28.0 + r * a.sin())
        })
        .collect::<Vec<_>>())
}

fn fixture(scale: u32) -> Vec<NestPart> {
    let rotations = RotationSet::new(vec![0.0, 90.0, 180.0, 270.0]).unwrap();
    let mk = |key: u32, outline: Path, count: u32| NestPart {
        key: PartKey(key),
        outline,
        count: count * scale,
        rotations: rotations.clone(),
        grain_deg: None,
    };
    vec![
        mk(0, bracket(), 6),
        mk(1, disc(40.0), 8),
        mk(2, slot(), 10),
        mk(3, triangle(), 6),
        mk(4, rectangle(90.0, 35.0), 5),
        mk(5, star(), 4),
    ]
}

fn sheet() -> Stock {
    Stock { id: StockId(0), outline: rectangle(600.0, 400.0), keep_out: vec![], grain_deg: None }
}

fn params(step: f64) -> NestParams {
    NestParams {
        spacing: mm(3.0),
        edge_margin: mm(5.0),
        step: mm(step),
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        max_candidates: u64::MAX,
        basis: UtilisationBasis::StockArea,
    }
}

fn run(parts: &[NestPart], step: f64, packing: Packing) -> (NestResult, f64) {
    let started = Instant::now();
    let r = nest_packed(
        parts,
        std::slice::from_ref(&sheet()),
        &params(step),
        &NestOptions::default(),
        packing,
        &CancellationToken::never(),
    )
    .unwrap();
    (r, started.elapsed().as_secs_f64())
}

fn boxes(parts: &[NestPart], r: &NestResult) -> Vec<Bounds> {
    r.placements
        .iter()
        .map(|pl| parts.iter().find(|p| p.key == pl.part).unwrap().outline.transform(&pl.transform).bounds().unwrap())
        .collect()
}

fn assert_inside_the_sheet(parts: &[NestPart], r: &NestResult) {
    for b in boxes(parts, r) {
        // 5 mm edge margin, less the slack of the exact checks.
        assert!(b.min().x >= 5.0 - 0.02 && b.min().y >= 5.0 - 0.02, "{b:?}");
        assert!(b.max().x <= 595.0 + 0.02 && b.max().y <= 395.0 + 0.02, "{b:?}");
    }
}

fn report(label: &str, step: f64, parts: &[NestPart]) {
    let ordered: u32 = parts.iter().map(|p| p.count).sum();
    for packing in [Packing::Grid, Packing::Tight] {
        let (r, secs) = run(parts, step, packing);
        assert_inside_the_sheet(parts, &r);
        // How far the parts reach: the smaller, the more room is left in one piece.
        let extent = boxes(parts, &r).iter().fold((0.0f64, 0.0f64), |e, b| (e.0.max(b.max().x), e.1.max(b.max().y)));
        println!(
            "MEASURE {label} step {step} mm {packing:?}: placed {}/{ordered}, utilisation {:.1} %, reach {:.0} x {:.0} mm, {secs:.2} s",
            r.placements.len(),
            100.0 * r.utilisation.value,
            extent.0,
            extent.1
        );
    }
}

#[test]
fn a_small_sheet_of_mixed_parts_packs_tighter_than_the_grid_and_stays_inside() {
    // A third of the full fixture (about 13 parts) at a coarse step: cheap enough for every run.
    let parts: Vec<NestPart> = fixture(1)
        .into_iter()
        .map(|mut p| {
            p.count = p.count.div_ceil(3);
            p
        })
        .collect();
    let (grid, _) = run(&parts, 4.0, Packing::Grid);
    let (tight, _) = run(&parts, 4.0, Packing::Tight);
    assert_inside_the_sheet(&parts, &grid);
    assert_inside_the_sheet(&parts, &tight);
    assert!(tight.placements.len() >= grid.placements.len(), "{} vs {}", tight.placements.len(), grid.placements.len());
    assert!(tight.utilisation.value >= grid.utilisation.value - 1e-9);
    // The utilisation is the placed area over the sheet: no more than the boxes of the parts hold.
    let ordered_boxes: f64 = parts
        .iter()
        .map(|p| {
            let b = p.outline.bounds().unwrap();
            f64::from(p.count) * b.width() * b.height()
        })
        .sum();
    assert!(tight.utilisation.value <= (ordered_boxes / (600.0 * 400.0)).min(1.0) + 1e-9);
}

/// `cargo test --release -p burnburn-cam --test nest_tight_measure -- --ignored --nocapture`
#[test]
#[ignore = "measurement: prints placed / utilisation / time for Grid and Tight on the realistic sheet"]
fn measure_tight_against_grid_on_a_realistic_sheet() {
    let parts = fixture(1);
    report("39 parts", 2.0, &parts);
    report("39 parts", 1.0, &parts);
    for scale in [2, 3, 4] {
        let more = fixture(scale);
        report(&format!("{} parts", 39 * scale), 2.0, &more);
    }
}
