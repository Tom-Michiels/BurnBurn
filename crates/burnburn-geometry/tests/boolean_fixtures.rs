//! Runs the hand-derived boolean cases in `test-data/geometry/boolean-cases.txt` (manifest there).

#![allow(clippy::unwrap_used, clippy::panic)] // fixture parsing in tests

use burnburn_geometry::{
    BooleanOp, BooleanOptions, FillRule, Path, Point, ProductionTolerance, Segment, Subpath, boolean,
};
use burnburn_types::{CancellationToken, Length};

fn contour(line: &str) -> Subpath {
    let pts: Vec<Point> = line
        .split_whitespace()
        .skip(1)
        .map(|xy| {
            let (x, y) = xy.split_once(',').unwrap();
            Point::new(x.parse().unwrap(), y.parse().unwrap())
        })
        .collect();
    Subpath::new(pts[0], pts[1..].iter().map(|p| Segment::Line { to: *p }).collect(), true).unwrap()
}

#[test]
fn boolean_fixture_cases_match_hand_derived_areas() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/geometry/boolean-cases.txt");
    let text = std::fs::read_to_string(file).unwrap();
    let mut cases: Vec<(String, BooleanOp, f64, Vec<Subpath>, Vec<Subpath>)> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("case") => {
                let name = words.next().unwrap().to_owned();
                let op = match words.next().unwrap() {
                    "union" => BooleanOp::Union,
                    "difference" => BooleanOp::Difference,
                    "intersection" => BooleanOp::Intersection,
                    "xor" => BooleanOp::Xor,
                    other => panic!("unknown op {other}"),
                };
                let area: f64 = words.next().unwrap().parse().unwrap();
                cases.push((name, op, area, Vec::new(), Vec::new()));
            }
            Some("a") => cases.last_mut().unwrap().3.push(contour(line)),
            Some("b") => cases.last_mut().unwrap().4.push(contour(line)),
            other => panic!("unexpected line start {other:?}"),
        }
    }
    assert_eq!(cases.len(), 9);
    let opts = BooleanOptions {
        fill: FillRule::EvenOdd,
        tolerance: ProductionTolerance::new(Length::from_mm(0.001).unwrap()).unwrap(),
    };
    for (name, op, area, a, b) in cases {
        let r =
            boolean(&Path::from_subpaths(a), &Path::from_subpaths(b), op, &opts, &CancellationToken::never()).unwrap();
        assert!((r.signed_area() - area).abs() < 1e-6, "{name} {op:?}: {} vs {area}", r.signed_area());
    }
}
