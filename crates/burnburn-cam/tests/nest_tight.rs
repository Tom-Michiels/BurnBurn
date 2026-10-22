//! Tight packing and holes (C10/C12, G7-cam). The raster of the nest is conservative: a part
//! occupies whole cells, so neighbours end up up to one step apart. `Packing::Tight` slides every
//! accepted placement against its neighbours with exact geometry, and only ever accepts a position
//! that the exact check confirms. Oracles are analytic (rectangles: distances between boxes).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_cam::nest::{
    NestOptions, NestParams, NestPart, NestResult, NestError, Packing, PartKey, RotationSet, Stock, StockId,
    UtilisationBasis, nest_packed,
};
use burnburn_geometry::{Bounds, Path, Point, ProductionTolerance, Rect};
use burnburn_types::{CancellationSource, CancellationToken, Length};
use proptest::prelude::*;

/// The exact checks let two parts overlap by at most a tenth of the production tolerance (0.01 mm).
const TOL: f64 = 5e-3;
/// The spacing is kept around round corners flattened at the production tolerance (0.01 mm), plus
/// the slack of the exact checks.
const CORNER_TOL: f64 = 0.012;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn square(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap())
}

fn params(spacing: f64, margin: f64, step: f64) -> NestParams {
    NestParams {
        spacing: mm(spacing),
        edge_margin: mm(margin),
        step: mm(step),
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        max_candidates: u64::MAX,
        basis: UtilisationBasis::StockArea,
    }
}

fn part(key: u32, outline: Path, count: u32, quarter_turns: bool) -> NestPart {
    let rotations = RotationSet::new(if quarter_turns { vec![0.0, 90.0] } else { vec![0.0] }).unwrap();
    NestPart { key: PartKey(key), outline, count, rotations, grain_deg: None }
}

fn stock(outline: Path) -> Stock {
    Stock { id: StockId(1), outline, keep_out: vec![], grain_deg: None }
}

fn run(parts: &[NestPart], s: &Stock, p: &NestParams, packing: Packing) -> NestResult {
    nest_packed(parts, std::slice::from_ref(s), p, &NestOptions::default(), packing, &CancellationToken::never())
        .unwrap()
}

/// Where each placement put its part: the part's bounds in stock coordinates.
fn boxes(parts: &[NestPart], r: &NestResult) -> Vec<Bounds> {
    r.placements
        .iter()
        .map(|pl| {
            let outline = &parts.iter().find(|p| p.key == pl.part).unwrap().outline;
            outline.transform(&pl.transform).bounds().unwrap()
        })
        .collect()
}

/// Euclidean distance between two axis-aligned boxes (0 when they touch or overlap).
fn distance(a: &Bounds, b: &Bounds) -> f64 {
    let dx = (b.min().x - a.max().x).max(a.min().x - b.max().x).max(0.0);
    let dy = (b.min().y - a.max().y).max(a.min().y - b.max().y).max(0.0);
    dx.hypot(dy)
}

#[test]
fn tight_packing_closes_the_gap_a_whole_cell_leaves() {
    // Two 10.4 x 10 parts on a 21 x 10 sheet: 20.8 mm of parts fit, but the raster gives each part
    // 11 whole cells (1 mm step) and only one fits.
    let parts = [part(1, square(0.0, 0.0, 10.4, 10.0), 2, false)];
    let sheet = stock(square(0.0, 0.0, 21.0, 10.0));
    let p = params(0.0, 0.0, 1.0);
    let grid = run(&parts, &sheet, &p, Packing::Grid);
    assert_eq!(grid.placements.len(), 1, "the premise: the raster alone places one");

    let tight = run(&parts, &sheet, &p, Packing::Tight);
    assert_eq!(tight.placements.len(), 2, "{:?}", tight.unplaced);
    let mut xs: Vec<f64> = boxes(&parts, &tight).iter().map(|b| b.min().x).collect();
    xs.sort_by(f64::total_cmp);
    assert!(xs[0].abs() < TOL && (xs[1] - 10.4).abs() < TOL, "{xs:?}");
    assert!(tight.utilisation.value > grid.utilisation.value);
}

#[test]
fn tight_packing_fits_a_part_exactly_into_what_is_left_of_a_row() {
    // 20.8 mm of sheet and two 10.4 mm parts: the second one fits to the thousandth, which no
    // position on a grid of 1 mm (or 0.25 mm) proposes.
    let parts = [part(1, square(0.0, 0.0, 10.4, 10.0), 2, false)];
    let sheet = stock(square(0.0, 0.0, 20.8, 10.0));
    let r = run(&parts, &sheet, &params(0.0, 0.0, 1.0), Packing::Tight);
    assert_eq!(r.placements.len(), 2, "{:?}", r.unplaced);
}

#[test]
fn tight_packing_keeps_the_spacing_exactly() {
    // 10 x 10 parts, 1.5 mm apart on a 50 x 10 sheet: x = 0, 11.5, 23, 34.5 and no fifth (46 + 10 > 50).
    let parts = [part(1, square(0.0, 0.0, 10.0, 10.0), 5, false)];
    let sheet = stock(square(0.0, 0.0, 50.0, 10.0));
    let r = run(&parts, &sheet, &params(1.5, 0.0, 1.0), Packing::Tight);
    assert_eq!(r.placements.len(), 4, "{:?}", r.unplaced);
    let mut xs: Vec<f64> = boxes(&parts, &r).iter().map(|b| b.min().x).collect();
    xs.sort_by(f64::total_cmp);
    for (x, want) in xs.iter().zip([0.0, 11.5, 23.0, 34.5]) {
        assert!((x - want).abs() < TOL, "{xs:?}");
    }
}

#[test]
fn a_part_goes_into_the_hole_of_another_part() {
    // A 100 x 100 plate with a 60 x 60 hole fills the whole sheet; four 25 x 25 parts can only
    // go into the hole.
    let hole = Path::from_subpaths(
        [square(0.0, 0.0, 100.0, 100.0), square(20.0, 20.0, 60.0, 60.0)]
            .iter()
            .flat_map(|p| p.subpaths().iter().cloned())
            .collect(),
    );
    let parts = [part(1, hole, 1, false), part(2, square(0.0, 0.0, 25.0, 25.0), 4, false)];
    let sheet = stock(square(0.0, 0.0, 100.0, 100.0));
    for packing in [Packing::Grid, Packing::Tight] {
        let r = run(&parts, &sheet, &params(1.0, 0.0, 1.0), packing);
        assert_eq!(r.placements.len(), 5, "{packing:?}: {:?}", r.unplaced);
        let inside = Bounds::from_corners(Point::new(20.0, 20.0), Point::new(80.0, 80.0));
        for (pl, b) in r.placements.iter().zip(boxes(&parts, &r)) {
            if pl.part == PartKey(2) {
                assert!(
                    b.min().x >= inside.min().x - TOL
                        && b.min().y >= inside.min().y - TOL
                        && b.max().x <= inside.max().x + TOL
                        && b.max().y <= inside.max().y + TOL,
                    "{packing:?}: a small part sits outside the hole: {b:?}"
                );
            }
        }
    }
}

#[test]
fn tight_packing_respects_a_keep_out_zone_and_the_edge_margin() {
    let mut sheet = stock(square(0.0, 0.0, 60.0, 20.0));
    sheet.keep_out = vec![square(25.0, 0.0, 10.0, 20.0)];
    let parts = [part(1, square(0.0, 0.0, 12.0, 12.0), 3, false)];
    let r = run(&parts, &sheet, &params(1.0, 2.0, 1.0), Packing::Tight);
    let keep = Bounds::from_corners(Point::new(25.0, 0.0), Point::new(35.0, 20.0));
    for b in boxes(&parts, &r) {
        assert!(distance(&b, &keep) >= 2.0 - TOL, "{b:?}");
        assert!(b.min().x >= 2.0 - TOL && b.min().y >= 2.0 - TOL && b.max().x <= 58.0 + TOL && b.max().y <= 18.0 + TOL);
    }
    assert_eq!(r.placements.len(), 2, "a 12 mm part fits twice on each side of the zone only once: {:?}", r.unplaced);
}

#[test]
fn a_cancelled_tight_nest_returns_cancelled() {
    let source = CancellationSource::new();
    source.cancel();
    let parts = [part(1, square(0.0, 0.0, 10.0, 10.0), 3, false)];
    let sheet = stock(square(0.0, 0.0, 50.0, 10.0));
    let r = nest_packed(
        &parts,
        std::slice::from_ref(&sheet),
        &params(0.0, 0.0, 1.0),
        &NestOptions::default(),
        Packing::Tight,
        &source.token(),
    );
    assert!(matches!(r, Err(NestError::Cancelled)), "{:?}", r.map(|_| ()));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    /// Whatever the rectangles, counts, spacing and margin: no two parts are closer than the
    /// spacing, every part is inside the sheet minus the margin, the counts are never exceeded,
    /// and the same input gives the same result twice. For the raster and for tight packing.
    #[test]
    fn placements_never_collide_stay_inside_and_are_deterministic(
        sizes in prop::collection::vec((4.0f64..25.0, 4.0f64..25.0, 1u32..4), 1..5),
        spacing in 0.0f64..3.0,
        margin in 0.0f64..2.0,
        quarter_turns in any::<bool>(),
    ) {
        let parts: Vec<NestPart> = sizes
            .iter()
            .enumerate()
            .map(|(i, &(w, h, n))| part(i as u32 + 1, square(0.0, 0.0, w, h), n, quarter_turns))
            .collect();
        let sheet = stock(square(0.0, 0.0, 70.0, 50.0));
        let p = params(spacing, margin, 1.0);
        for packing in [Packing::Grid, Packing::Tight] {
            let a = run(&parts, &sheet, &p, packing);
            let b = run(&parts, &sheet, &p, packing);
            prop_assert_eq!(&a, &b, "{:?} is deterministic", packing);
            let bs = boxes(&parts, &a);
            for (i, x) in bs.iter().enumerate() {
                prop_assert!(
                    x.min().x >= margin - TOL && x.min().y >= margin - TOL
                        && x.max().x <= 70.0 - margin + TOL && x.max().y <= 50.0 - margin + TOL,
                    "{:?}: outside the margin: {:?}", packing, x
                );
                for y in &bs[i + 1..] {
                    prop_assert!(distance(x, y) >= spacing - CORNER_TOL, "{:?}: {:?} and {:?} are closer than {}", packing, x, y, spacing);
                }
            }
            for (i, &(_, _, n)) in sizes.iter().enumerate() {
                let placed = a.placements.iter().filter(|pl| pl.part == PartKey(i as u32 + 1)).count();
                prop_assert!(placed <= n as usize);
            }
        }
    }
}
