//! Public contract: units convert to canonical millimetres and bounds are exact.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_geometry::{Path, Point, Rect};
use burnburn_types::{Length, Unit};

const EPS_MM: f64 = 1e-12;

#[test]
fn inch_rectangle_has_expected_millimeter_bounds() {
    // 2 in x 1 in at (1 in, 0.5 in): by definition 1 in = 25.4 mm.
    let width = Length::new(2.0, Unit::Inch).unwrap();
    let height = Length::new(1.0, Unit::Inch).unwrap();
    let origin = Point::new(Length::new(1.0, Unit::Inch).unwrap().mm(), Length::new(0.5, Unit::Inch).unwrap().mm());
    let path = Path::rectangle(Rect::new(origin, width, height).unwrap());

    let b = path.bounds().unwrap();
    assert!((b.min().x - 25.4).abs() < EPS_MM, "{b:?}");
    assert!((b.min().y - 12.7).abs() < EPS_MM, "{b:?}");
    assert!((b.width() - 50.8).abs() < EPS_MM, "{b:?}");
    assert!((b.height() - 25.4).abs() < EPS_MM, "{b:?}");
    assert!((width.to_unit(Unit::Inch) - 2.0).abs() < EPS_MM);
}
