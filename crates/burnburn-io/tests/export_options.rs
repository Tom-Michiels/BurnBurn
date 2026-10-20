//! W2-L6, export options (P05): units, styling, layers and DXF curve policy for SVG/DXF, and the
//! preflight that names lost constraints/modifiers *before* anything is written. Roundtrips use
//! our own importers; the DXF layer table is read back with plain string scanning (independent
//! of the exporter).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use burnburn_geometry::{Path, PathBuilder, Point, Rect};
use burnburn_io::export::{
    DxfCurves, DxfLayers, DxfOptions, DxfUnit, ExportError, ExportItem, ExportLoss, LostSemantics, SvgOptions, SvgUnit,
    export_dxf, export_dxf_with, export_svg, export_svg_with, preflight_dxf, preflight_svg,
};
use burnburn_io::import::{UnitDecision, parse_dxf, parse_svg};
use burnburn_types::Length;

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn plate() -> Path {
    Path::rectangle(
        Rect::new(Point::new(10.0, 5.0), Length::from_mm(50.8).unwrap(), Length::from_mm(25.4).unwrap()).unwrap(),
    )
}

fn logo() -> Path {
    PathBuilder::new()
        .move_to(Point::new(60.0, 20.0))
        .cubic_to(Point::new(60.0, 40.0), Point::new(90.0, 40.0), Point::new(90.0, 20.0))
        .line_to(Point::new(75.0, 10.0))
        .close()
        .build()
        .unwrap()
}

#[test]
fn svg_inch_and_pixel_units_keep_the_physical_size() {
    let (rect, logo) = (plate(), logo());
    let items = [ExportItem::new("plaat", &rect), ExportItem::new("logo", &logo)];
    let (mm_text, _) = export_svg(&items).unwrap();
    let reference = parse_svg(mm_text.as_bytes()).unwrap();
    let ref_area = reference.items[0].path.signed_area().abs();
    assert!(close(ref_area, 50.8 * 25.4, 1e-9));

    let (inch_text, _) = export_svg_with(&items, &SvgOptions { unit: SvgUnit::Inch, ..SvgOptions::default() }).unwrap();
    // The drawing spans x 10..90 mm = 80 mm = 3.149606… in.
    let width_in: f64 = inch_text.split("width=\"").nth(1).unwrap().split("in\"").next().unwrap().parse().unwrap();
    assert!(close(width_in, 80.0 / 25.4, 1e-9), "{width_in}");
    let inch = parse_svg(inch_text.as_bytes()).unwrap();
    assert!(
        matches!(inch.report.units, UnitDecision::Svg { mm_per_user_unit, .. } if close(mm_per_user_unit, 25.4, 1e-12))
    );
    assert!(close(inch.items[0].path.signed_area().abs(), ref_area, 1e-6), "{}", inch.items[0].path.signed_area());

    let (px_text, _) = export_svg_with(&items, &SvgOptions { unit: SvgUnit::Pixel, ..SvgOptions::default() }).unwrap();
    assert!(px_text.contains("px\""), "{px_text}");
    let px = parse_svg(px_text.as_bytes()).unwrap();
    assert!(close(px.items[0].path.signed_area().abs(), ref_area, 1e-6));
    let (b_ref, b_px) = (reference.items[1].path.bounds().unwrap(), px.items[1].path.bounds().unwrap());
    assert!(close(b_ref.max().x, b_px.max().x, 1e-6) && close(b_ref.max().y, b_px.max().y, 1e-6));
}

#[test]
fn svg_strokes_colours_and_layer_groups_follow_the_options() {
    let (rect, logo) = (plate(), logo());
    let items = [
        ExportItem::new("plaat", &rect).with_layer("Snijden").with_color([255, 0, 0]),
        ExportItem::new("logo", &logo).with_layer("Graveren").with_color([0, 0, 255]),
        ExportItem::new("rand", &rect).with_layer("Snijden"),
    ];
    let (text, _) = export_svg_with(&items, &SvgOptions { group_layers: true, ..SvgOptions::default() }).unwrap();
    assert!(text.contains(r##"stroke="#ff0000""##) && text.contains(r##"stroke="#0000ff""##), "{text}");
    assert!(text.contains(r##"stroke="#000000""##), "an item without colour uses the default");
    assert!(text.contains(r#"fill="none""#) && text.contains(r#"stroke-width="0.1""#), "{text}");
    // Two layers, in order of first appearance; "rand" joins "Snijden".
    let first = text.find(r#"<g id="Snijden">"#).unwrap();
    let second = text.find(r#"<g id="Graveren">"#).unwrap();
    assert!(first < second && text.matches("<g id=").count() == 2, "{text}");
    assert!(text.matches("<path").count() == 3);
    assert_eq!(parse_svg(text.as_bytes()).unwrap().items.len(), 3, "our importer reads grouped output");
    // Colours off: everything is the default colour.
    let (plain, _) = export_svg_with(&items, &SvgOptions { use_item_colors: false, ..SvgOptions::default() }).unwrap();
    assert!(!plain.contains("#ff0000") && !plain.contains("<g "));
}

#[test]
fn svg_thumbnail_size_is_in_pixels_and_the_view_box_keeps_the_proportions() {
    let (rect, logo) = (plate(), logo());
    let items = [ExportItem::new("plaat", &rect), ExportItem::new("logo", &logo)];
    let (text, _) =
        export_svg_with(&items, &SvgOptions { fit_px: Some((64.0, 64.0)), ..SvgOptions::default() }).unwrap();
    assert!(text.contains(r#"width="64px""#) && text.contains(r#"height="64px""#), "{text}");
    // Drawing bounds: x 10..90 → 80 mm wide; the height is that of the union of both shapes.
    let bounds = rect.bounds().unwrap().union(logo.bounds().unwrap());
    assert!(close(bounds.width(), 80.0, 1e-9));
    assert!(text.contains(&format!("viewBox=\"0 0 80 {}\"", bounds.height())), "{text}");
}

#[test]
fn svg_decimals_round_coordinates() {
    let wavy = PathBuilder::new()
        .move_to(Point::new(1.0 / 3.0, 2.0 / 3.0))
        .line_to(Point::new(10.123456, 20.987654))
        .build()
        .unwrap();
    let (text, _) =
        export_svg_with(&[ExportItem::new("w", &wavy)], &SvgOptions { decimals: Some(2), ..SvgOptions::default() })
            .unwrap();
    let d = text.split(" d=\"").nth(1).unwrap().split('"').next().unwrap();
    for number in d.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-')).filter(|t| !t.is_empty()) {
        let decimals = number.split('.').nth(1).map_or(0, str::len);
        assert!(decimals <= 2, "{number} in {d}");
    }
    let back = parse_svg(text.as_bytes()).unwrap();
    let b = back.items[0].path.bounds().unwrap();
    assert!(close(b.width(), 9.79, 0.011), "{}", b.width());
}

/// Layer names and ACI colours as written in the LAYER table, found by plain scanning.
fn layer_table(dxf: &str) -> Vec<(String, i32)> {
    let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        if lines[i] == "0" && lines[i + 1] == "LAYER" {
            let (mut name, mut color) = (String::new(), 0);
            let mut j = i + 2;
            while j + 1 < lines.len() && !(lines[j] == "0") {
                match lines[j] {
                    "2" => name = lines[j + 1].to_owned(),
                    "62" => color = lines[j + 1].parse().unwrap(),
                    _ => {}
                }
                j += 2;
            }
            out.push((name, color));
            i = j;
        } else {
            i += 2;
        }
    }
    out
}

#[test]
fn dxf_inch_units_roundtrip_through_the_declared_unit() {
    let rect = plate();
    let (text, _) = export_dxf_with(
        &[ExportItem::new("plaat", &rect)],
        &DxfOptions { unit: DxfUnit::Inch, ..DxfOptions::default() },
    )
    .unwrap();
    let dxf = parse_dxf(text.as_bytes()).unwrap();
    assert_eq!(dxf.declared_unit(), Some(1));
    let draft = dxf.to_draft(None).unwrap();
    assert_eq!(draft.report.units, UnitDecision::DxfDeclared { insunits: 1, mm_per_unit: 25.4 });
    assert!(close(draft.items[0].path.signed_area().abs(), 50.8 * 25.4, 1e-6));
    // The numbers in the file are inches: the plate's right edge is at (10 + 50.8) / 25.4 = 2.3937… in.
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let max_x =
        lines.windows(2).filter(|w| w[0] == "10").map(|w| w[1].parse::<f64>().unwrap()).fold(f64::MIN, f64::max);
    assert!(close(max_x, 60.8 / 25.4, 1e-12), "{max_x}");
    assert!(close(draft.items[0].path.bounds().unwrap().width(), 50.8, 1e-9));
}

#[test]
fn dxf_layers_follow_the_naming_policy_and_the_layer_table_carries_colours() {
    let (rect, logo) = (plate(), logo());
    let items = [
        ExportItem::new("plaat", &rect).with_layer("Snijden").with_color([255, 0, 0]),
        ExportItem::new("logo", &logo).with_layer("Graveren").with_color([0, 0, 255]),
        ExportItem::new("rand", &rect).with_layer("Snijden").with_color([255, 0, 0]),
    ];
    let by_layer = DxfOptions { layers: DxfLayers::ItemLayerOrName, ..DxfOptions::default() };
    let (text, report) = export_dxf_with(&items, &by_layer).unwrap();
    assert_eq!(
        layer_table(&text),
        [("Snijden".to_owned(), 1), ("Graveren".to_owned(), 5)],
        "first appearance, ACI 1 = red, 5 = blue"
    );
    assert!(
        report.losses.iter().all(|l| !matches!(l, ExportLoss::ColorApproximated { .. })),
        "pure primaries are exact"
    );
    // Our importer names items after their layer.
    let names: Vec<String> =
        parse_dxf(text.as_bytes()).unwrap().to_draft(None).unwrap().items.iter().map(|i| i.name.clone()).collect();
    assert_eq!(names, ["Snijden", "Graveren", "Snijden"]);
    // Other policies.
    let (by_name, _) = export_dxf_with(&items, &DxfOptions::default()).unwrap();
    assert_eq!(layer_table(&by_name).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["plaat", "logo", "rand"]);
    let (single, _) =
        export_dxf_with(&items, &DxfOptions { layers: DxfLayers::Single, ..DxfOptions::default() }).unwrap();
    assert_eq!(layer_table(&single).iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["0"]);
    // Colours off: every layer gets the default index 7.
    let (plain, _) = export_dxf_with(&items, &DxfOptions { use_item_colors: false, ..by_layer }).unwrap();
    assert!(layer_table(&plain).iter().all(|(_, c)| *c == 7));
}

#[test]
fn colours_without_an_exact_autocad_index_are_mapped_and_reported() {
    let rect = plate();
    let items =
        [ExportItem::new("a", &rect).with_color([200, 30, 30]), ExportItem::new("b", &rect).with_color([0, 255, 0])];
    let (text, report) = export_dxf_with(&items, &DxfOptions::default()).unwrap();
    assert_eq!(layer_table(&text), [("a".to_owned(), 1), ("b".to_owned(), 3)]);
    assert_eq!(report.losses, [ExportLoss::ColorApproximated { item: 0 }]);
}

#[test]
fn dxf_curves_can_be_written_as_polylines_within_a_tolerance() {
    let logo = logo();
    let items = [ExportItem::new("logo", &logo)];
    let (spline, _) = export_dxf(&items).unwrap();
    assert!(spline.contains("SPLINE"));
    let tol = 0.01;
    let (text, report) = export_dxf_with(
        &items,
        &DxfOptions { curves: DxfCurves::Polylines { tolerance_mm: tol }, ..DxfOptions::default() },
    )
    .unwrap();
    assert!(!text.contains("SPLINE") && text.contains("LWPOLYLINE"), "{text}");
    let [ExportLoss::CurvesFlattened { item: 0, max_deviation_mm }] = report.losses[..] else {
        panic!("expected one CurvesFlattened loss, got {:?}", report.losses)
    };
    assert!(max_deviation_mm <= tol && max_deviation_mm > 0.0, "{max_deviation_mm}");
    let back = parse_dxf(text.as_bytes()).unwrap().to_draft(None).unwrap();
    assert!(back.items[0].path.subpaths()[0].is_closed());
    // The chords lie within `tol` of the curve, so the area differs by at most perimeter × tol.
    let perimeter = 50.0 + 2.0 * 20.0; // generous bound for the logo outline
    assert!(close(back.items[0].path.signed_area().abs(), logo.signed_area().abs(), perimeter * tol));
    // A non-positive tolerance is refused instead of looping or exploding.
    assert_eq!(
        export_dxf_with(
            &items,
            &DxfOptions { curves: DxfCurves::Polylines { tolerance_mm: 0.0 }, ..DxfOptions::default() }
        )
        .unwrap_err(),
        ExportError::InvalidOption("curve tolerance must be positive"),
    );
}

/// P05: `export_lists_lost_constraints_before_writing`.
#[test]
fn export_lists_lost_constraints_before_writing() {
    let (rect, logo) = (plate(), logo());
    let items = [
        ExportItem::new("plaat", &rect),
        ExportItem::new("schets", &logo).with_lost(LostSemantics { constraints: 3, ..LostSemantics::default() }),
        ExportItem::new("rij", &rect).with_lost(LostSemantics {
            modifiers: 2,
            parametric: true,
            ..LostSemantics::default()
        }),
        ExportItem::new("tekst", &rect).with_lost(LostSemantics { text_outlined: true, ..LostSemantics::default() }),
    ];
    let expected_semantic =
        [ExportLoss::ConstraintsDropped { item: 1, count: 3 }, ExportLoss::ModifiersApplied { item: 2, count: 2 }];
    // The preflight names everything without producing a file.
    let svg = preflight_svg(&items, &SvgOptions::default()).unwrap();
    assert_eq!(
        svg,
        [
            expected_semantic[0],
            expected_semantic[1],
            ExportLoss::ParametricFlattened { item: 2 },
            ExportLoss::TextAsOutlines { item: 3 },
        ]
    );
    assert!(svg.iter().filter(|l| l.needs_acknowledgement()).count() == 2);
    // Exporting without acknowledgement writes nothing and returns the same list.
    assert_eq!(export_svg_with(&items, &SvgOptions::default()), Err(ExportError::LossesNotAcknowledged(svg.clone())));
    let dxf_pre = preflight_dxf(&items, &DxfOptions::default()).unwrap();
    assert_eq!(
        export_dxf_with(&items, &DxfOptions::default()),
        Err(ExportError::LossesNotAcknowledged(dxf_pre.clone()))
    );
    // Acknowledged: the file is written and the report is the preflight.
    let (text, report) =
        export_svg_with(&items, &SvgOptions { acknowledge_lost_semantics: true, ..SvgOptions::default() }).unwrap();
    assert!(text.contains("<svg") && report.losses == svg);
    let (dxf, report) =
        export_dxf_with(&items, &DxfOptions { acknowledge_lost_semantics: true, ..DxfOptions::default() }).unwrap();
    assert!(dxf.contains("ENTITIES") && report.losses == dxf_pre);
    assert!(dxf_pre.starts_with(&expected_semantic), "{dxf_pre:?}");
    // Parametric shapes and text alone are informational: no acknowledgement needed.
    let info_only =
        [ExportItem::new("t", &rect).with_lost(LostSemantics { text_outlined: true, ..LostSemantics::default() })];
    assert!(export_svg_with(&info_only, &SvgOptions::default()).is_ok());
    // Plain items: nothing lost, nothing to acknowledge, the legacy entry points keep working.
    assert!(preflight_svg(&[ExportItem::new("p", &rect)], &SvgOptions::default()).unwrap().is_empty());
    assert!(export_svg(&[ExportItem::new("p", &rect)]).is_ok());
}
