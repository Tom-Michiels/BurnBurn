//! Production nesting (C12, G5-cam): mirroring is a choice, never taken to improve a score; the
//! result says how much of each sheet is used and left, which orders are complete and how many
//! complete sets were placed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_cam::nest::{
    MirrorPolicy, NestOptions, NestParams, NestPart, PartKey, RotationSet, Stock, StockId, UnplacedReason,
    UtilisationBasis, complete_sets, nest_with, order_status,
};
use burnburn_geometry::{Path, Point, ProductionTolerance, Rect};
use burnburn_types::{CancellationToken, Length};
use proptest::prelude::*;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn poly(points: &[(f64, f64)]) -> Path {
    Path::polyline(&points.iter().map(|&(x, y)| Point::new(x, y)).collect::<Vec<_>>(), true).unwrap()
}

fn square(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap())
}

fn params() -> NestParams {
    NestParams {
        spacing: mm(0.0),
        edge_margin: mm(0.0),
        step: mm(0.5),
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        max_candidates: u64::MAX,
        basis: UtilisationBasis::StockArea,
    }
}

fn every_15_degrees() -> RotationSet {
    RotationSet::new((0..24).map(|k| f64::from(k) * 15.0).collect()).unwrap()
}

fn part(key: u32, outline: Path, count: u32, rotations: RotationSet) -> NestPart {
    NestPart { key: PartKey(key), outline, count, rotations, grain_deg: None }
}

fn stock(id: u32, outline: Path) -> Stock {
    Stock { id: StockId(id), outline, keep_out: vec![], grain_deg: None }
}

fn opts(mirror: MirrorPolicy) -> NestOptions {
    NestOptions { mirror }
}

fn never() -> CancellationToken {
    CancellationToken::never()
}

/// +1 for a proper transform, −1 for a mirrored one.
fn handedness(t: &burnburn_geometry::Affine) -> f64 {
    let (a, b) = (
        t.apply_vector(burnburn_geometry::Vector::new(1.0, 0.0)),
        t.apply_vector(burnburn_geometry::Vector::new(0.0, 1.0)),
    );
    (a.x * b.y - a.y * b.x).signum()
}

/// A scalene triangle (no rotation allowed) and a sheet that is its mirror image, 12 % bigger: the
/// apex of the part lies outside the sheet's, so it fits only mirrored.
fn chiral() -> (NestPart, Stock) {
    let tri = [(0.0, 0.0), (40.0, 0.0), (10.0, 30.0)];
    let part = part(1, poly(&tri), 1, RotationSet::new(vec![0.0]).unwrap());
    // Mirror image (x -> 40 - x), scaled 1.12 about its centroid.
    let mirrored: Vec<(f64, f64)> = tri.iter().map(|&(x, y)| (40.0 - x, y)).collect();
    let (cx, cy) = (mirrored.iter().map(|p| p.0).sum::<f64>() / 3.0, mirrored.iter().map(|p| p.1).sum::<f64>() / 3.0);
    let big: Vec<(f64, f64)> = mirrored.iter().map(|&(x, y)| (cx + (x - cx) * 1.12, cy + (y - cy) * 1.12)).collect();
    (part, stock(1, poly(&big)))
}

#[test]
fn a_part_that_fits_only_mirrored_stays_unplaced_until_mirroring_is_allowed() {
    let (p, s) = chiral();
    let forbidden = nest_with(
        std::slice::from_ref(&p),
        std::slice::from_ref(&s),
        &params(),
        &opts(MirrorPolicy::Forbidden),
        &never(),
    )
    .unwrap();
    assert!(forbidden.placements.is_empty());
    assert_eq!(forbidden.unplaced[0].reason, UnplacedReason::NoFit);

    let allowed = nest_with(
        std::slice::from_ref(&p),
        std::slice::from_ref(&s),
        &params(),
        &opts(MirrorPolicy::Allowed),
        &never(),
    )
    .unwrap();
    assert_eq!(allowed.placements.len(), 1, "{:?}", allowed.unplaced);
    assert!(allowed.placements[0].mirrored);
    assert_eq!(handedness(&allowed.placements[0].transform), -1.0, "the transform really is a mirror");
}

#[test]
fn plain_nest_never_mirrors() {
    let (p, s) = chiral();
    let r = burnburn_cam::nest::nest(std::slice::from_ref(&p), std::slice::from_ref(&s), &params(), &never()).unwrap();
    assert!(r.placements.is_empty());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// A better score is never a reason to mirror: with mirroring forbidden no placement is a mirror,
    /// whatever the shapes and however much a mirrored layout would have used of the sheet.
    #[test]
    fn better_score_never_violates_forbidden_mirroring(
        shapes in prop::collection::vec((5.0..30.0f64, 5.0..30.0f64, 2.0..20.0f64), 1..5),
        counts in prop::collection::vec(1u32..4, 1..5),
    ) {
        let parts: Vec<NestPart> = shapes
            .iter()
            .zip(counts.iter().cycle())
            .enumerate()
            .map(|(i, (&(w, h, skew), &count))| {
                part(i as u32 + 1, poly(&[(0.0, 0.0), (w, 0.0), (w * 0.5 + skew, h)]), count, every_15_degrees())
            })
            .collect();
        let sheet = stock(1, square(0.0, 0.0, 120.0, 90.0));
        let forbidden = nest_with(&parts, std::slice::from_ref(&sheet), &params(), &opts(MirrorPolicy::Forbidden), &never()).unwrap();
        let allowed = nest_with(&parts, std::slice::from_ref(&sheet), &params(), &opts(MirrorPolicy::Allowed), &never()).unwrap();
        for p in &forbidden.placements {
            prop_assert!(!p.mirrored);
            prop_assert_eq!(handedness(&p.transform), 1.0);
        }
        // Allowing it may use it; the invariant is about forbidding.
        let _ = allowed;
    }
}

#[test]
fn every_sheet_says_how_much_is_used_and_how_much_is_left() {
    // Two 100 x 100 sheets and five 40 x 40 parts: four fit on a sheet (4 x 1600 = 6400), one is left.
    let parts = [part(1, square(0.0, 0.0, 40.0, 40.0), 5, RotationSet::new(vec![0.0]).unwrap())];
    let sheets = [stock(1, square(0.0, 0.0, 100.0, 100.0)), stock(2, square(0.0, 0.0, 100.0, 100.0))];
    let r = nest_with(&parts, &sheets, &params(), &opts(MirrorPolicy::Forbidden), &never()).unwrap();
    assert!(r.unplaced.is_empty());
    let use_of = |id| r.stock_use.iter().find(|u| u.stock == StockId(id)).unwrap();
    // Four parts on the first sheet (fully used grid positions), the fifth on the second.
    assert!((use_of(1).placed_mm2 - 6400.0).abs() < 1e-6, "{:?}", r.stock_use);
    assert!((use_of(2).placed_mm2 - 1600.0).abs() < 1e-6);
    for u in &r.stock_use {
        assert!((u.net_mm2 - 10_000.0).abs() < 1e-6);
        assert!((u.free_mm2 - (u.net_mm2 - u.placed_mm2)).abs() < 1e-6, "free = net - placed");
    }
    let total: f64 = r.stock_use.iter().map(|u| u.placed_mm2).sum();
    assert!((total - r.utilisation.used_mm2).abs() < 1e-6, "the sheets add up to the utilisation");
}

#[test]
fn orders_and_complete_sets_are_counted_from_what_was_placed() {
    // A set is one big and two small parts; three sets ordered.
    let big = part(1, square(0.0, 0.0, 60.0, 60.0), 3, RotationSet::new(vec![0.0]).unwrap());
    let small = part(2, square(0.0, 0.0, 10.0, 10.0), 6, RotationSet::new(vec![0.0]).unwrap());
    let parts = [big, small];
    // Room for two big squares side by side (120 x 60) and a 15 mm strip above them for the small ones.
    let sheet = stock(1, square(0.0, 0.0, 125.0, 75.0));
    let r =
        nest_with(&parts, std::slice::from_ref(&sheet), &params(), &opts(MirrorPolicy::Forbidden), &never()).unwrap();
    let status = order_status(&r, &parts);
    let of = |key| status.iter().find(|s| s.part == PartKey(key)).unwrap();
    assert_eq!((of(1).ordered, of(1).placed), (3, 2));
    assert_eq!((of(2).ordered, of(2).placed), (6, 6));
    assert!(!status.iter().all(|s| s.ordered == s.placed), "the order is not complete");
    // A set needs 1 big and 2 small: two complete sets, the third lacks its big part.
    assert_eq!(complete_sets(&r, &[(PartKey(1), 1), (PartKey(2), 2)]), 2);
    // Nothing placed is no set; a part that is not in the result counts as none.
    assert_eq!(complete_sets(&r, &[(PartKey(9), 1)]), 0);
    assert_eq!(complete_sets(&r, &[]), 0);
}
