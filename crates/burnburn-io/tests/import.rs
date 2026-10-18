//! Public contract (IM10, P03): SVG/DXF import at physical scale with an import report.
//! Fixtures live in `test-data/import/` (manifest there); every expectation is hand-derived
//! from the fixture text, never from a previous run.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use std::f64::consts::PI;
use std::path::PathBuf;

use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession, GeometrySource};
use burnburn_geometry::{Path, Point, Segment};
use burnburn_io::asset_hash;
use burnburn_io::import::{
    DxfScale, ImportError, ParseError, ParseLimits, SkipReason, SvgUnitBasis, UnitDecision, parse_dxf, parse_dxf_with,
    parse_svg, parse_svg_with,
};
use burnburn_types::{DocumentId, RequestId, StableId, Unit};

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/import").join(name);
    std::fs::read(p).unwrap()
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn item<'a>(items: &'a [burnburn_io::import::ImportedPath], entity: &str) -> &'a Path {
    &items.iter().find(|i| i.entity == entity).unwrap_or_else(|| panic!("no entity {entity}")).path
}

#[test]
fn unitless_dxf_needs_explicit_scale_before_import_commit() {
    let bytes = fixture("unitless-lines.dxf");
    let dxf = parse_dxf(&bytes).unwrap();
    assert_eq!(dxf.declared_unit(), None, "no $INSUNITS: unitless");
    let asset = asset_hash(&bytes);
    // No commit without an explicit scale.
    assert_eq!(dxf.import_command(asset, None).unwrap_err(), ImportError::ScaleRequired);
    // With inches the 4 × 2 drawing units become 101.6 × 50.8 mm, and the report says why.
    let draft = dxf.to_draft(Some(DxfScale::unit(Unit::Inch))).unwrap();
    assert_eq!(draft.report.units, UnitDecision::DxfExplicit { mm_per_unit: 25.4 });
    let cmd = dxf.import_command(asset, Some(DxfScale::unit(Unit::Inch))).unwrap();
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let out = s
        .apply(DocumentCommand {
            document: s.id(),
            expected: s.revision(),
            request: RequestId::from_raw(1),
            payload: CommandPayload::Edit(cmd),
        })
        .unwrap();
    assert_eq!(out.changes.created.len(), 2);
    let snap = s.snapshot();
    let second = snap.object(out.changes.created[1]).unwrap();
    assert_eq!(second.source(), &GeometrySource::Imported { asset, entity: Some("LINE#1B".into()) });
    let b = second.bounds().unwrap();
    assert!(close(b.min().x, 101.6, 1e-12) && close(b.max().y, 50.8, 1e-12), "{b:?}");
}

#[test]
fn declared_mm_dxf_imports_plate_holes_and_bulge_slot_and_reports_the_rest() {
    let dxf = parse_dxf(&fixture("mm-plate.dxf")).unwrap();
    assert_eq!(dxf.declared_unit(), Some(4));
    let draft = dxf.to_draft(None).unwrap();
    assert_eq!(draft.report.units, UnitDecision::DxfDeclared { insunits: 4, mm_per_unit: 1.0 });
    let plate = item(&draft.items, "LWPOLYLINE#2A");
    assert!(plate.subpaths()[0].is_closed() && close(plate.signed_area(), 5000.0, 1e-9));
    let hole = item(&draft.items, "CIRCLE#2B");
    assert!(close(hole.signed_area(), PI * 25.0, 1e-9));
    let b = hole.bounds().unwrap();
    assert!(close(b.min().x, 20.0, 1e-12) && close(b.max().y, 30.0, 1e-12), "{b:?}");
    // Two bulge-1 half circles closing back: a full circle of diameter 10 around (65, 40).
    let slot = item(&draft.items, "LWPOLYLINE#2D");
    assert!(close(slot.signed_area(), PI * 25.0, 1e-9), "{}", slot.signed_area());
    let sb = slot.bounds().unwrap();
    assert!(close(sb.min().y, 35.0, 1e-9) && close(sb.max().y, 45.0, 1e-9), "{sb:?}");
    // The upper half ARC (0°..180° counter-clockwise) is open and peaks at y = 30.
    let arc = item(&draft.items, "ARC#2C");
    assert!(close(arc.bounds().unwrap().max().y, 30.0, 1e-12));
    assert!(
        draft
            .report
            .open_contours
            .iter()
            .any(|o| draft.items[o.item].entity == "ARC#2C" && close(o.gap_mm, 10.0, 1e-12))
    );
    // TEXT is reported, not silently dropped.
    let text = draft.report.skipped.iter().find(|s| s.entity == "TEXT#2E").unwrap();
    assert_eq!(text.reason, SkipReason::UnsupportedEntity);
    // Layer names are kept as item names.
    assert_eq!(draft.items.iter().find(|i| i.entity == "LWPOLYLINE#2A").unwrap().name, "Plaat");
}

/// Independent oracle: de Boor evaluation of the fixture's B-spline (inches).
fn de_boor(u: f64) -> Point {
    let knots = [0.0, 0.0, 0.0, 0.0, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0];
    let ctrl = [(0.0, 0.0), (1.0, 2.0), (2.0, -1.0), (3.0, 2.0), (4.0, 0.0), (5.0, 1.0)];
    let p = 3;
    let k = (p..knots.len() - p - 1).rfind(|&i| knots[i] <= u).unwrap_or(p).min(ctrl.len() - 1);
    let mut d: Vec<(f64, f64)> = (0..=p).map(|j| ctrl[j + k - p]).collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = j + k - p;
            let a = (u - knots[i]) / (knots[i + p + 1 - r] - knots[i]);
            d[j] = ((1.0 - a) * d[j - 1].0 + a * d[j].0, (1.0 - a) * d[j - 1].1 + a * d[j].1);
        }
    }
    Point::new(d[p].0 * 25.4, d[p].1 * 25.4)
}

#[test]
fn inch_spline_becomes_exact_beziers_and_ellipse_keeps_its_area() {
    let draft = parse_dxf(&fixture("inch-spline.dxf")).unwrap().to_draft(None).unwrap();
    let spline = item(&draft.items, "SPLINE#3A");
    let segs = spline.subpaths()[0].segments();
    assert_eq!(segs.len(), 3, "interior knots 0.5 and 0.75 → three Bézier spans");
    assert!(segs.iter().all(|s| matches!(s, Segment::Cubic { .. })));
    // Span boundaries are the curve points at the knots; mid-span points match de Boor.
    let sp = &spline.subpaths()[0];
    assert!(sp.start().distance(de_boor(0.0)) < 1e-9);
    assert!(segs[0].end().distance(de_boor(0.5)) < 1e-9 && segs[1].end().distance(de_boor(0.75)) < 1e-9);
    let mid = segs[1].point_at(segs[0].end(), 0.5);
    assert!(mid.distance(de_boor(0.625)) < 1e-9, "{mid:?} vs {:?}", de_boor(0.625));
    let ellipse = item(&draft.items, "ELLIPSE#3B");
    assert!(close(ellipse.signed_area(), 2.0 * PI * 25.4 * 25.4, 1e-6), "{}", ellipse.signed_area());
}

#[test]
fn svg_viewbox_and_mm_size_give_physical_scale() {
    let draft = parse_svg(&fixture("mm-viewbox.svg")).unwrap();
    assert_eq!(
        draft.report.units,
        UnitDecision::Svg { mm_per_user_unit: 0.1, basis: SvgUnitBasis::ViewBoxToPhysicalSize }
    );
    let plate = item(&draft.items, "plate");
    let b = plate.bounds().unwrap();
    assert!(close(b.width(), 100.0, 1e-9) && close(b.height(), 50.0, 1e-9) && close(b.min().y, 0.0, 1e-9), "{b:?}");
    // The file draws the rect visually clockwise; the y-flip keeps the visual winding, so the
    // import keeps the file's orientation (fill rules are even-odd anyway).
    assert!(close(plate.signed_area(), -5000.0, 1e-9), "{}", plate.signed_area());
    let hole = item(&draft.items, "hole");
    let hb = hole.bounds().unwrap();
    assert!(close(hb.center().x, 25.0, 1e-9) && close(hb.center().y, 25.0, 1e-9), "{hb:?}");
    // Stadium: 10 × 10 mm rectangle (x 60..70, y_down 10..20) plus two r = 5 mm half circles → x 55..75.
    let slot = item(&draft.items, "slot");
    assert!(close(slot.signed_area().abs(), 100.0 + PI * 25.0, 1e-9), "{}", slot.signed_area());
    let sb = slot.bounds().unwrap();
    assert!(close(sb.min().x, 55.0, 1e-9) && close(sb.max().x, 75.0, 1e-9) && close(sb.max().y, 40.0, 1e-9), "{sb:?}");
    let skipped: Vec<(&str, &SkipReason)> =
        draft.report.skipped.iter().map(|s| (s.entity.as_str(), &s.reason)).collect();
    assert!(skipped.contains(&("label", &SkipReason::UnsupportedElement)), "{skipped:?}");
    assert!(skipped.contains(&("hidden", &SkipReason::Hidden)), "{skipped:?}");
}

#[test]
fn svg_without_size_assumes_css_pixels_and_applies_nested_transforms() {
    let draft = parse_svg(&fixture("px-transforms.svg")).unwrap();
    let k = 25.4 / 96.0;
    assert_eq!(draft.report.units, UnitDecision::Svg { mm_per_user_unit: k, basis: SvgUnitBasis::AssumedCssPixels });
    // rotate(90 10 10) then scale(2): (0,0) → (40,0), (10,0) → (40,20), (10,5) → (30,20); y-up with height 96 px.
    let tick = item(&draft.items, "tick");
    let sp = &tick.subpaths()[0];
    let pts: Vec<Point> = std::iter::once(sp.start()).chain(sp.segments().iter().map(Segment::end)).collect();
    let want = [(40.0, 0.0), (40.0, 20.0), (30.0, 20.0)].map(|(x, y)| Point::new(x * k, (96.0 - y) * k));
    for (a, b) in pts.iter().zip(want) {
        assert!(a.distance(b) < 1e-9, "{a:?} vs {b:?}");
    }
    assert!(draft.report.open_contours.iter().any(|o| draft.items[o.item].entity == "tick"));
    // Smooth cubic reflects the previous control point (20,80) about (30,90) → (40,100).
    let curves = item(&draft.items, "curves");
    let Segment::Cubic { c1, .. } = curves.subpaths()[0].segments()[1] else { panic!("cubic") };
    assert!(c1.distance(Point::new(40.0 * k, (96.0 - 100.0) * k)) < 1e-9, "{c1:?}");
    // Quadratics are elevated exactly: Q (60,90) (70,80) (80,90) → c1 = P0 + ⅔(Q − P0).
    let Segment::Cubic { c1: q1, to: q_end, .. } = curves.subpaths()[0].segments()[2] else { panic!("cubic") };
    assert!(q1.distance(Point::new((60.0 + 20.0 / 3.0) * k, (96.0 - (90.0 - 20.0 / 3.0)) * k)) < 1e-9);
    assert!(q_end.distance(Point::new(80.0 * k, 6.0 * k)) < 1e-9);
    let last = curves.subpaths()[0].end();
    assert!(last.distance(Point::new(95.0 * k, 6.0 * k)) < 1e-9);
    // clip-path is an effect we do not apply: geometry imported, effect reported.
    assert!(draft.items.iter().any(|i| i.entity == "diag"));
    assert!(
        draft
            .report
            .skipped
            .iter()
            .any(|s| s.entity == "diag" && s.reason == SkipReason::UnsupportedEffect("clip-path"))
    );
}

#[test]
fn svg_inch_size_and_arc_flags_follow_the_spec() {
    // 2 in wide, viewBox 0..200: 0.254 mm per unit. From (0,100) to (100,0) with r = 100 the
    // centres are (0,0) and (100,100); large-arc = 1 with sweep = 1 (positive angles, y-down)
    // selects the 270° arc around (0,0): 90° → 360°.
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2in" height="2in" viewBox="0 0 200 200">
        <path id="a" d="M0,100 A100,100 0 1,1 100,0"/></svg>"#;
    let draft = parse_svg(svg).unwrap();
    assert_eq!(
        draft.report.units,
        UnitDecision::Svg { mm_per_user_unit: 0.254, basis: SvgUnitBasis::ViewBoxToPhysicalSize }
    );
    let a = item(&draft.items, "a");
    let b = a.bounds().unwrap();
    // Three quarters around (0,0): x −100..100, y_down −100..100 → 50.8 × 50.8 mm, centred on
    // the viewBox origin, which maps to (0, 50.8) y-up.
    assert!(close(b.width(), 50.8, 1e-9) && close(b.height(), 50.8, 1e-9), "{b:?}");
    assert!(close(b.center().x, 0.0, 1e-9) && close(b.center().y, 50.8, 1e-9), "{b:?}");
}

#[test]
fn parser_limits_are_typed_errors() {
    let limits = ParseLimits { max_bytes: 64, ..ParseLimits::default() };
    let big = fixture("mm-viewbox.svg");
    assert!(matches!(parse_svg_with(&big, &limits), Err(ParseError::TooLarge { limit: 64, .. })));
    let deep = format!("<svg xmlns='http://www.w3.org/2000/svg'>{}{}</svg>", "<g>".repeat(100), "</g>".repeat(100));
    let r = parse_svg_with(deep.as_bytes(), &ParseLimits { max_depth: 32, ..ParseLimits::default() });
    assert_eq!(r.unwrap_err(), ParseError::TooDeep { limit: 32 });
    let many =
        format!("<svg xmlns='http://www.w3.org/2000/svg'>{}</svg>", "<line x1='0' y1='0' x2='1' y2='1'/>".repeat(50));
    let r = parse_svg_with(many.as_bytes(), &ParseLimits { max_entities: 10, ..ParseLimits::default() });
    assert_eq!(r.unwrap_err(), ParseError::TooManyEntities { limit: 10 });
    let dxf = fixture("mm-plate.dxf");
    let r = parse_dxf_with(&dxf, &ParseLimits { max_entities: 2, ..ParseLimits::default() });
    assert_eq!(r.unwrap_err(), ParseError::TooManyEntities { limit: 2 });
    assert!(matches!(
        parse_dxf(b"0\nSECTION\n2\nENTITIES\n0\nLINE\n10\nnot-a-number\n0\nEOF\n"),
        Err(ParseError::Malformed { .. })
    ));
    assert!(matches!(parse_svg(b"<svg><path d='M 0 0 L"), Err(ParseError::Malformed { .. })));
    let mismatched = parse_svg(b"<svg xmlns='http://www.w3.org/2000/svg'><g></h></svg>");
    assert!(matches!(mismatched, Err(ParseError::Malformed { .. })), "{mismatched:?}");
    assert!(matches!(parse_svg(b"\xff\xfe<svg/>"), Err(ParseError::NotUtf8)));
    assert!(matches!(parse_dxf(b"AutoCAD Binary DXF\r\n\x1a\x00"), Err(ParseError::Unsupported(_))));
}

#[test]
fn entity_expansion_is_never_performed() {
    // Billion-laughs style input: custom entities are not expanded; the reference is an error.
    let svg = br#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY a "aaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;">]>
        <svg xmlns="http://www.w3.org/2000/svg"><path id="&b;" d="M0 0 L1 1"/></svg>"#;
    assert!(matches!(parse_svg(svg), Err(ParseError::Malformed { .. })));
}
