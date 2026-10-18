//! Public contract of the CAM compiler (IM13): semantic JobPlan tests (units, origin, order,
//! moves, scans, power policy). Expected values are analytical, from the fixture geometry.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use burnburn_cam::{CompileError, JobAnchor, Placement, StartPoint, compile};
use burnburn_document::process::{Kerf, OperationMode, Passes, ProcessCommand};
use burnburn_document::{
    CommandPayload, DocumentSession, DocumentSnapshot, GeometryObject, GeometrySource, ObjectKind,
};
use burnburn_geometry::{Path, PathBuilder, Point};
use burnburn_job::{OriginCorner, PlanEvent, PlanMode, PlanUnits, SourceRef};
use burnburn_types::{CancellationSource, CancellationToken, DocumentId, GeometryId, StableId};
use common::*;

#[test]
fn rectangle_line_plan_enters_at_nearest_corner_and_closes_the_contour() {
    let mut s = session();
    let g = rectangle(&mut s, 100.0, 50.0, 30.0, 15.0);
    let op = add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![g]);
    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();

    assert_eq!(plan.units(), PlanUnits::MillimetreSecond);
    assert_eq!(plan.settings().len(), 1);
    let set = plan.settings()[0];
    assert_eq!(set.mode, PlanMode::Line);
    assert_eq!(set.speed_mm_s, 100.0, "mm/s, not mm/min");
    assert_eq!((set.min_power.get(), set.max_power.get()), (0.2, 0.2), "20 % → ratio 0.2");
    assert!(set.air_assist);
    assert!(matches!(plan.events()[0], PlanEvent::OperationBoundary { operation, pass: 0, .. } if operation == op));
    assert_eq!(
        motion(plan.events()),
        vec![
            ("travel", 100.0, 50.0),
            ("cut", 130.0, 50.0),
            ("cut", 130.0, 65.0),
            ("cut", 100.0, 65.0),
            ("cut", 100.0, 50.0),
        ],
        "enter at the corner nearest the machine origin, keep the drawn direction, close the loop"
    );
    let src = SourceRef { geometry: g, operation: op };
    assert!(
        plan.events().iter().all(|e| !matches!(e, PlanEvent::Cut { origin, .. } if *origin != Some(src))),
        "every cut is traceable to its source geometry and operation"
    );
    assert_eq!(plan.source().document_revision, s.revision());
}

/// A plate with a hole as one geometry (two subpaths), a separate small part inside the plate
/// and a part far away.
fn nested_document() -> (DocumentSession, [GeometryId; 3]) {
    let ring = PathBuilder::new()
        .move_to(Point::new(10.0, 10.0))
        .line_to(Point::new(110.0, 10.0))
        .line_to(Point::new(110.0, 110.0))
        .line_to(Point::new(10.0, 110.0))
        .close()
        .move_to(Point::new(80.0, 80.0))
        .line_to(Point::new(90.0, 80.0))
        .line_to(Point::new(90.0, 90.0))
        .line_to(Point::new(80.0, 90.0))
        .close()
        .build()
        .unwrap();
    let object = |id: u64, path: Path| {
        GeometryObject::new(GeometryId::from_raw(id), String::new(), ObjectKind::Path, GeometrySource::Drawn, path)
    };
    let small = path_of(&[(30.0, 30.0), (40.0, 30.0), (40.0, 40.0), (30.0, 40.0)]);
    let far = path_of(&[(200.0, 10.0), (210.0, 10.0), (210.0, 20.0), (200.0, 20.0)]);
    let snap = DocumentSnapshot::from_parts(vec![object(1, ring), object(2, small), object(3, far)], 4).unwrap();
    let s = DocumentSession::from_snapshot(DocumentId::from_raw(1), snap);
    (s, [GeometryId::from_raw(1), GeometryId::from_raw(2), GeometryId::from_raw(3)])
}

fn travels(events: &[PlanEvent]) -> Vec<(f64, f64)> {
    motion(events).into_iter().filter(|m| m.0 == "travel").map(|m| (m.1, m.2)).collect()
}

#[test]
fn inner_contours_are_cut_before_the_contour_that_contains_them() {
    let (mut s, [plate, small, far]) = nested_document();
    add_op(&mut s, line_op("Snijden", 20.0, 80.0), vec![plate, small, far]);
    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();

    // Nearest neighbour alone would start at the plate corner (10, 10). Containment forces the
    // small part (30, 30) and the hole (80, 80) first; then the plate, entered at the vertex
    // nearest the head; the far part last, entered at (200, 20): 127.3 mm from (110, 110)
    // versus 134.5 mm for (200, 10).
    assert_eq!(travels(plan.events()), vec![(30.0, 30.0), (80.0, 80.0), (110.0, 110.0), (200.0, 20.0)]);
    let cuts = motion(plan.events()).iter().filter(|m| m.0 == "cut").count();
    assert_eq!(cuts, 16, "four closed quadrilaterals, four cuts each");
}

#[test]
fn nearest_neighbour_ties_are_broken_by_target_order() {
    let mut s = session();
    let right = rectangle(&mut s, 10.0, 0.0, 5.0, 5.0); // nearest corner (10, 0): distance 10
    let up = rectangle(&mut s, 0.0, 10.0, 5.0, 5.0); // nearest corner (0, 10): distance 10
    add_op(&mut s, line_op("a", 10.0, 10.0), vec![up, right]);
    let first = travels(compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap().events());
    assert_eq!(first[0], (0.0, 10.0), "equal distance: earlier target wins");

    let mut t = session();
    let right = rectangle(&mut t, 10.0, 0.0, 5.0, 5.0);
    let up = rectangle(&mut t, 0.0, 10.0, 5.0, 5.0);
    add_op(&mut t, line_op("a", 10.0, 10.0), vec![right, up]);
    let second = travels(compile(&t.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap().events());
    assert_eq!(second[0], (10.0, 0.0));
}

fn scans(events: &[PlanEvent]) -> Vec<((f64, f64), (f64, f64), f64)> {
    events
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::ScanRun { line, .. } => {
                Some(((line.from.x_mm(), line.from.y_mm()), (line.to.x_mm(), line.to.y_mm()), line.overscan_mm))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn fill_rows_sit_at_row_centres_and_alternate_direction() {
    let mut s = session();
    let g = rectangle(&mut s, 100.0, 50.0, 3.0, 1.0);
    add_op(&mut s, fill_op("Vullen", 0.25, 2.0), vec![g]);
    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();

    assert!(
        matches!(plan.settings()[0].mode, PlanMode::Scan { interval_mm, bidirectional: true, .. } if interval_mm == 0.25)
    );
    // 1 mm / 0.25 mm = 4 rows, half an interval inside the edges; start at the corner nearest
    // the head (bottom-left), then zig-zag.
    assert_eq!(
        scans(plan.events()),
        vec![
            ((100.0, 50.125), (103.0, 50.125), 2.0),
            ((103.0, 50.375), (100.0, 50.375), 2.0),
            ((100.0, 50.625), (103.0, 50.625), 2.0),
            ((103.0, 50.875), (100.0, 50.875), 2.0),
        ]
    );
    let b = plan.bounds().unwrap();
    assert_eq!((b.min().x, b.max().x), (98.0, 105.0), "bounds include 2 mm overscan on both sides");
}

#[test]
fn fill_leaves_holes_open_even_odd() {
    let (mut s, [plate, ..]) = nested_document();
    add_op(&mut s, fill_op("Vullen", 25.0, 0.0), vec![plate]);
    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    // Rows at 22.5, 47.5, 72.5, 97.5 (four rows of 25 mm in 100 mm); none crosses the hole.
    for ((x0, y), (x1, _), _) in scans(plan.events()) {
        let (lo, hi) = (x0.min(x1), x0.max(x1));
        assert!(!(y > 80.0 && y < 90.0 && lo < 85.0 && hi > 85.0), "run at y={y} crosses the hole");
    }
    assert_eq!(scans(plan.events()).len(), 4);

    // A row through the hole is split into two runs.
    let (mut s2, [plate2, ..]) = nested_document();
    add_op(&mut s2, fill_op("Vullen", 20.0, 0.0), vec![plate2]);
    let plan2 = compile(&s2.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    // Rows at 20, 40, 60, 80, 100 (5 rows of 20 mm in 100 mm); y = 80 lies on the hole's lower
    // edge (half-open rule: counted inside the hole), so that row is split.
    let row80: Vec<_> = scans(plan2.events()).into_iter().filter(|r| r.0.1 == 80.0).collect();
    assert_eq!(row80.len(), 2, "{row80:?}");
}

#[test]
fn same_snapshot_compiles_to_identical_plan_hash() {
    let build = || {
        let (mut s, [plate, small, far]) = nested_document();
        add_op(&mut s, line_op("Snijden", 20.0, 80.0), vec![plate, small, far]);
        add_op(&mut s, fill_op("Vullen", 0.5, 1.0), vec![small]);
        s
    };
    let (a, b) = (build(), build());
    let pa = compile(&a.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    let pa2 = compile(&a.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    let pb = compile(&b.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    assert_eq!(pa.id(), pa2.id(), "same revision, same plan hash");
    assert_eq!(pa, pa2);
    assert_eq!(pa.id(), pb.id(), "independently built identical document, same plan hash");

    let mut c = build();
    let g = GeometryId::from_raw(3);
    run(&mut c, CommandPayload::Rename { target: g, name: "ver".into() });
    let pc = compile(&c.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    assert_ne!(pc.source().source_hash, pa.source().source_hash, "source hash covers the document content");
    let other = profile_with(OriginCorner::TopLeft);
    let pd = compile(&a.snapshot(), &other, &options(), &CancellationToken::never()).unwrap();
    assert_ne!(pd.id(), pa.id(), "another machine frame is another plan");
}

/// Cut contours of the plan, one vertex list per travel-started run of cuts.
fn cut_loops(events: &[PlanEvent]) -> Vec<Vec<(f64, f64)>> {
    let mut loops: Vec<Vec<(f64, f64)>> = Vec::new();
    for (kind, x, y) in motion(events) {
        match kind {
            "travel" => loops.push(vec![(x, y)]),
            "cut" => loops.last_mut().unwrap().push((x, y)),
            _ => {}
        }
    }
    loops
}

fn extent(points: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let f = |g: fn(f64, f64) -> f64, i: usize, init: f64| {
        points.iter().map(|p| if i == 0 { p.0 } else { p.1 }).fold(init, g)
    };
    (
        f(f64::min, 0, f64::INFINITY),
        f(f64::min, 1, f64::INFINITY),
        f(f64::max, 0, f64::NEG_INFINITY),
        f(f64::max, 1, f64::NEG_INFINITY),
    )
}

fn close(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    let d = 1e-4; // offset grid is tolerance / 1000 = 10 nm; 0.1 µm is far below any kerf
    (a.0 - b.0).abs() < d && (a.1 - b.1).abs() < d && (a.2 - b.2).abs() < d && (a.3 - b.3).abs() < d
}

#[test]
fn kerf_offsets_outer_contours_outward_and_holes_inward() {
    // Plate 10..110 with a hole 80..90 (one geometry), kerf 0.2 mm: the beam centre runs
    // 0.1 mm into the waste — outside the plate, inside the hole — so the part keeps 100 × 100
    // with a 10 × 10 hole.
    let (mut s, [plate, ..]) = nested_document();
    let mut draft = line_op("Snijden", 10.0, 10.0);
    draft.mode = OperationMode::Line { kerf: Kerf::width(mm(0.2)).unwrap() };
    let op = add_op(&mut s, draft, vec![plate]);
    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();

    let loops = cut_loops(plan.events());
    assert_eq!(loops.len(), 2, "{loops:?}");
    assert!(
        close(extent(&loops[0]), (80.1, 80.1, 89.9, 89.9)),
        "hole first, offset into the hole: {:?}",
        extent(&loops[0])
    );
    assert!(close(extent(&loops[1]), (9.9, 9.9, 110.1, 110.1)), "outer offset outward: {:?}", extent(&loops[1]));
    for l in &loops {
        assert_eq!(l.first(), l.last(), "kerf paths stay closed");
    }

    // The nominal size stays visible next to the planned path size.
    let report = burnburn_cam::preflight(&s.snapshot(), &plan, &profile());
    let size = report.sizes.iter().find(|c| c.source == SourceRef { geometry: plate, operation: op }).unwrap();
    assert_eq!(size.nominal_mm, (100.0, 100.0));
    assert!((size.planned_mm.0 - 100.2).abs() < 1e-4 && (size.planned_mm.1 - 100.2).abs() < 1e-4, "{size:?}");
}

#[test]
fn kerf_on_an_open_contour_is_rejected_not_ignored() {
    let mut s = session();
    let open = burnburn_geometry::PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(10.0, 0.0))
        .build()
        .unwrap();
    let g = run(
        &mut s,
        CommandPayload::Edit(burnburn_document::GeometryCommand::CreatePath { path: open, name: String::new() }),
    )
    .changes
    .created[0];
    let mut draft = line_op("Snijden", 10.0, 10.0);
    draft.mode = OperationMode::Line { kerf: Kerf::width(mm(0.2)).unwrap() };
    let op = add_op(&mut s, draft, vec![g]);
    let err = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap_err();
    assert_eq!(err, CompileError::KerfOnOpenContour { operation: op, geometry: g }, "an open path has no side");
}

#[test]
fn cancelled_compile_returns_cancelled_not_a_partial_plan() {
    let (mut s, [plate, small, far]) = nested_document();
    add_op(&mut s, line_op("Snijden", 20.0, 80.0), vec![plate, small, far]);
    let source = CancellationSource::new();
    source.cancel();
    let err = compile(&s.snapshot(), &profile(), &options(), &source.token()).unwrap_err();
    assert_eq!(err, CompileError::Cancelled);
}

#[test]
fn origin_corner_and_anchored_placement_map_to_machine_coordinates() {
    let mut s = session();
    let g = rectangle(&mut s, 100.0, 50.0, 30.0, 15.0);
    add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![g]);

    // Top-right origin (typical Ruida): x' = 1500 − x, y' = 900 − y.
    let plan =
        compile(&s.snapshot(), &profile_with(OriginCorner::TopRight), &options(), &CancellationToken::never()).unwrap();
    let b = plan.bounds().unwrap();
    assert_eq!((b.min().x, b.min().y, b.max().x, b.max().y), (1370.0, 835.0, 1400.0, 850.0));

    // Anchored: the job's top-left corner goes to (10, 890) in the bed view.
    let mut o = options();
    o.placement = Placement::Anchored { anchor: JobAnchor::TopLeft, at: Point::new(10.0, 890.0) };
    let plan = compile(&s.snapshot(), &profile(), &o, &CancellationToken::never()).unwrap();
    let b = plan.bounds().unwrap();
    assert_eq!((b.min().x, b.min().y, b.max().x, b.max().y), (10.0, 875.0, 40.0, 890.0));

    // A known head position changes only the entry point, not the placement.
    let mut o = options();
    o.start = StartPoint::Bed(Point::new(200.0, 200.0));
    let plan = compile(&s.snapshot(), &profile(), &o, &CancellationToken::never()).unwrap();
    assert_eq!(travels(plan.events())[0], (130.0, 65.0), "entry at the corner nearest (200, 200)");
}

#[test]
fn passes_order_constraints_and_disabled_operations_shape_the_plan() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let mut twice = line_op("Snijden", 10.0, 50.0);
    twice.settings.passes = Passes::new(2).unwrap();
    let cut = add_op(&mut s, twice, vec![g]);
    let mark = add_op(&mut s, fill_op("Markeren", 1.0, 0.0), vec![g]);
    let mut off = line_op("Uit", 10.0, 50.0);
    off.output_enabled = false;
    add_op(&mut s, off, vec![g]);
    run(&mut s, CommandPayload::Process(ProcessCommand::AddOrderConstraint { before: mark, after: cut }));

    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    let boundaries: Vec<_> = plan
        .events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::OperationBoundary { operation, pass, .. } => Some((operation, pass)),
            _ => None,
        })
        .collect();
    assert_eq!(boundaries, vec![(mark, 0), (cut, 0), (cut, 1)], "mark first (constraint), cut twice, disabled skipped");
}

#[test]
fn profile_content_change_changes_plan_id() {
    let mut s = session();
    let g = rectangle(&mut s, 10.0, 10.0, 10.0, 10.0);
    add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![g]);
    let a = profile();
    // Same id and display revision, other content (another travel speed).
    let b = burnburn_job::MachineProfile::new(
        a.revision().id.clone(),
        a.revision().revision,
        *a.frame(),
        a.max_speed_mm_s(),
        a.travel_speed_mm_s() + 1.0,
        a.power_range().0,
        a.power_range().1,
    )
    .unwrap();
    assert_ne!(a.revision().content, b.revision().content);
    let pa = compile(&s.snapshot(), &a, &options(), &CancellationToken::never()).unwrap();
    let pb = compile(&s.snapshot(), &b, &options(), &CancellationToken::never()).unwrap();
    assert_ne!(pa.id(), pb.id(), "a plan compiled for other profile content is another plan");
}
