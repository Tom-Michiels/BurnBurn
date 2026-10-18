//! IM26: arrays and mixed-part nesting (C10). Assertions are geometric invariants checked on the
//! result (counts, allowed rotations, containment, spacing, forbidden zones, explicit
//! utilisation denominator), not the algorithm's internals.

#![allow(clippy::unwrap_used)]

use burnburn_cam::nest::{
    NestParams, NestPart, PartKey, RotationSet, Stock, StockId, UnplacedReason, UtilisationBasis, nest,
};
use burnburn_cam::{CircularArray, GridArray, circular_array, grid_array};
use burnburn_geometry::{Bounds, Path, PathBuilder, Point, ProductionTolerance, Rect};
use burnburn_types::{CancellationSource, CancellationToken, Length};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap())
}

/// 100 × 50 plate with a 20 × 20 hole (an earlier cut-out) at 40..60 × 15..35, grain along X.
fn plate() -> Stock {
    let outline = PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(100.0, 0.0))
        .line_to(Point::new(100.0, 50.0))
        .line_to(Point::new(0.0, 50.0))
        .close()
        .move_to(Point::new(40.0, 15.0))
        .line_to(Point::new(60.0, 15.0))
        .line_to(Point::new(60.0, 35.0))
        .line_to(Point::new(40.0, 35.0))
        .close()
        .build()
        .unwrap();
    Stock { id: StockId(1), outline, keep_out: vec![], grain_deg: Some(0.0) }
}

fn params() -> NestParams {
    NestParams {
        spacing: mm(2.0),
        edge_margin: mm(1.0),
        step: mm(1.0),
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        max_candidates: 10_000_000,
        basis: UtilisationBasis::StockArea,
    }
}

fn parts() -> Vec<NestPart> {
    vec![
        // Long strip whose grain must follow the plate: 90° and 270° are offered but forbidden.
        NestPart {
            key: PartKey(7),
            outline: rect(0.0, 0.0, 40.0, 10.0),
            count: 3,
            rotations: RotationSet::new(vec![0.0, 90.0, 180.0, 270.0]).unwrap(),
            grain_deg: Some(0.0),
        },
        // More squares than fit.
        NestPart {
            key: PartKey(9),
            outline: rect(0.0, 0.0, 15.0, 15.0),
            count: 12,
            rotations: RotationSet::new(vec![0.0]).unwrap(),
            grain_deg: None,
        },
    ]
}

/// Euclidean distance between two axis-aligned rectangles is at least `gap`, up to the
/// production tolerance (0.01 mm: round spacing corners are chord polygons).
fn separated(a: &Bounds, b: &Bounds, gap: f64) -> bool {
    let dx = (b.min().x - a.max().x).max(a.min().x - b.max().x).max(0.0);
    let dy = (b.min().y - a.max().y).max(a.min().y - b.max().y).max(0.0);
    dx.hypot(dy) >= gap - 0.011
}

#[test]
fn nesting_keeps_order_counts_and_respects_grain_rotation() {
    let parts = parts();
    let result = nest(&parts, &[plate()], &params(), &CancellationToken::never()).unwrap();

    for p in &parts {
        let placed = result.placements.iter().filter(|x| x.part == p.key).count() as u32;
        let unplaced: u32 = result.unplaced.iter().filter(|u| u.part == p.key).map(|u| u.count).sum();
        assert_eq!(placed + unplaced, p.count, "every ordered copy is placed or reported ({:?})", p.key);
    }
    let strips: Vec<_> = result.placements.iter().filter(|x| x.part == PartKey(7)).collect();
    assert_eq!(strips.len(), 3, "the strips fit and are all placed");
    assert!(strips.iter().all(|s| s.rotation_deg == 0.0 || s.rotation_deg == 180.0), "grain: never 90°/270°");
    let squares = result.placements.iter().filter(|x| x.part == PartKey(9)).count();
    assert!(squares < 12, "not every square fits");
    assert!(result.unplaced.iter().any(|u| u.part == PartKey(9) && u.reason == UnplacedReason::NoFit));

    // Geometry of the result: inside the margin, clear of the hole (+ margin) and spaced.
    let placed: Vec<Bounds> = result
        .placements
        .iter()
        .map(|pl| {
            let part = parts.iter().find(|p| p.key == pl.part).unwrap();
            part.outline.transform(&pl.transform).bounds().unwrap()
        })
        .collect();
    let hole = Bounds::from_corners(Point::new(40.0, 15.0), Point::new(60.0, 35.0));
    for (i, b) in placed.iter().enumerate() {
        assert!(b.min().x >= 1.0 - 1e-9 && b.min().y >= 1.0 - 1e-9, "{b:?}");
        assert!(b.max().x <= 99.0 + 1e-9 && b.max().y <= 49.0 + 1e-9, "{b:?}");
        assert!(separated(b, &hole, 1.0), "plate hole is a forbidden zone: {b:?}");
        for c in &placed[i + 1..] {
            assert!(separated(b, c, 2.0), "spacing between {b:?} and {c:?}");
        }
    }

    // Explicit denominator: net stock area 100·50 − 20·20 = 4600 mm².
    let u = result.utilisation;
    assert_eq!(u.basis, UtilisationBasis::StockArea);
    assert!((u.denominator_mm2 - 4600.0).abs() < 1e-6);
    let used = 3.0 * 400.0 + squares as f64 * 225.0;
    assert!((u.used_mm2 - used).abs() < 1e-6 && (u.value - used / 4600.0).abs() < 1e-12);

    let again = nest(&parts, &[plate()], &params(), &CancellationToken::never()).unwrap();
    assert_eq!(again, result, "deterministic");
}

#[test]
fn a_grain_that_no_allowed_rotation_can_follow_is_reported_not_rotated() {
    let mut p = parts();
    p[0].rotations = RotationSet::new(vec![90.0]).unwrap(); // grain 0° part, only 90° allowed
    p.truncate(1);
    let result = nest(&p, &[plate()], &params(), &CancellationToken::never()).unwrap();
    assert!(result.placements.is_empty());
    assert_eq!(result.unplaced.len(), 1);
    assert_eq!((result.unplaced[0].count, result.unplaced[0].reason), (3, UnplacedReason::NoAllowedRotation));
}

#[test]
fn bounding_box_basis_and_cancellation() {
    let mut p = parts();
    p.truncate(1);
    let mut q = params();
    q.basis = UtilisationBasis::UsedBoundingBox;
    let r = nest(&p, &[plate()], &q, &CancellationToken::never()).unwrap();
    let boxes = r
        .placements
        .iter()
        .map(|pl| p[0].outline.transform(&pl.transform).bounds().unwrap())
        .reduce(Bounds::union)
        .unwrap();
    assert!((r.utilisation.denominator_mm2 - boxes.width() * boxes.height()).abs() < 1e-6);

    let source = CancellationSource::new();
    source.cancel();
    assert!(nest(&p, &[plate()], &params(), &source.token()).is_err());
}

#[test]
fn grid_and_circular_arrays_generate_placements() {
    let g = grid_array(&GridArray { columns: 3, rows: 2, pitch_x: mm(10.0), pitch_y: mm(5.0) }).unwrap();
    let origins: Vec<(f64, f64)> = g
        .iter()
        .map(|t| {
            let p = t.apply(Point::ORIGIN);
            (p.x, p.y)
        })
        .collect();
    assert_eq!(origins, vec![(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (0.0, 5.0), (10.0, 5.0), (20.0, 5.0)]);

    // Four copies on a quarter-turn pattern about (10, 0): the point (20, 0) visits all four
    // quadrant points; with rotate_items the copies also turn.
    let c = circular_array(&CircularArray {
        count: 4,
        center: Point::new(10.0, 0.0),
        sweep_deg: 360.0,
        rotate_items: true,
        reference: Point::new(20.0, 0.0),
    })
    .unwrap();
    let pts: Vec<(f64, f64)> = c
        .iter()
        .map(|t| {
            let p = t.apply(Point::new(20.0, 0.0));
            (p.x, p.y)
        })
        .collect();
    let expected = [(20.0, 0.0), (10.0, 10.0), (0.0, 0.0), (10.0, -10.0)];
    for (a, b) in pts.iter().zip(expected) {
        assert!((a.0 - b.0).abs() < 1e-12 && (a.1 - b.1).abs() < 1e-12, "{pts:?}");
    }
    let translate_only = circular_array(&CircularArray {
        count: 4,
        center: Point::new(10.0, 0.0),
        sweep_deg: 360.0,
        rotate_items: false,
        reference: Point::new(20.0, 0.0),
    })
    .unwrap();
    let p1 = translate_only[1].apply(Point::new(20.0, 0.0));
    assert!((p1.x - 10.0).abs() < 1e-12 && (p1.y - 10.0).abs() < 1e-12, "reference moves around the centre");
    let v = translate_only[1].apply_vector(burnburn_geometry::Vector::new(1.0, 0.0));
    assert!((v.x - 1.0).abs() < 1e-12 && v.y.abs() < 1e-12, "copies keep their orientation");
    assert!(grid_array(&GridArray { columns: 0, rows: 2, pitch_x: mm(1.0), pitch_y: mm(1.0) }).is_err());
}

mod properties {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]
        /// Random rectangles: every result keeps counts, margins, the hole and the spacing.
        #[test]
        fn random_orders_never_overlap_and_keep_counts(
            dims in proptest::collection::vec((3.0f64..30.0, 3.0f64..20.0, 1u32..5, any::<bool>()), 1..5)
        ) {
            let parts: Vec<NestPart> = dims
                .iter()
                .enumerate()
                .map(|(i, &(w, h, count, turn))| NestPart {
                    key: PartKey(i as u32),
                    outline: rect(0.0, 0.0, w, h),
                    count,
                    rotations: RotationSet::new(if turn { vec![0.0, 90.0] } else { vec![0.0] }).unwrap(),
                    grain_deg: None,
                })
                .collect();
            let r = nest(&parts, &[plate()], &params(), &CancellationToken::never()).unwrap();
            let hole = Bounds::from_corners(Point::new(40.0, 15.0), Point::new(60.0, 35.0));
            let boxes: Vec<Bounds> = r.placements.iter().map(|pl| {
                parts[pl.part.0 as usize].outline.transform(&pl.transform).bounds().unwrap()
            }).collect();
            for p in &parts {
                let placed = r.placements.iter().filter(|x| x.part == p.key).count() as u32;
                let un: u32 = r.unplaced.iter().filter(|u| u.part == p.key).map(|u| u.count).sum();
                prop_assert_eq!(placed + un, p.count);
            }
            for (i, b) in boxes.iter().enumerate() {
                prop_assert!(b.min().x >= 1.0 - 1e-9 && b.min().y >= 1.0 - 1e-9 && b.max().x <= 99.0 + 1e-9 && b.max().y <= 49.0 + 1e-9);
                prop_assert!(separated(b, &hole, 1.0));
                for c in &boxes[i + 1..] {
                    prop_assert!(separated(b, c, 2.0), "{:?} {:?}", b, c);
                }
            }
        }
    }
}
