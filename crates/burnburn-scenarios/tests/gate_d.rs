//! Integration gate D (docs/user-flows/gate-d.md): four product chains, each ending in an RD
//! file that is decoded with `burnburn-ruida` and checked semantically (geometry, layer
//! settings), never against an earlier run.
//!
//! - (A) montageplaat as a part with two variants, nested on two plates → one JobPlan per plate.
//! - (B) finger-joint box generator; the material thickness changes the slots, kerf (CAM) is
//!   applied exactly once.
//! - (C) customer logo: SVG import → close the gap → boolean union → fill + line.
//! - (D) handwriting: centerline of the corpus letters T, X, Y → explicit junction choice and
//!   confirmed primitive replacement → line operation.
//!
//! Expectations are derived by hand from the drawing parameters (sizes, radii, corpus JSON).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use burnburn_cam::nest::{NestParams, NestPart, RotationSet, Stock, StockId, UtilisationBasis, nest};
use burnburn_cam::preflight;
use burnburn_document::engine::{EngineRecipe, ParamValue};
use burnburn_document::generator::{GeneratorCommand, GeneratorDefinition};
use burnburn_document::process::{
    FillSettings, Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand,
    ProcessSettings, ScanAxis, ScanDirection, Speed,
};
use burnburn_document::structure::{
    Dim, NamedShape, PartDefinition, PartInstance, PartShape, Plate, PlateId, StructureChange, StructureCommand,
};
use burnburn_document::trace::{TraceCommand, TraceDefinition, TraceOutput};
use burnburn_document::{CommandPayload, DocumentSnapshot, GeometryCommand, ObjectKind};
use burnburn_generators::{GENERATOR_ENGINE, GeneratorEvaluator};
use burnburn_geometry::{
    Affine, BooleanOp, BooleanOptions, DiagnosticTolerance, FillRule, Path, Point, ProductionTolerance, Rect,
};
use burnburn_image::{DecodeLimits, ImageRecipe, PixelScale, Threshold, decode as decode_png};
use burnburn_io::asset_hash;
use burnburn_io::import::parse_svg;
use burnburn_ruida::{Command, PowerBound, RdFile};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_trace::{CENTERLINE_ENGINE_VERSION, CenterlineRequest, CenterlineSettings, Connections, trace_centerline};
use burnburn_types::{CancellationToken, GeometryId, Length, RevisionId, StableId, TaskId};
use common::*;

// ---------------------------------------------------------------------------------------------
// RD reading per layer (part)
// ---------------------------------------------------------------------------------------------

/// Laser-on segments per RD part (layer), relative moves resolved to absolute machine mm.
fn cuts_by_part(file: &RdFile) -> BTreeMap<u8, Vec<Cut>> {
    let mut at = (0i64, 0i64);
    let mut part = 0u8;
    let mut out: BTreeMap<u8, Vec<Cut>> = BTreeMap::new();
    let pt = |p: (i64, i64)| Point::new(p.0 as f64 / 1000.0, p.1 as f64 / 1000.0);
    for c in file.commands() {
        let (to, cut) = match *c {
            Command::LayerNumberPart(p) => {
                part = p.get();
                continue;
            }
            Command::MoveAbs { x, y } => ((x.um() as i64, y.um() as i64), false),
            Command::MoveRel { dx, dy } => ((at.0 + i64::from(dx.um()), at.1 + i64::from(dy.um())), false),
            Command::MoveRelX(d) => ((at.0 + i64::from(d.um()), at.1), false),
            Command::MoveRelY(d) => ((at.0, at.1 + i64::from(d.um())), false),
            Command::CutAbs { x, y } => ((x.um() as i64, y.um() as i64), true),
            Command::CutRel { dx, dy } => ((at.0 + i64::from(dx.um()), at.1 + i64::from(dy.um())), true),
            Command::CutRelX(d) => ((at.0 + i64::from(d.um()), at.1), true),
            Command::CutRelY(d) => ((at.0, at.1 + i64::from(d.um())), true),
            _ => continue,
        };
        if cut {
            out.entry(part).or_default().push(Cut { from: pt(at), to: pt(to) });
        }
        at = to;
    }
    out
}

/// Header settings of one part: (speed µm/s, max power raw, work mode).
fn part_settings(file: &RdFile, part: u8) -> (u64, u16, u8) {
    let mut speed = None;
    let mut power = None;
    let mut mode = None;
    for c in file.commands() {
        match *c {
            Command::PartSpeed { part: p, speed: s } if p.get() == part => speed = Some(s.um_per_s()),
            Command::PartPower { bound: PowerBound::Max, part: p, value, .. } if p.get() == part => {
                power = Some(value.raw());
            }
            Command::WorkModePart { part: p, mode: m } if p.get() == part => mode = Some(m.get()),
            _ => {}
        }
    }
    (speed.expect("part speed"), power.expect("part power"), mode.expect("work mode"))
}

fn bbox(points: &[Point]) -> (Point, Point) {
    let mut lo = Point::new(f64::INFINITY, f64::INFINITY);
    let mut hi = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in points {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    (lo, hi)
}

fn size(points: &[Point]) -> (f64, f64) {
    let (lo, hi) = bbox(points);
    (hi.x - lo.x, hi.y - lo.y)
}

/// Centre of the bounding box (a polyline's vertex mean is biased by its closing point).
fn mid(points: &[Point]) -> Point {
    let (lo, hi) = bbox(points);
    Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0)
}

fn closed(l: &[Point]) -> bool {
    l.len() > 2 && l[0].distance(*l.last().unwrap()) < 1e-9
}

/// Shoelace area of a closed loop (absolute).
fn area(l: &[Point]) -> f64 {
    l.windows(2).map(|w| w[0].x * w[1].y - w[1].x * w[0].y).sum::<f64>().abs() / 2.0
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn op(name: &str, mode: OperationMode, speed: f64, power: f64) -> OperationDraft {
    let p = PowerPercent::new(power).unwrap();
    OperationDraft {
        name: name.into(),
        mode,
        settings: ProcessSettings {
            speed: Speed::mm_per_s(speed).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

fn add_op(s: &mut burnburn_document::DocumentSession, draft: OperationDraft, targets: Vec<GeometryId>) {
    apply(s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets }), &[]);
}

/// Compile → preflight without errors → RD bytes → decoded file.
fn rd_of(snap: &DocumentSnapshot) -> (burnburn_job::JobPlan, RdFile) {
    let plan = compile_plan(snap);
    let report = preflight(snap, &plan, &profile());
    assert!(!report.has_errors(), "{:?}", report);
    let file = decode(&rd_bytes(&plan));
    (plan, file)
}

// ---------------------------------------------------------------------------------------------
// (A) montageplaat: part with variants → nesting on two plates → one JobPlan per plate
// ---------------------------------------------------------------------------------------------

const PLATE_W: f64 = 200.0;
const PLATE_H: f64 = 100.0;
const SPACING: f64 = 2.0;
const EDGE: f64 = 1.0;
/// (width, height) of the variants "groot" (default values) and "klein".
const GROOT: (f64, f64) = (120.0, 80.0);
const KLEIN: (f64, f64) = (60.0, 40.0);

/// Plate `breedte × hoogte` with four Ø4 holes 10 mm from the edges. The holes are placed by
/// the parameters `rechts`/`boven` (= size − 10), because a `Dim` names one parameter.
fn montageplaat() -> PartDefinition {
    let mm = |v| Length::from_mm(v).unwrap();
    let p = |n: &str| Dim::Param(n.into());
    let mut shapes = BTreeMap::new();
    shapes.insert(
        1,
        NamedShape {
            name: "omtrek".into(),
            shape: PartShape::Rectangle { x: Dim::Mm(0.0), y: Dim::Mm(0.0), width: p("breedte"), height: p("hoogte") },
        },
    );
    for (k, (x, y)) in [
        (Dim::Mm(10.0), Dim::Mm(10.0)),
        (p("rechts"), Dim::Mm(10.0)),
        (p("rechts"), p("boven")),
        (Dim::Mm(10.0), p("boven")),
    ]
    .into_iter()
    .enumerate()
    {
        shapes.insert(
            2 + k as u32,
            NamedShape { name: format!("gat {k}"), shape: PartShape::Circle { x, y, radius: Dim::Mm(2.0) } },
        );
    }
    let values = |w: f64, h: f64| {
        BTreeMap::from([
            ("breedte".to_owned(), mm(w)),
            ("hoogte".to_owned(), mm(h)),
            ("rechts".to_owned(), mm(w - 10.0)),
            ("boven".to_owned(), mm(h - 10.0)),
        ])
    };
    PartDefinition {
        name: "Montageplaat".into(),
        parameters: values(GROOT.0, GROOT.1),
        shapes,
        variants: BTreeMap::from([
            ("groot".to_owned(), values(GROOT.0, GROOT.1)),
            ("klein".to_owned(), values(KLEIN.0, KLEIN.1)),
        ]),
    }
}

fn rect_path(w: f64, h: f64) -> Path {
    let mm = |v| Length::from_mm(v).unwrap();
    Path::rectangle(Rect::new(Point::new(0.0, 0.0), mm(w), mm(h)).unwrap())
}

#[test]
fn headless_gate_d_mounting_plate_variants_nested_on_two_plates_give_one_rd_per_plate() {
    let mut s = new_session();
    let out =
        apply(&mut s, CommandPayload::Structure(StructureCommand::DefinePart { definition: montageplaat() }), &[]);
    let StructureChange::Definition(definition) = out.changes.structure[0] else { panic!("definition") };
    let mut plates: Vec<PlateId> = Vec::new();
    for n in 1..=2 {
        let plate = Plate {
            name: format!("Plaat {n}"),
            width: mm(PLATE_W),
            height: mm(PLATE_H),
            material: "MDF".into(),
            thickness: Some(mm(3.0)),
        };
        let out = apply(&mut s, CommandPayload::Structure(StructureCommand::CreatePlate { plate }), &[]);
        let StructureChange::Plate(p) = out.changes.structure[0] else { panic!("plate") };
        plates.push(p);
    }

    // Order: 2 × groot, 3 × klein. By hand: one 120 × 80 fits per 200 × 100 plate (two would
    // need 242 mm or 162 mm), and two 60 × 40 fit next to it (1 + 120 + 2 + 60 ≤ 199,
    // 1 + 40 + 2 + 40 ≤ 99), so all five fit on the two plates.
    let variants = [("groot", GROOT, 2), ("klein", KLEIN, 3)];
    let parts: Vec<NestPart> = variants
        .iter()
        .enumerate()
        .map(|(i, (_, (w, h), count))| NestPart {
            key: burnburn_cam::nest::PartKey(i as u32),
            outline: rect_path(*w, *h),
            count: *count,
            rotations: RotationSet::new(vec![0.0]).unwrap(),
            grain_deg: None,
        })
        .collect();
    let stock: Vec<Stock> = (0..2)
        .map(|i| Stock { id: StockId(i), outline: rect_path(PLATE_W, PLATE_H), keep_out: vec![], grain_deg: None })
        .collect();
    let params = NestParams {
        spacing: mm(SPACING),
        edge_margin: mm(EDGE),
        step: mm(1.0),
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        max_candidates: 10_000_000,
        basis: UtilisationBasis::StockArea,
    };
    let result = nest(&parts, &stock, &params, &CancellationToken::never()).unwrap();
    assert!(result.unplaced.is_empty(), "{:?}", result.unplaced);
    assert_eq!(result.placements.len(), 5);
    let used = 2.0 * GROOT.0 * GROOT.1 + 3.0 * KLEIN.0 * KLEIN.1;
    assert!(near(result.utilisation.value, used / (2.0 * PLATE_W * PLATE_H), 1e-9), "{:?}", result.utilisation);

    // Apply: every placement becomes an instance of its variant on the plate of its stock.
    let mut on_plate: BTreeMap<PlateId, Vec<(&str, Vec<GeometryId>)>> = BTreeMap::new();
    for pl in &result.placements {
        let (variant, ..) = variants[pl.part.0 as usize];
        let plate = plates[pl.stock.0 as usize];
        let instance = PartInstance {
            definition,
            variant: Some(variant.into()),
            overrides: BTreeMap::new(),
            transform: pl.transform,
            plate: Some(plate),
        };
        let out = apply(&mut s, CommandPayload::Structure(StructureCommand::PlaceInstance { instance }), &[]);
        assert_eq!(out.changes.created.len(), 5, "outline + 4 holes");
        on_plate.entry(plate).or_default().push((variant, out.changes.created));
    }
    for (plate, items) in &on_plate {
        let targets = items.iter().flat_map(|(_, ids)| ids.iter().copied()).collect();
        add_op(
            &mut s,
            op(&format!("Snijden {plate:?}"), OperationMode::Line { kerf: Kerf::NONE }, 20.0, 60.0),
            targets,
        );
    }
    let snap = s.snapshot();

    let mut plans = Vec::new();
    let mut counted = BTreeMap::new();
    for plate in &plates {
        let view = snap.plate_view(*plate).unwrap();
        assert!(view.issues.is_empty(), "{:?}", view.issues);
        let (plan, file) = rd_of(&view.snapshot);
        plans.push(plan.id());
        let items = &on_plate[plate];
        let loops: Vec<Vec<Point>> = cuts_by_part(&file).into_values().flat_map(|c| loops(&c)).collect();
        assert_eq!(loops.len(), 5 * items.len(), "every outline and hole of this plate, nothing of the other");
        let outlines: Vec<_> = loops.iter().filter(|l| size(l).0 > 10.0).collect();
        let holes: Vec<_> = loops.iter().filter(|l| near(size(l).0, 4.0, 0.01) && near(size(l).1, 4.0, 0.01)).collect();
        assert_eq!((outlines.len(), holes.len()), (items.len(), 4 * items.len()));
        for l in &loops {
            assert!(closed(l), "closed cut loop");
            let (lo, hi) = bbox(l);
            assert!(lo.x >= EDGE - 1e-6 && lo.y >= EDGE - 1e-6, "inside the edge margin: {lo:?}");
            assert!(hi.x <= PLATE_W - EDGE + 1e-6 && hi.y <= PLATE_H - EDGE + 1e-6, "inside the plate: {hi:?}");
        }
        for (i, a) in outlines.iter().enumerate() {
            let (alo, ahi) = bbox(a);
            for b in &outlines[i + 1..] {
                let (blo, bhi) = bbox(b);
                let gap = (blo.x - ahi.x).max(alo.x - bhi.x).max(blo.y - ahi.y).max(alo.y - bhi.y);
                assert!(gap >= SPACING - 1e-6, "parts keep the spacing: {gap}");
            }
        }
        for o in &outlines {
            let (w, h) = size(o);
            let variant = if near(w, GROOT.0, 1e-6) && near(h, GROOT.1, 1e-6) {
                "groot"
            } else {
                assert!(near(w, KLEIN.0, 1e-6) && near(h, KLEIN.1, 1e-6), "{w} × {h}");
                "klein"
            };
            *counted.entry(variant).or_insert(0) += 1;
            // Its four holes sit 10 mm inside its corners.
            let (lo, hi) = bbox(o);
            for c in [
                (lo.x + 10.0, lo.y + 10.0),
                (hi.x - 10.0, lo.y + 10.0),
                (hi.x - 10.0, hi.y - 10.0),
                (lo.x + 10.0, hi.y - 10.0),
            ] {
                let centre = Point::new(c.0, c.1);
                assert!(holes.iter().any(|h| mid(h).distance(centre) < 0.01), "hole at {centre:?}");
            }
        }
        assert_eq!(part_settings(&file, 0), (20_000, 9830, 0), "line layer: 20 mm/s, 60 %");
    }
    assert_eq!(counted, BTreeMap::from([("groot", 2), ("klein", 3)]), "the order counts over both plates");
    assert_ne!(plans[0], plans[1], "one JobPlan per plate");
}

// ---------------------------------------------------------------------------------------------
// (B) finger-joint box: thickness change, kerf once
// ---------------------------------------------------------------------------------------------

const KERF: f64 = 0.2;

fn box_definition(thickness: f64) -> GeneratorDefinition {
    GeneratorDefinition {
        name: "Doos".into(),
        generator: "finger-box".into(),
        recipe: EngineRecipe::new(GENERATOR_ENGINE)
            .with("width", ParamValue::Number(100.0))
            .with("depth", ParamValue::Number(60.0))
            .with("height", ParamValue::Number(40.0))
            .with("thickness", ParamValue::Number(thickness))
            .with("finger_width", ParamValue::Number(10.0))
            .with("lid", ParamValue::Text("closed".into()))
            .with("spacing", ParamValue::Number(5.0)),
        // The flat layout starts at the origin; kerf grows the outline by 0.1 mm, so place it
        // 10 mm into the bed.
        placement: Affine::translate(burnburn_geometry::Vector::new(10.0, 10.0)).unwrap(),
    }
}

/// Outer sizes of the six panels (bottom, top, front, back, left, right) — the box's outer
/// dimensions, independent of the material thickness.
const PANELS: [(f64, f64); 6] =
    [(100.0, 60.0), (100.0, 60.0), (100.0, 40.0), (100.0, 40.0), (60.0, 40.0), (60.0, 40.0)];

/// Depth of the finger/slot steps along the bottom edge of a loop: distinct heights (above the
/// loop's lowest point) of its horizontal cut segments in the lower band.
fn bottom_steps(l: &[Point], band: f64) -> Vec<f64> {
    let (lo, _) = bbox(l);
    let mut ys: Vec<f64> = l
        .windows(2)
        .filter(|w| (w[0].y - w[1].y).abs() < 1e-6 && (w[0].x - w[1].x).abs() > 0.5)
        .map(|w| ((w[0].y - lo.y) * 1000.0).round() / 1000.0)
        .filter(|y| *y <= band)
        .collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    ys
}

fn box_rd(s: &burnburn_document::DocumentSession) -> Vec<Vec<Point>> {
    let (_, file) = rd_of(&s.snapshot());
    assert_eq!(part_settings(&file, 0), (15_000, 11468, 0), "line layer: 15 mm/s, 70 %");
    let loops: Vec<Vec<Point>> = cuts_by_part(&file).into_values().flat_map(|c| loops(&c)).collect();
    assert_eq!(loops.len(), 6, "six closed panels");
    loops
}

fn check_panels(loops: &[Vec<Point>], thickness: f64) {
    let round = |v: f64| (v * 1000.0).round() / 1000.0;
    let mut sizes: Vec<(f64, f64)> = loops.iter().map(|l| size(l)).map(|(w, h)| (round(w), round(h))).collect();
    sizes.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut want: Vec<(f64, f64)> = PANELS.iter().map(|(w, h)| (w + KERF, h + KERF)).collect();
    want.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    for (got, want) in sizes.iter().zip(&want) {
        assert!(near(got.0, want.0, 1e-3) && near(got.1, want.1, 1e-3), "kerf once: {got:?} vs {want:?}");
    }
    // Bottom/top panels start every edge with a slot: along the bottom edge the outline steps
    // between the outer edge (0) and the slot floor (one material thickness in).
    for l in loops.iter().filter(|l| near(size(l).1, 60.0 + KERF, 1e-3)) {
        assert!(closed(l));
        let steps = bottom_steps(l, thickness + 1.0);
        assert_eq!(steps, vec![0.0, thickness], "slot depth = material thickness");
    }
}

#[test]
fn headless_gate_d_box_thickness_change_no_double_kerf() {
    let mut s = new_session();
    let create = GeneratorCommand::Create { definition: Box::new(box_definition(3.0)) };
    let out = apply(&mut s, CommandPayload::Generator(create), &[&GeneratorEvaluator]);
    let panels = out.changes.created.clone();
    assert_eq!(panels.len(), 6);
    let kerf = Kerf::width(mm(KERF)).unwrap();
    add_op(&mut s, op("Snijden", OperationMode::Line { kerf }, 15.0, 70.0), panels.clone());
    check_panels(&box_rd(&s), 3.0);

    let generator = s.snapshot().engine_layer().generators().next().map(|(id, _)| id).expect("generator");
    let set = GeneratorCommand::Set { generator, definition: Box::new(box_definition(6.0)) };
    let out = apply(&mut s, CommandPayload::Generator(set), &[&GeneratorEvaluator]);
    let mut modified = out.changes.modified.clone();
    modified.sort();
    let mut expected = panels.clone();
    expected.sort();
    assert_eq!(modified, expected, "same panels (and so the same operation targets) after the change");
    check_panels(&box_rd(&s), 6.0);
}

// ---------------------------------------------------------------------------------------------
// (C) customer logo: SVG → repair → boolean → fill + line
// ---------------------------------------------------------------------------------------------

/// Ring r 10 around (20, 20) mm drawn as two arcs whose end stops 0.03 mm short of the start
/// (end point on the circle, 0.003 rad before it), and a 25 × 10 bar
/// from (20, 15) overlapping it. SVG user units are mm (viewBox = size), y down from 100.
const LOGO: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
  <path id="ring" d="M 30 80 A 10 10 0 0 1 10 80 A 10 10 0 0 1 29.99996 79.97"/>
  <rect id="balk" x="20" y="75" width="25" height="10"/>
</svg>"#;

const RING: (f64, f64, f64) = (20.0, 20.0, 10.0);
const BAR: (f64, f64, f64, f64) = (20.0, 15.0, 45.0, 25.0);

fn in_logo(p: Point, tol: f64) -> bool {
    Point::new(RING.0, RING.1).distance(p) <= RING.2 + tol
        || (p.x >= BAR.0 - tol && p.x <= BAR.2 + tol && p.y >= BAR.1 - tol && p.y <= BAR.3 + tol)
}

#[test]
fn headless_gate_d_customer_logo_repaired_union_with_fill_and_line() {
    let draft = parse_svg(LOGO.as_bytes()).unwrap();
    assert_eq!(draft.report.open_contours.len(), 1, "the ring does not close: {:?}", draft.report.open_contours);
    assert!(near(draft.report.open_contours[0].gap_mm, 0.03, 0.005));
    let mut s = new_session();
    let created = apply(&mut s, CommandPayload::Edit(draft.into_command(asset_hash(LOGO.as_bytes())).unwrap()), &[])
        .changes
        .created;
    assert_eq!(created.len(), 2);

    let tolerance = DiagnosticTolerance::new(mm(0.05)).unwrap();
    apply(&mut s, CommandPayload::Edit(GeometryCommand::CloseGaps { targets: created.clone(), tolerance }), &[]);
    let options = BooleanOptions { fill: FillRule::NonZero, tolerance: ProductionTolerance::new(mm(0.001)).unwrap() };
    let union = GeometryCommand::Boolean { op: BooleanOp::Union, targets: created, options, keep_sources: false };
    let logo = apply(&mut s, CommandPayload::Edit(union), &[]).changes.created[0];

    let fill = FillSettings::new(mm(0.5), ScanAxis::X, ScanDirection::Bidirectional, mm(0.0)).unwrap();
    add_op(&mut s, op("Vullen", OperationMode::Fill(fill), 300.0, 30.0), vec![logo]);
    add_op(&mut s, op("Contour", OperationMode::Line { kerf: Kerf::NONE }, 20.0, 60.0), vec![logo]);
    let (_, file) = rd_of(&s.snapshot());
    // 30 % → round(0.3 × 16383) = 4915; 60 % → 9830.
    assert_eq!(part_settings(&file, 0), (300_000, 4915, 2), "fill layer: scan mode");
    assert_eq!(part_settings(&file, 1), (20_000, 9830, 0), "line layer");
    let parts = cuts_by_part(&file);

    // Fill: horizontal rows 0.5 mm apart, every row inside the union and ending on its boundary.
    let rows = &parts[&0];
    let mut ys: Vec<i64> = rows.iter().map(|c| (c.from.y * 1000.0).round() as i64).collect();
    ys.sort();
    ys.dedup();
    assert!((39..=41).contains(&ys.len()), "20 mm high at 0.5 mm: {} rows", ys.len());
    assert!(ys.windows(2).all(|w| w[1] - w[0] == 500), "row pitch 0.5 mm");
    for c in rows {
        assert!((c.from.y - c.to.y).abs() < 1e-9, "scan rows are horizontal");
        let mid = Point::new((c.from.x + c.to.x) / 2.0, c.from.y);
        assert!(in_logo(mid, 0.01) && in_logo(c.from, 0.02) && in_logo(c.to, 0.02), "row {c:?} inside the logo");
    }

    // Line: one closed outline of the union (no inner ring edge left from the overlap).
    let outline = loops(&parts[&1]);
    assert_eq!(outline.len(), 1, "the union is one contour");
    let l = &outline[0];
    assert!(closed(l));
    let (lo, hi) = bbox(l);
    assert!(lo.distance(Point::new(10.0, 10.0)) < 0.01 && hi.distance(Point::new(45.0, 30.0)) < 0.01, "{lo:?} {hi:?}");
    // Union area by hand: πr² + bar − overlap; overlap = ∫_{−5}^{5} √(100 − y²) dy
    // = 5·√75 + 100·asin(0.5) = 95.661 mm².
    let overlap = 5.0 * 75f64.sqrt() + 100.0 * 0.5f64.asin();
    let expected = std::f64::consts::PI * 100.0 + 250.0 - overlap;
    assert!(near(area(l), expected, 0.2), "{} vs {expected}", area(l));
}

// ---------------------------------------------------------------------------------------------
// (D) handwriting: centerline T, X, Y → confirmed replacement → line operation
// ---------------------------------------------------------------------------------------------

/// Reference skeleton segments of a corpus letter in mm (y up), from its JSON.
fn skeleton(name: &str) -> Vec<(Point, Point)> {
    let json: serde_json::Value =
        serde_json::from_slice(&read_fixture("trace", &format!("synthetic/{name}.json")).unwrap()).unwrap();
    let mm = json["pixel_size_mm"].as_f64().unwrap();
    let h = json["height"].as_f64().unwrap();
    let px = |v: &serde_json::Value| Point::new(v[0].as_f64().unwrap() * mm, (h - v[1].as_f64().unwrap()) * mm);
    json["union_of"][0]["segments"].as_array().unwrap().iter().map(|s| (px(&s[0]), px(&s[1]))).collect()
}

fn distance_to_segment(p: Point, (a, b): (Point, Point)) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

/// Stroke ends of the reference: segment end points that no other segment shares.
fn reference_ends(segments: &[(Point, Point)]) -> Vec<Point> {
    let all: Vec<Point> = segments.iter().flat_map(|(a, b)| [*a, *b]).collect();
    all.iter().copied().filter(|p| all.iter().filter(|q| q.distance(*p) < 1e-9).count() == 1).collect()
}

fn handwriting(name: &str) {
    let png = read_fixture("trace", &format!("synthetic/{name}.png")).unwrap();
    let req = CenterlineRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(decode_png(&png, DecodeLimits::default()).unwrap()),
        image: ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(0.1).unwrap()),
            threshold: Threshold::Fixed(127),
            ..ImageRecipe::default()
        },
        settings: CenterlineSettings::default(),
    };
    let mut result = trace_centerline(&req, &CancellationToken::never()).unwrap();
    // The operator resolves each ambiguous junction explicitly (here: the best-scored pairing).
    let junctions: Vec<_> = result
        .nodes
        .iter()
        .filter_map(|n| match &n.connections {
            Connections::Ambiguous { candidates } => Some((n.id, candidates[0].pairs.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(junctions.len(), 1, "{name}: one junction to decide");
    for (node, pairs) in junctions {
        result = result.resolve_junction(node, pairs).unwrap();
    }
    // … and confirms the line replacement of every straight chain.
    let chains: Vec<_> = result.chains.iter().map(|c| c.id).collect();
    for chain in chains {
        let proposal = result.propose_replacement(chain).unwrap();
        assert!(proposal.candidate.within_tolerance, "{name}: corpus strokes are straight");
        result = result.confirm_replacement(&proposal).unwrap();
    }
    assert!(result.chains.iter().all(|c| matches!(c.confirmed, Some(burnburn_trace::Primitive::Line { .. }))));
    let strokes = result.strokes();
    let reference = skeleton(name);
    let ends = reference_ends(&reference);
    assert_eq!(strokes.len(), ends.len().div_ceil(2), "{name}: resolved strokes");

    let outputs: Vec<TraceOutput> = strokes
        .iter()
        .enumerate()
        .map(|(i, st)| TraceOutput {
            local: i as u64 + 1,
            kind: ObjectKind::Path,
            path: Path::from_subpaths(vec![st.path.clone()]),
            name: Some(format!("{name} streek {i}")),
        })
        .collect();
    let definition = TraceDefinition {
        name: format!("Handschrift {name}"),
        source: asset_hash(&png),
        recipe: EngineRecipe::new(CENTERLINE_ENGINE_VERSION),
        key: result.inputs,
        placement: Affine::IDENTITY,
    };
    let mut s = new_session();
    let created =
        apply(&mut s, CommandPayload::Trace(TraceCommand::Create { definition: Box::new(definition), outputs }), &[])
            .changes
            .created;
    assert_eq!(created.len(), strokes.len());
    add_op(&mut s, op("Graveerlijn", OperationMode::Line { kerf: Kerf::NONE }, 50.0, 25.0), created);
    let (_, file) = rd_of(&s.snapshot());
    assert_eq!(part_settings(&file, 0), (50_000, 4096, 0), "{name}: line layer 50 mm/s, 25 %");

    let polylines = loops(&cuts_by_part(&file)[&0]);
    assert_eq!(polylines.len(), strokes.len(), "{name}: one open cut per stroke");
    for l in &polylines {
        assert!(!closed(l), "{name}: centerlines stay open");
        for p in l {
            let d = reference.iter().map(|s| distance_to_segment(*p, *s)).fold(f64::INFINITY, f64::min);
            assert!(d <= 0.15, "{name}: {p:?} is {d:.3} mm from the reference skeleton");
        }
    }
    let cut_ends: Vec<Point> = polylines.iter().flat_map(|l| [l[0], *l.last().unwrap()]).collect();
    for e in &ends {
        let d = cut_ends.iter().map(|c| c.distance(*e)).fold(f64::INFINITY, f64::min);
        assert!(d <= 0.35, "{name}: stroke end {e:?} reached within {d:.3} mm");
    }
}

#[test]
fn headless_gate_d_handwriting_centerline_open_paths() {
    for name in ["stroke-t", "stroke-x", "stroke-y"] {
        handwriting(name);
    }
}

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}
