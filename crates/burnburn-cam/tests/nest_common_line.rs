//! Common line in the nest (C12, G7-cam): without spacing and with tight packing, parts touch and
//! share edges that can be cut once. `shared_edge_length_mm` says how much. Oracles: analytic for
//! rectangles (the length of the touching sides of their boxes).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_cam::nest::{
    NestOptions, NestParams, NestPart, NestResult, Packing, PartKey, RotationSet, Stock, StockId, UtilisationBasis,
    nest_packed, shared_edge_length_mm,
};
use burnburn_geometry::{Bounds, Path, Point, ProductionTolerance, Rect};
use burnburn_types::{CancellationToken, Length};
use proptest::prelude::*;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn rectangle(w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::ORIGIN, mm(w), mm(h)).unwrap())
}

fn params(spacing: f64) -> NestParams {
    NestParams {
        spacing: mm(spacing),
        edge_margin: mm(0.0),
        step: mm(1.0),
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        max_candidates: u64::MAX,
        basis: UtilisationBasis::StockArea,
    }
}

fn part(key: u32, w: f64, h: f64, count: u32) -> NestPart {
    NestPart {
        key: PartKey(key),
        outline: rectangle(w, h),
        count,
        rotations: RotationSet::new(vec![0.0]).unwrap(),
        grain_deg: None,
    }
}

fn run(parts: &[NestPart], sheet_w: f64, sheet_h: f64, spacing: f64) -> NestResult {
    let stock = Stock { id: StockId(1), outline: rectangle(sheet_w, sheet_h), keep_out: vec![], grain_deg: None };
    nest_packed(
        parts,
        std::slice::from_ref(&stock),
        &params(spacing),
        &NestOptions::default(),
        Packing::Tight,
        &CancellationToken::never(),
    )
    .unwrap()
}

fn shared(parts: &[NestPart], r: &NestResult) -> f64 {
    shared_edge_length_mm(parts, r, &ProductionTolerance::new(mm(0.01)).unwrap(), 0.02)
}

#[test]
fn two_parts_side_by_side_share_the_length_of_their_common_side() {
    let parts = [part(1, 20.0, 10.0, 2)];
    let r = run(&parts, 40.0, 10.0, 0.0);
    assert_eq!(r.placements.len(), 2, "{:?}", r.unplaced);
    assert!((shared(&parts, &r) - 10.0).abs() < 0.02, "{}", shared(&parts, &r));
}

#[test]
fn a_grid_of_four_squares_shares_four_edges() {
    let parts = [part(1, 10.0, 10.0, 4)];
    let r = run(&parts, 20.0, 20.0, 0.0);
    assert_eq!(r.placements.len(), 4, "{:?}", r.unplaced);
    assert!((shared(&parts, &r) - 40.0).abs() < 0.05, "{}", shared(&parts, &r));
}

#[test]
fn parts_with_spacing_share_nothing() {
    let parts = [part(1, 10.0, 10.0, 4)];
    let r = run(&parts, 30.0, 30.0, 1.0);
    assert_eq!(r.placements.len(), 4);
    assert_eq!(shared(&parts, &r), 0.0);
}

fn boxes(parts: &[NestPart], r: &NestResult) -> Vec<Bounds> {
    r.placements
        .iter()
        .map(|pl| parts.iter().find(|p| p.key == pl.part).unwrap().outline.transform(&pl.transform).bounds().unwrap())
        .collect()
}

/// Length of the sides two boxes share (they touch within `tol`), 0 when they do not.
fn shared_sides(a: &Bounds, b: &Bounds, tol: f64) -> f64 {
    let overlap = |a0: f64, a1: f64, b0: f64, b1: f64| (a1.min(b1) - a0.max(b0)).max(0.0);
    let mut total = 0.0;
    if (a.max().x - b.min().x).abs() <= tol || (b.max().x - a.min().x).abs() <= tol {
        total += overlap(a.min().y, a.max().y, b.min().y, b.max().y);
    }
    if (a.max().y - b.min().y).abs() <= tol || (b.max().y - a.min().y).abs() <= tol {
        total += overlap(a.min().x, a.max().x, b.min().x, b.max().x);
    }
    total
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]

    /// For any set of rectangles nested without spacing, the shared length equals the touching
    /// sides of their boxes, counted independently.
    #[test]
    fn the_shared_length_is_the_length_of_the_touching_sides(
        sizes in prop::collection::vec((5.0f64..20.0, 5.0f64..20.0, 1u32..3), 1..4),
    ) {
        let parts: Vec<NestPart> =
            sizes.iter().enumerate().map(|(i, &(w, h, n))| part(i as u32 + 1, w, h, n)).collect();
        let r = run(&parts, 60.0, 40.0, 0.0);
        let bs = boxes(&parts, &r);
        let mut expected = 0.0;
        for (i, a) in bs.iter().enumerate() {
            for b in &bs[i + 1..] {
                expected += shared_sides(a, b, 0.02);
            }
        }
        let got = shared(&parts, &r);
        prop_assert!((got - expected).abs() < 0.05 * (bs.len() as f64).max(1.0), "{} vs {}", got, expected);
    }
}
