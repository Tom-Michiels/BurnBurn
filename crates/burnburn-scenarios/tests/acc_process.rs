//! Acceptance plan, process block: LY01 (layers: output vs visibility, sublayer order), CAM01
//! (order constraints under travel optimisation), CAM02 (tabs + kerf + leads + overcut + passes),
//! CAM03/CAM04 (fill semantics and coverage), CAM05 (selection + origin + one frame), PRE01 (one
//! revision hash for preview, export and job) and PRE02 (what a time estimate is, and is not).
//!
//! Expectations are arithmetic on the shapes drawn in each test: contour lengths, areas divided by
//! the scan interval, positions of anchors.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod acc_cam;
mod common;

use std::path::PathBuf;

use acc_cam::{add_op, fill_op, line_op, mm, plan_from_outside, rectangle, run, session, settings, supporting};
use burnburn_cam::compile;
use burnburn_document::process::{
    AdvancedOptions, CutPriority, Kerf, Lead, LineOptions, OperationDraft, OperationMode, Overcut, Passes, FillGrouping,
    OptimizationSettings, OrderKey, ProcessCommand, ProcessSettings, TabSpec,
};
use burnburn_document::{CommandPayload, DocumentSession, GeometryCommand};
use burnburn_geometry::{Path, PathBuilder, Point};
use burnburn_job::{Feature, JobPlan, PlanEvent, preview};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, UiIntent};
use burnburn_types::{CancellationToken, GeometryId, OperationId, StableId};
use proptest::prelude::*;

fn never() -> CancellationToken {
    CancellationToken::never()
}

// ---------------------------------------------------------------------------------------------
// LY01 output flag, visibility and sublayer order (through the layer palette of the app)
// ---------------------------------------------------------------------------------------------

fn window() -> (HeadlessEditorHarness, burnburn_types::DocumentId, GeometryId, GeometryId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(common::profile()), ruida: common::RUIDA });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.drag(doc, (60.0, 0.0), (100.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let ids: Vec<_> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    (h, doc, ids[0], ids[1])
}

fn assign(h: &mut HeadlessEditorHarness, doc: burnburn_types::DocumentId, id: GeometryId, slot: u8, add: bool) {
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot, add });
}

fn op_of(h: &HeadlessEditorHarness, doc: burnburn_types::DocumentId, slot: u8) -> OperationId {
    h.document(doc).unwrap().operations.iter().find(|o| o.slot == slot).unwrap().id
}

/// The laser-on extent of what the app would export for this document.
fn exported_extent(h: &mut HeadlessEditorHarness, doc: burnburn_types::DocumentId) -> Option<(Point, Point)> {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    h.fs().create_dir_all_for_test(std::path::Path::new("/out"));
    let path = PathBuf::from("/out/ly.rd");
    h.send(UiIntent::ExportRd { document: doc, path: path.clone() });
    h.run_tasks();
    let bytes = burnburn_io::fs::FileSystem::read(h.fs(), &path, u64::MAX).ok()?;
    let rd = common::decode(&bytes);
    let c = common::cuts(&rd);
    (!c.is_empty()).then(|| common::cut_extent(&c))
}

#[test]
fn ly01_output_and_visibility_are_independent_and_only_output_reaches_the_file() {
    let (mut h, doc, a, b) = window();
    assign(&mut h, doc, a, 0, false);
    assign(&mut h, doc, b, 1, false);
    let (lo, hi) = exported_extent(&mut h, doc).expect("both layers cut");
    assert_eq!((lo.x, hi.x), (0.0, 100.0));

    let op1 = op_of(&h, doc, 1);
    // Hidden but still output: the file keeps it (visibility is for the screen).
    h.send(UiIntent::SetOperationVisible { document: doc, operation: op1, visible: false });
    let (lo, hi) = exported_extent(&mut h, doc).unwrap();
    assert_eq!((lo.x, hi.x), (0.0, 100.0), "a hidden layer is still cut");
    // Visible but output off: drawn, and not in the file.
    h.send(UiIntent::SetOperationVisible { document: doc, operation: op1, visible: true });
    h.send(UiIntent::SetOperationOutput { document: doc, operation: op1, enabled: false });
    let drawn = h.scene(doc).unwrap().items.len();
    assert_eq!(drawn, 2, "both objects are still drawn");
    let (lo, hi) = exported_extent(&mut h, doc).unwrap();
    assert_eq!((lo.x, hi.x), (0.0, 40.0), "output off removes only layer 1 from the file");
    // Both off: nothing to export (the compile reports an empty job instead of writing a stub).
    let op0 = op_of(&h, doc, 0);
    h.send(UiIntent::SetOperationOutput { document: doc, operation: op0, enabled: false });
    assert!(exported_extent(&mut h, doc).is_none(), "no layer with output, no cut motion");
}

#[test]
fn ly01_sublayers_run_in_layer_order_and_cut_the_same_geometry_once_per_treatment() {
    let (mut h, doc, a, _) = window();
    assign(&mut h, doc, a, 4, false);
    h.send(UiIntent::AssignLayer { document: doc, slot: 2, add: true });
    let (first, second) = (op_of(&h, doc, 4), op_of(&h, doc, 2));
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let plan = h.document(doc).unwrap().plan.expect("compiled");
    let sequence: Vec<_> = plan.preview.moves.iter().filter_map(|m| m.operation).fold(Vec::new(), |mut v, o| {
        if v.last() != Some(&o) {
            v.push(o);
        }
        v
    });
    // Execution order = the order the treatments were given (first colour 4, then 2), not the
    // palette number: the sublayer is a second pass over the same geometry.
    assert_eq!(sequence, vec![first, second]);
    let cut_moves = plan.preview.moves.iter().filter(|m| m.source.is_some()).count();
    assert_eq!(cut_moves % 2, 0, "every segment is cut once per treatment");
    // Defaults of a new layer: visible and with output.
    for slot in [4, 2] {
        let row = h.document(doc).unwrap().operations.into_iter().find(|o| o.slot == slot).unwrap();
        assert!(row.visible && row.output_enabled && row.passes == "1", "{row:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// CAM01 order constraints survive travel optimisation
// ---------------------------------------------------------------------------------------------

fn cut_order(plan: &JobPlan) -> Vec<GeometryId> {
    let mut out: Vec<GeometryId> = Vec::new();
    for e in plan.events() {
        let g = match *e {
            PlanEvent::Cut { origin: Some(o), .. } | PlanEvent::ScanRun { origin: Some(o), .. } => o.geometry,
            _ => continue,
        };
        if out.last() != Some(&g) {
            out.push(g);
        }
    }
    out
}

struct Part {
    layer: usize,
    priority: i32,
    outer: GeometryId,
    inner: Option<GeometryId>,
}

/// `specs`: (layer 0/1, priority 0..3, nested?) per part; parts sit on a grid 60 mm apart.
fn build(specs: &[(usize, i32, bool)], order: Vec<OrderKey>, reduce: bool) -> (DocumentSession, Vec<Part>, [OperationId; 2]) {
    let mut s = session();
    let mut parts = Vec::new();
    let mut targets: [Vec<GeometryId>; 2] = [Vec::new(), Vec::new()];
    for (k, &(layer, priority, nested)) in specs.iter().enumerate() {
        let (x, y) = ((k % 4) as f64 * 60.0, (k / 4) as f64 * 60.0);
        let outer = rectangle(&mut s, x, y, 30.0, 30.0);
        let inner = nested.then(|| rectangle(&mut s, x + 10.0, y + 10.0, 10.0, 10.0));
        targets[layer].push(outer);
        targets[layer].extend(inner);
        parts.push(Part { layer, priority, outer, inner });
    }
    let op0 = add_op(&mut s, line_op("L1", 20.0, 50.0), targets[0].clone());
    let op1 = add_op(&mut s, line_op("L2", 30.0, 60.0), targets[1].clone());
    for p in &parts {
        for g in std::iter::once(p.outer).chain(p.inner) {
            let priority = CutPriority::new(p.priority).unwrap();
            run(&mut s, CommandPayload::Process(ProcessCommand::SetPriority { geometry: g, priority }));
        }
    }
    let settings = OptimizationSettings::default().with_order(order).unwrap();
    let settings = settings.with_reduce_travel(reduce);
    run(&mut s, CommandPayload::Process(ProcessCommand::SetOptimization { settings }));
    (s, parts, [op0, op1])
}

fn compiled(s: &DocumentSession) -> JobPlan {
    compile(&s.snapshot(), &common::profile(), &common::options(), &never()).unwrap()
}

fn check_order(parts: &[Part], order: &[OrderKey], plan: &JobPlan) {
    let seq = cut_order(plan);
    let total: usize = parts.iter().map(|p| 1 + usize::from(p.inner.is_some())).sum();
    assert_eq!(seq.len(), total, "every geometry is cut exactly once");
    let pos = |g: GeometryId| seq.iter().position(|x| *x == g).unwrap();
    for p in parts {
        if let Some(inner) = p.inner {
            assert!(pos(inner) < pos(p.outer), "inner contour before the outer one it sits in");
        }
    }
    let key_of = |g: GeometryId| {
        let p = parts.iter().find(|p| p.outer == g || p.inner == Some(g)).unwrap();
        order
            .iter()
            .map(|k| match k {
                OrderKey::Layer => p.layer as i32,
                OrderKey::Priority => p.priority,
                _ => 0,
            })
            .collect::<Vec<_>>()
    };
    let keys: Vec<_> = seq.iter().map(|g| key_of(*g)).collect();
    assert!(keys.windows(2).all(|w| w[0] <= w[1]), "the Order by hierarchy {order:?} is not respected: {keys:?}");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 40, failure_persistence: None, ..ProptestConfig::default() })]

    /// Whatever the layers, priorities and nesting, travel optimisation only chooses inside what
    /// the hierarchy leaves open.
    #[test]
    fn cam01_property_order_keys_and_inner_first_hold_under_travel_optimisation(
        specs in proptest::collection::vec((0_usize..2, 0_i32..3, any::<bool>()), 2..8),
        which in 0_usize..3, reduce in any::<bool>(),
    ) {
        let order = [vec![OrderKey::Layer], vec![OrderKey::Layer, OrderKey::Priority], vec![OrderKey::Priority, OrderKey::Layer]][which].clone();
        // A nested pair shares its priority (the inner/outer rule is only defined inside one level).
        let (s, parts, _) = build(&specs, order.clone(), reduce);
        check_order(&parts, &order, &compiled(&s));
    }
}

#[test]
fn cam01_a_hard_order_constraint_overrides_the_layer_order_and_a_cycle_is_refused() {
    let specs = [(0, 0, true), (1, 0, false), (0, 0, false), (1, 0, true)];
    let (mut s, parts, [op0, op1]) = build(&specs, vec![OrderKey::Layer], true);
    // Layer 2 first, although layer 1 comes first by default.
    run(&mut s, CommandPayload::Process(ProcessCommand::AddOrderConstraint { before: op1, after: op0 }));
    let seq = cut_order(&compiled(&s));
    let layer_of = |g: GeometryId| parts.iter().find(|p| p.outer == g || p.inner == Some(g)).unwrap().layer;
    let layers: Vec<usize> = seq.iter().map(|g| layer_of(*g)).collect();
    assert!(layers.windows(2).all(|w| w[0] >= w[1]), "layer 2 completely before layer 1: {layers:?}");
    // The reverse constraint would close a cycle: refused, the document keeps working.
    let cmd = burnburn_document::DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: burnburn_types::RequestId::from_raw(9),
        payload: CommandPayload::Process(ProcessCommand::AddOrderConstraint { before: op0, after: op1 }),
    };
    assert!(s.apply(cmd).is_err());
}

// ---------------------------------------------------------------------------------------------
// CAM02 tabs + kerf + leads + overcut + several passes
// ---------------------------------------------------------------------------------------------

/// Per pass: cut length and the lengths of the laser-off gaps that lie inside the pass's contour.
fn per_pass(plan: &JobPlan) -> Vec<(f64, Vec<f64>)> {
    let mut passes: Vec<(f64, Vec<f64>)> = Vec::new();
    let mut at = plan.start();
    let mut last_cut_end: Option<Point> = None;
    for e in plan.events() {
        match *e {
            PlanEvent::OperationBoundary { .. } => {
                passes.push((0.0, Vec::new()));
                last_cut_end = None;
            }
            PlanEvent::Travel { to } => {
                if let (Some(end), Some(p)) = (last_cut_end, passes.last_mut()) {
                    p.1.push(Point::new(at.x_mm(), at.y_mm()).distance(Point::new(to.x_mm(), to.y_mm())));
                    let _ = end;
                }
                at = to;
            }
            PlanEvent::Cut { to, .. } => {
                if let Some(p) = passes.last_mut() {
                    p.0 += at.distance(to);
                }
                at = to;
                last_cut_end = Some(Point::new(to.x_mm(), to.y_mm()));
            }
            _ => {}
        }
    }
    passes
}

#[test]
fn cam02_tabs_kerf_leads_overcut_and_passes_give_the_arithmetic_length_without_bridges() {
    let mut s = session();
    let g = rectangle(&mut s, 100.0, 100.0, 40.0, 30.0);
    let draft = OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::width(mm(0.4)).unwrap() },
        settings: ProcessSettings { passes: Passes::new(2).unwrap(), ..settings(10.0, 50.0) },
        output_enabled: true,
    };
    let op = add_op(&mut s, draft, vec![g]);
    let line = LineOptions {
        tabs: Some(TabSpec::new(4, mm(2.0)).unwrap()),
        lead_in: Some(Lead::new(mm(3.0)).unwrap()),
        lead_out: Some(Lead::new(mm(3.0)).unwrap()),
        overcut: Some(Overcut::new(mm(1.0)).unwrap()),
        ..Default::default()
    };
    run(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced: AdvancedOptions { line, ..Default::default() } }));
    let plan = plan_from_outside(&s, &common::profile()).unwrap();

    // Kerf: the cut path is offset by half the kerf on every side. Its bounds are the shape's +-0.2
    // (leads lie on the waste side and stay outside the contour bounds only at the entry).
    let passes = per_pass(&plan);
    assert_eq!(passes.len(), 2, "two passes, two operation blocks");
    // Contour length of the offset rectangle: mitred corners would give 2 * (40.4 + 30.4) = 141.6,
    // rounded corners 140 + 2 pi 0.2 = 141.2566; the planner uses one of the two.
    let miter = 2.0 * (40.4 + 30.4);
    let round = 140.0 + 2.0 * std::f64::consts::PI * 0.2;
    let lead_total = 3.0 + 3.0;
    let tabs_total = 4.0 * 2.0;
    let candidates: Vec<f64> = [miter, round].iter().map(|c| c - tabs_total + lead_total).collect();
    let first = passes[0].0;
    assert!(candidates.iter().any(|c| (first - c).abs() < 0.01), "pass 1 cuts {first}, expected one of {candidates:?}");
    // The overcut (1 mm) is cut on the last pass only.
    assert!((passes[1].0 - first - 1.0).abs() < 1e-6, "pass 2 - pass 1 = {}", passes[1].0 - first);
    // Tabs: every pass leaves four 2 mm gaps (laser-off moves inside the contour), no bridge is cut.
    for (n, (_, gaps)) in passes.iter().enumerate() {
        let tab_gaps: Vec<f64> = gaps.iter().copied().filter(|g| (*g - 2.0).abs() < 1e-6).collect();
        assert_eq!(tab_gaps.len(), 4, "pass {n}: gaps {gaps:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// CAM03 / CAM04 fill semantics and coverage
// ---------------------------------------------------------------------------------------------

fn scan_len(plan: &JobPlan) -> f64 {
    preview(plan, &common::profile()).scan_length_mm
}

/// Total scanned length per settings block (one block per scan direction).
fn scan_by_block(plan: &JobPlan) -> Vec<f64> {
    let mut v: Vec<f64> = Vec::new();
    for e in plan.events() {
        match *e {
            PlanEvent::OperationBoundary { .. } => v.push(0.0),
            PlanEvent::ScanRun { line, .. } => {
                if let Some(x) = v.last_mut() {
                    *x += line.from.distance(line.to);
                }
            }
            _ => {}
        }
    }
    v
}

fn fill_plan(shapes: &[(f64, f64, f64, f64)], same_layer: bool, interval: f64) -> JobPlan {
    let mut s = session();
    let ids: Vec<_> = shapes.iter().map(|&(x, y, w, h)| rectangle(&mut s, x, y, w, h)).collect();
    if same_layer {
        add_op(&mut s, fill_op("Vullen", interval, 0.0), ids);
    } else {
        for (k, id) in ids.into_iter().enumerate() {
            add_op(&mut s, fill_op(&format!("Vullen {k}"), interval, 0.0), vec![id]);
        }
    }
    plan_from_outside(&s, &common::profile()).unwrap()
}

#[test]
fn cam03_parity_fill_xors_overlaps_in_one_layer_and_burns_them_twice_across_layers() {
    let overlap = [(0.0, 0.0, 10.0, 10.0), (5.0, 5.0, 10.0, 10.0)]; // overlap 5 x 5
    // Same operation: even-odd across shapes: 100 + 100 - 2 * 25 = 150 mm2 / 0.1 mm = 1500 mm of scan.
    assert!((scan_len(&fill_plan(&overlap, true, 0.1)) - 1500.0).abs() < 1e-6);
    // Different layers: each shape is filled on its own: 100 + 100 mm2 = 2000 mm; the overlap is burned twice.
    assert!((scan_len(&fill_plan(&overlap, false, 0.1)) - 2000.0).abs() < 1e-6);
    // Shapes that only share an edge: no overlap, no gap, no double burn in either case.
    let touching = [(0.0, 0.0, 10.0, 10.0), (10.0, 0.0, 10.0, 10.0)];
    assert!((scan_len(&fill_plan(&touching, true, 0.1)) - 2000.0).abs() < 1e-6);
    assert!((scan_len(&fill_plan(&touching, false, 0.1)) - 2000.0).abs() < 1e-6);
    // A shape wholly inside another in one layer is a hole (even-odd): 400 - 100 = 300 mm2.
    let nested = [(0.0, 0.0, 20.0, 20.0), (5.0, 5.0, 10.0, 10.0)];
    assert!((scan_len(&fill_plan(&nested, true, 0.1)) - 3000.0).abs() < 1e-6);
    // ... and burned over again when it is another layer.
    assert!((scan_len(&fill_plan(&nested, false, 0.1)) - 5000.0).abs() < 1e-6);
}

fn ring_square() -> Path {
    // 20 x 20 with a 6 x 6 hole: area 364 mm2.
    PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(20.0, 0.0))
        .line_to(Point::new(20.0, 20.0))
        .line_to(Point::new(0.0, 20.0))
        .close()
        .move_to(Point::new(7.0, 7.0))
        .line_to(Point::new(7.0, 13.0))
        .line_to(Point::new(13.0, 13.0))
        .line_to(Point::new(13.0, 7.0))
        .close()
        .build()
        .unwrap()
}

fn path_geometry(s: &mut DocumentSession, path: Path) -> GeometryId {
    run(s, CommandPayload::Edit(GeometryCommand::CreatePath { path, name: String::new() })).changes.created[0]
}

#[test]
fn cam04_every_wanted_area_is_scanned_exactly_once_per_planned_pass() {
    let interval = 0.5;
    let area = 364.0;
    let once = area / interval; // 728 mm of scan lines per pass
    let doc = |f: burnburn_document::process::FillSettings| {
        let mut s = session();
        let g = path_geometry(&mut s, ring_square());
        let draft = OperationDraft {
            name: "Vullen".into(),
            mode: OperationMode::Fill(f),
            settings: settings(100.0, 20.0),
            output_enabled: true,
        };
        add_op(&mut s, draft, vec![g]);
        s
    };
    let base = burnburn_document::process::FillSettings::new(
        mm(interval),
        burnburn_document::process::ScanAxis::X,
        burnburn_document::process::ScanDirection::Bidirectional,
        mm(0.0),
    )
    .unwrap();
    // Plain fill: once.
    let plain = plan_from_outside(&doc(base), &common::profile()).unwrap();
    assert!((scan_len(&plain) - once).abs() < 1e-6, "{}", scan_len(&plain));
    // Flood fill: the same area, the same single scan.
    let flood = plan_from_outside(&doc(base.with_flood_fill(true)), &common::profile()).unwrap();
    assert!((scan_len(&flood) - once).abs() < 1e-6);
    // Cross-hatch: two planned passes, each of them covers the area once.
    let hatch = plan_from_outside(&doc(base.with_cross_hatch(true)), &common::profile()).unwrap();
    let blocks = scan_by_block(&hatch);
    assert_eq!(blocks.len(), 2);
    assert!(blocks.iter().all(|b| (b - once).abs() < 1e-6), "{blocks:?}");
    // Grouped fill: the ring and a copy of it, grouped: once each.
    let mut s = session();
    let a = path_geometry(&mut s, ring_square());
    let b = path_geometry(&mut s, ring_square().transform(&burnburn_geometry::Affine::translate(burnburn_geometry::Vector::new(40.0, 0.0)).unwrap()));
    run(&mut s, CommandPayload::Edit(GeometryCommand::Group { targets: vec![a, b] }));
    let draft = OperationDraft {
        name: "Vullen".into(),
        mode: OperationMode::Fill(base.with_grouping(FillGrouping::Groups)),
        settings: settings(100.0, 20.0),
        output_enabled: true,
    };
    add_op(&mut s, draft, vec![a, b]);
    let grouped = plan_from_outside(&s, &common::profile()).unwrap();
    assert!((scan_len(&grouped) - 2.0 * once).abs() < 1e-6);
    // Every row is a set of disjoint spans (no scan line overlaps another in the same pass).
    for plan in [&plain, &flood, &grouped] {
        let mut by_row: std::collections::BTreeMap<i64, Vec<(f64, f64)>> = Default::default();
        for e in plan.events() {
            if let PlanEvent::ScanRun { line, .. } = *e {
                let (a, b) = (line.from.x_mm(), line.to.x_mm());
                by_row.entry((line.from.y_mm() * 1000.0).round() as i64).or_default().push((a.min(b), a.max(b)));
            }
        }
        for spans in by_row.values_mut() {
            spans.sort_by(|x, y| x.0.total_cmp(&y.0));
            assert!(spans.windows(2).all(|w| w[0].1 <= w[1].0 + 1e-9), "overlapping scan spans {spans:?}");
        }
    }
}

#[test]
fn cam04_offset_fill_rings_total_the_concentric_perimeters() {
    use burnburn_document::process::{OffsetFillOrder, OffsetFillSettings};
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 20.0, 20.0);
    let draft = OperationDraft {
        name: "Offset".into(),
        mode: OperationMode::OffsetFill(OffsetFillSettings::new(mm(2.0), OffsetFillOrder::OutsideIn).unwrap()),
        settings: settings(100.0, 20.0),
        output_enabled: true,
    };
    add_op(&mut s, draft, vec![g]);
    let plan = plan_from_outside(&s, &common::profile()).unwrap();
    // Rings sit at half-step insets (1, 3, 5, 7, 9 mm) so that a line of one step width centred on
    // each ring tiles the area: sides 18, 14, 10, 6, 2 -> perimeters 72 + 56 + 40 + 24 + 8 = 200.
    let cut = preview(&plan, &common::profile()).cut_length_mm;
    assert!((cut - 200.0).abs() < 0.01, "ring perimeters sum to {cut}");
    let _ = supporting(&[Feature::ScanAngle]);
}

// ---------------------------------------------------------------------------------------------
// CAM05 selection + origin + frame: preview, machine bounds and the frame job are one thing
// ---------------------------------------------------------------------------------------------

#[test]
fn cam05_the_placed_plan_its_preview_and_the_header_bounds_agree_on_selection_and_origin() {
    use burnburn_machine::{CompileScope, JobAnchor, PlacementInput, PlacementSpec, Rect, ScopeObject, StartFrom, place_plan, resolve_scope};
    let mut s = session();
    let a = rectangle(&mut s, 100.0, 100.0, 40.0, 20.0);
    let b = rectangle(&mut s, 300.0, 200.0, 10.0, 10.0);
    add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![a, b]);
    let profile = common::profile();
    let plan = compile(&s.snapshot(), &profile, &common::options(), &never()).unwrap();

    // "Use selection origin" with b selected, head at (500, 400), anchor bottom-left: b's corner (300, 200)
    // lands on the head; everything is still cut.
    let rect = |x, y, w, h| Rect::new(x, y, x + w, y + h).unwrap();
    let objects = [
        ScopeObject { id: a.raw(), bounds: rect(100.0, 100.0, 40.0, 20.0) },
        ScopeObject { id: b.raw(), bounds: rect(300.0, 200.0, 10.0, 10.0) },
    ];
    let scope = resolve_scope(&CompileScope { cut_selected: false, use_selection_origin: true }, &objects, &[b.raw()]).unwrap();
    assert_eq!(scope.included.len(), 2, "all objects are cut");
    let head = burnburn_job::MachinePoint::new(500.0, 400.0).unwrap();
    let input = PlacementInput { frame: plan.frame(), current_position: Some(head), user_origin: None };
    let placed = place_plan(
        &plan,
        &PlacementSpec { start_from: StartFrom::CurrentPosition, anchor: JobAnchor::BottomLeft },
        scope.origin_bounds,
        &input,
    )
    .unwrap();
    assert_eq!(placed.delta_mm, (200.0, 200.0), "(300, 200) -> (500, 400)");
    // Preview of the placed plan, the plan's own bounds and the RD header all describe the same box.
    let (lo, hi) = (100.0 + 200.0, 100.0 + 200.0);
    let pv = preview(&placed.plan, &profile);
    let xs: Vec<f64> = pv.moves.iter().filter(|m| m.source.is_some()).flat_map(|m| [m.from.x_mm(), m.to.x_mm()]).collect();
    let ys: Vec<f64> = pv.moves.iter().filter(|m| m.source.is_some()).flat_map(|m| [m.from.y_mm(), m.to.y_mm()]).collect();
    let min_x = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let min_y = ys.iter().copied().fold(f64::INFINITY, f64::min);
    let max_x = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let max_y = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert_eq!((min_x, min_y, max_x, max_y), (lo, hi, 510.0, 410.0));
    let rd = common::decode(&common::rd_bytes(&placed.plan));
    let (hdr_lo, hdr_hi) = common::header_bounds(&rd);
    assert_eq!((hdr_lo.x, hdr_lo.y, hdr_hi.x, hdr_hi.y), (min_x, min_y, max_x, max_y), "RD header = preview extent");
    let (cut_lo, cut_hi) = common::cut_extent(&common::cuts(&rd));
    assert_eq!((cut_lo.x, cut_lo.y, cut_hi.x, cut_hi.y), (min_x, min_y, max_x, max_y), "and the cut motion");
}

// ---------------------------------------------------------------------------------------------
// PRE01 one revision hash for preview, export and job
// ---------------------------------------------------------------------------------------------

#[test]
fn pre01_preview_export_and_job_name_one_plan_and_one_rd_hash_and_edits_make_them_stale() {
    let (mut h, doc, a, _) = window();
    assign(&mut h, doc, a, 0, false);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    h.fs().create_dir_all_for_test(std::path::Path::new("/out"));
    let path = PathBuf::from("/out/pre.rd");
    h.send(UiIntent::ExportRd { document: doc, path: path.clone() });
    h.run_tasks();
    h.invoke(doc, CommandKey::PrepareJob);
    h.run_tasks();
    let vm = h.document(doc).unwrap();
    let (plan, export, job) = (vm.plan.clone().unwrap(), vm.export.clone().unwrap(), vm.job.clone().unwrap());
    assert_eq!(plan.preview.plan, plan.id, "the preview carries the plan it was derived from");
    assert_eq!(export.plan, plan.id);
    assert_eq!(job.plan, plan.id);
    let bytes = burnburn_io::fs::FileSystem::read(h.fs(), &path, u64::MAX).unwrap();
    assert_eq!(export.rd_hash, burnburn_types::ContentHash::of(burnburn_types::HashDomain::Rd, &bytes));
    assert_eq!(export.rd_hash, job.rd_hash, "export and upload are the same bytes");
    // The preview's cut length is the cut motion of the file: three independent routes to one number.
    let rd = common::decode(&bytes);
    let from_file: f64 = common::cuts(&rd).iter().map(|c| c.from.distance(c.to)).sum();
    assert!((from_file - plan.preview.cut_length_mm).abs() < 0.01, "{from_file} vs {}", plan.preview.cut_length_mm);

    // An edit makes the shown plan stale; compiling again gives another plan id and other bytes.
    h.send(UiIntent::Select { document: doc, ids: vec![a], mode: SelectMode::Replace });
    h.send(UiIntent::CommitField { document: doc, field: burnburn_presentation::FieldKey::Width, text: "45".into() });
    let stale = h.document(doc).unwrap();
    assert!(stale.plan.as_ref().unwrap().stale, "the old preview says it is stale");
    assert_eq!(stale.job.as_ref().map(|j| j.plan), Some(plan.id), "a prepared job never changes under an edit");
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let fresh = h.document(doc).unwrap().plan.unwrap();
    assert_ne!(fresh.id, plan.id);
    assert!(!fresh.stale);
}

// ---------------------------------------------------------------------------------------------
// PRE02 the estimate is arithmetic, labelled as such; accuracy needs a stopwatch (hardware)
// ---------------------------------------------------------------------------------------------

#[test]
fn pre02_the_estimate_is_length_over_speed_for_ten_job_shapes_and_claims_no_accuracy() {
    let profile = common::profile(); // travel 400 mm/s
    let cases: Vec<(&str, JobPlan, f64)> = {
        let mut out = Vec::new();
        let mk = |w: f64, speed: f64| {
            let mut s = session();
            let g = rectangle(&mut s, 10.0, 10.0, w, w);
            add_op(&mut s, line_op("c", speed, 50.0), vec![g]);
            plan_from_outside(&s, &profile).unwrap()
        };
        // Short and long cutting jobs at different speeds: time = perimeter / speed + travel / 400.
        for (name, w, v) in [("short-slow", 5.0, 10.0), ("short-fast", 5.0, 200.0), ("long-slow", 400.0, 20.0), ("long-fast", 400.0, 300.0)] {
            let plan = mk(w, v);
            let pv = preview(&plan, &profile);
            let expected = 4.0 * w / v + pv.travel_length_mm / 400.0;
            out.push((name, plan, expected));
        }
        // Raster jobs: scan time = scanned length / speed (+ travel); interval 0.5 mm.
        for (name, w) in [("raster-small", 10.0), ("raster-large", 100.0)] {
            let mut s = session();
            let g = rectangle(&mut s, 10.0, 10.0, w, w);
            let draft = OperationDraft {
                name: "r".into(),
                mode: OperationMode::Fill(
                    burnburn_document::process::FillSettings::new(
                        mm(0.5),
                        burnburn_document::process::ScanAxis::X,
                        burnburn_document::process::ScanDirection::Bidirectional,
                        mm(0.0),
                    )
                    .unwrap(),
                ),
                settings: settings(100.0, 20.0),
                output_enabled: true,
            };
            add_op(&mut s, draft, vec![g]);
            let plan = plan_from_outside(&s, &profile).unwrap();
            let pv = preview(&plan, &profile);
            let expected = (w * w / 0.5) / 100.0 + pv.travel_length_mm / 400.0;
            out.push((name, plan, expected));
        }
        // Mixed: a cut and a raster in one job; and two passes of a cut.
        let mut s = session();
        let a = rectangle(&mut s, 10.0, 10.0, 20.0, 20.0);
        let b = rectangle(&mut s, 100.0, 10.0, 20.0, 20.0);
        add_op(&mut s, line_op("c", 50.0, 50.0), vec![a]);
        add_op(&mut s, fill_op("f", 1.0, 0.0), vec![b]);
        let plan = plan_from_outside(&s, &profile).unwrap();
        let pv = preview(&plan, &profile);
        out.push(("mixed", plan, 80.0 / 50.0 + (400.0 / 1.0) / 100.0 + pv.travel_length_mm / 400.0));
        let mut s = session();
        let g = rectangle(&mut s, 10.0, 10.0, 30.0, 30.0);
        let draft = OperationDraft { settings: ProcessSettings { passes: Passes::new(2).unwrap(), ..settings(60.0, 50.0) }, ..line_op("c", 60.0, 50.0) };
        add_op(&mut s, draft, vec![g]);
        let plan = plan_from_outside(&s, &profile).unwrap();
        let pv = preview(&plan, &profile);
        out.push(("two-passes", plan, 2.0 * 120.0 / 60.0 + pv.travel_length_mm / 400.0));
        // Cutting, engraving at a higher interval and a tiny job.
        let mut s = session();
        let g = rectangle(&mut s, 10.0, 10.0, 1.0, 1.0);
        add_op(&mut s, line_op("c", 10.0, 50.0), vec![g]);
        let plan = plan_from_outside(&s, &profile).unwrap();
        let pv = preview(&plan, &profile);
        out.push(("tiny", plan, 4.0 / 10.0 + pv.travel_length_mm / 400.0));
        // A coarse raster: 20 x 20 mm at interval 2 mm.
        let mut s = session();
        let g = rectangle(&mut s, 10.0, 10.0, 20.0, 20.0);
        add_op(&mut s, fill_op("coarse", 2.0, 0.0), vec![g]);
        let plan = plan_from_outside(&s, &profile).unwrap();
        let pv = preview(&plan, &profile);
        out.push(("coarse-raster", plan, (400.0 / 2.0) / 100.0 + pv.travel_length_mm / 400.0));
        out
    };
    assert_eq!(cases.len(), 10);
    for (name, plan, expected) in &cases {
        let got = preview(plan, &profile).estimated_seconds;
        assert!((got - expected).abs() < 1e-6, "{name}: estimate {got} s, arithmetic {expected} s");
    }
}
