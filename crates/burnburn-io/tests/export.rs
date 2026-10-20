//! Public contract (IM10, P05): limited SVG/DXF export. Physical scale and closedness survive
//! a roundtrip through our own importer; approximations are named in the report.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_geometry::{Path, PathBuilder, Point, Rect};
use burnburn_io::export::{ExportError, ExportItem, ExportLoss, export_dxf, export_svg};
use burnburn_io::import::{UnitDecision, parse_dxf, parse_svg};
use burnburn_types::{Angle, Length};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn shapes() -> Vec<(&'static str, Path)> {
    let rect = Path::rectangle(Rect::new(Point::new(10.0, 5.0), mm(120.0), mm(80.0)).unwrap());
    let circle = Path::circle(Point::new(40.0, 40.0), 7.5).unwrap();
    let logo = PathBuilder::new()
        .move_to(Point::new(60.0, 20.0))
        .cubic_to(Point::new(60.0, 40.0), Point::new(90.0, 40.0), Point::new(90.0, 20.0))
        .line_to(Point::new(75.0, 10.0))
        .close()
        .build()
        .unwrap();
    let ellipse = Path::ellipse(Point::new(100.0, 60.0), 12.0, 5.0, Angle::from_degrees(30.0).unwrap()).unwrap();
    let cut = Path::polyline(&[Point::new(15.0, 70.0), Point::new(35.0, 72.0), Point::new(50.0, 70.0)], false).unwrap();
    vec![("plaat", rect), ("gat", circle), ("logo", logo), ("ovaal", ellipse), ("snede", cut)]
}

fn items<'a>(s: &'a [(&'static str, Path)]) -> Vec<ExportItem<'a>> {
    s.iter().map(|(n, p)| ExportItem::new(n, p)).collect()
}

/// Same closedness per subpath, bounds and area within `eps` (mm, mm²).
fn assert_same(a: &Path, b: &Path, eps: f64, what: &str) {
    let closed = |p: &Path| p.subpaths().iter().map(|s| s.is_closed()).collect::<Vec<_>>();
    assert_eq!(closed(a), closed(b), "{what}: closedness");
    let (ba, bb) = (a.bounds().unwrap(), b.bounds().unwrap());
    for (x, y) in
        [(ba.min().x, bb.min().x), (ba.min().y, bb.min().y), (ba.max().x, bb.max().x), (ba.max().y, bb.max().y)]
    {
        assert!((x - y).abs() <= eps, "{what}: bounds {ba:?} vs {bb:?}");
    }
    let (aa, ab) = (a.signed_area().abs(), b.signed_area().abs());
    assert!((aa - ab).abs() <= eps * (1.0 + aa.sqrt()) * 4.0, "{what}: area {aa} vs {ab}");
}

#[test]
fn svg_export_roundtrips_exactly_at_physical_scale() {
    let s = shapes();
    let (text, report) = export_svg(&items(&s)).unwrap();
    assert!(report.losses.is_empty(), "{:?}", report.losses);
    assert!(text.contains(r#"width="120mm""#) && text.contains(r#"height="80mm""#), "{text}");
    let draft = parse_svg(text.as_bytes()).unwrap();
    assert_eq!(
        draft.report.units,
        UnitDecision::Svg { mm_per_user_unit: 1.0, basis: burnburn_io::import::SvgUnitBasis::ViewBoxToPhysicalSize }
    );
    assert_eq!(draft.items.len(), s.len());
    // Export places the drawing's bounds at the origin; compare after undoing that shift.
    let all = s.iter().filter_map(|(_, p)| p.bounds()).reduce(burnburn_geometry::Bounds::union).unwrap();
    let back = burnburn_geometry::Affine::translate(all.min().to_vector()).unwrap();
    for ((name, original), imported) in s.iter().zip(&draft.items) {
        assert_eq!(&imported.entity, name);
        assert_same(original, &imported.path.transform(&back), 1e-9, name);
    }
}

#[test]
fn dxf_export_roundtrips_and_names_the_ellipse_approximation() {
    let s = shapes();
    let (text, report) = export_dxf(&items(&s)).unwrap();
    // Circular arcs stay exact (bulges); the rotated ellipse and the cubic become a spline,
    // the ellipse with a bounded deviation.
    let loss = report.losses.iter().find_map(|l| match l {
        ExportLoss::EllipticalArcAsSpline { item, max_deviation_mm } => Some((*item, *max_deviation_mm)),
        _ => None,
    });
    let (item, dev) = loss.unwrap();
    assert_eq!(item, 3);
    assert!(dev > 0.0 && dev < 0.01, "{dev}");
    assert_eq!(report.losses.len(), 1, "{:?}", report.losses);

    let dxf = parse_dxf(text.as_bytes()).unwrap();
    assert_eq!(dxf.declared_unit(), Some(4));
    let draft = dxf.to_draft(None).unwrap();
    assert!(draft.report.skipped.is_empty(), "{:?}", draft.report.skipped);
    assert_eq!(draft.items.len(), s.len());
    for ((name, original), imported) in s.iter().zip(&draft.items) {
        let eps = if *name == "ovaal" { dev + 1e-9 } else { 1e-9 };
        assert_eq!(&imported.name, name, "layer carries the object name");
        assert_same(original, &imported.path, eps, name);
    }
}

#[test]
fn empty_export_is_an_error() {
    assert_eq!(export_svg(&[]).unwrap_err(), ExportError::Empty);
    let empty = Path::default();
    assert_eq!(export_dxf(&[ExportItem::new("x", &empty)]).unwrap_err(), ExportError::Empty);
}
