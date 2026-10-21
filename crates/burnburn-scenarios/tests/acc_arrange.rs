//! Acceptance plan, arrange block: AR01 (align / distribute / dock), AR02 (arrays) and AR03
//! (nesting). Align goes through the harness commands; distribute-by-centres, dock and arrays use
//! the document/geometry public commands because the UI has no route for them yet. Expected
//! values are computed by hand from the boxes drawn here.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod acc_rig;

use acc_rig::{bounds_of, opened_with, select, view};
use burnburn_cam::nest::{
    NestParams, NestPart, PartKey, RotationSet, Stock, StockId, UnplacedReason, UtilisationBasis, nest,
};
use burnburn_document::structure::{EvalMode, Modifier, ModifierChain, ModifierStep, StructureCommand};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession, GeometryCommand};
use burnburn_geometry::array::{CircularArray, GridArray};
use burnburn_geometry::{
    AlignEdge, AlignTo, BooleanOp, BooleanOptions, Bounds, Direction, DockContact, DockDirection, FillRule, OffsetJoin,
    Path, Point, ProductionTolerance, Rect, Spacing, Vector, align_offsets, boolean, distribute_offsets, dock_offset,
    offset,
};
use burnburn_presentation::CommandKey;
use burnburn_types::{CancellationToken, DocumentId, GeometryId, Length, RequestId, StableId};
use proptest::prelude::*;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap())
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

// ---------------------------------------------------------------------------------------------
// AR01 align by centre / edge, distribute by centres / gaps, dock
// ---------------------------------------------------------------------------------------------

/// A 10x10 at the origin (the reference, selected first), a 20x10 at (30, 5) and a 6x6 at (70, 20).
fn three() -> (burnburn_presentation::HeadlessEditorHarness, DocumentId, Vec<GeometryId>) {
    opened_with(vec![rect(0.0, 0.0, 10.0, 10.0), rect(30.0, 5.0, 20.0, 10.0), rect(70.0, 20.0, 6.0, 6.0)])
}

#[test]
fn ar01_each_align_command_lines_up_its_edge_with_the_first_selected_and_keeps_sizes() {
    // (command, which value of the bounds is compared, expected value of the reference)
    type Pick = fn((f64, f64, f64, f64)) -> f64;
    let cases: [(CommandKey, Pick, f64); 6] = [
        (CommandKey::AlignLeft, |b| b.0, 0.0),
        (CommandKey::AlignRight, |b| b.2, 10.0),
        (CommandKey::AlignCenterX, |b| (b.0 + b.2) / 2.0, 5.0),
        (CommandKey::AlignBottom, |b| b.1, 0.0),
        (CommandKey::AlignTop, |b| b.3, 10.0),
        (CommandKey::AlignCenterY, |b| (b.1 + b.3) / 2.0, 5.0),
    ];
    for (command, pick, expected) in cases {
        let (mut h, doc, ids) = three();
        let sizes: Vec<(f64, f64)> = ids
            .iter()
            .map(|id| {
                let b = bounds_of(&h, doc, *id);
                (b.2 - b.0, b.3 - b.1)
            })
            .collect();
        select(&mut h, doc, &ids);
        h.invoke(doc, command);
        for (k, id) in ids.iter().enumerate() {
            let b = bounds_of(&h, doc, *id);
            assert!(near(pick(b), expected), "{command:?}: object {k} is at {} not {expected}", pick(b));
            assert!(near(b.2 - b.0, sizes[k].0) && near(b.3 - b.1, sizes[k].1), "{command:?}: size changed");
        }
        assert!(view(&h, doc).undo.is_some());
        h.invoke(doc, CommandKey::Undo);
        assert!(near(bounds_of(&h, doc, ids[2]).0, 70.0), "{command:?}: one undo restores");
    }
}

#[test]
fn ar01_distribute_horizontally_leaves_equal_gaps_and_the_outer_objects_in_place() {
    let (mut h, doc, ids) = three();
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::DistributeHorizontal);
    let b: Vec<_> = ids.iter().map(|id| bounds_of(&h, doc, *id)).collect();
    // Outer boxes stay: 0..10 and 70..76; the 20 mm box sits so both gaps are (76 - 36) / 2 = 20 ... widths 10+20+6=36.
    assert!(near(b[0].0, 0.0) && near(b[2].2, 76.0));
    let gap1 = b[1].0 - b[0].2;
    let gap2 = b[2].0 - b[1].2;
    assert!(near(gap1, gap2) && near(gap1, (76.0 - 36.0) / 2.0), "gaps {gap1} / {gap2}");
}

#[test]
fn ar01_distribute_by_centres_spaces_the_centres_evenly_unlike_the_gap_mode() {
    let boxes: Vec<Bounds> = [rect(0.0, 0.0, 10.0, 10.0), rect(30.0, 0.0, 20.0, 10.0), rect(70.0, 0.0, 6.0, 10.0)]
        .iter()
        .map(|p| p.bounds().unwrap())
        .collect();
    let c = distribute_offsets(&boxes, Direction::Horizontal, Spacing::Centers).unwrap();
    // Centres 5, 40, 73 -> the middle one goes to (5 + 73) / 2 = 39.
    assert!(near(c[0].x, 0.0) && near(c[2].x, 0.0) && near(c[1].x, -1.0), "{c:?}");
    let g = distribute_offsets(&boxes, Direction::Horizontal, Spacing::Gaps).unwrap();
    assert!(near(g[1].x, 0.0), "the middle box already has gaps 20 and 20: {g:?}");
    // Align by centre against an explicit reference object.
    let a = align_offsets(&boxes, AlignEdge::CenterX, AlignTo::Object(1)).unwrap();
    assert!(near(a[0].x, 35.0) && near(a[1].x, 0.0) && near(a[2].x, -33.0), "{a:?}");
}

#[test]
fn ar01_dock_slides_to_the_reference_edge_and_keeps_the_requested_gap() {
    let tol = ProductionTolerance::new(mm(0.01)).unwrap();
    let reference = rect(0.0, 0.0, 20.0, 30.0);
    let moving = rect(50.0, 5.0, 10.0, 10.0);
    for (gap, dx) in [(0.0, -30.0), (2.0, -28.0)] {
        let v = dock_offset(&[&moving], &[&reference], DockDirection::Left, DockContact::Bounds, gap, &tol).unwrap();
        assert!(near(v.x, dx) && near(v.y, 0.0), "gap {gap}: {v:?}");
    }
    let up = dock_offset(
        &[&rect(5.0, 100.0, 4.0, 4.0)],
        &[&reference],
        DockDirection::Down,
        DockContact::Geometry,
        0.0,
        &tol,
    )
    .unwrap();
    assert!(near(up.y, -70.0), "down onto the 30 mm tall reference: {up:?}");
}

// ---------------------------------------------------------------------------------------------
// AR02 grid / circular / linear arrays with compound sources, text sources
// ---------------------------------------------------------------------------------------------

fn run(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn chain(source: GeometryId, modifier: Modifier) -> ModifierChain {
    ModifierChain {
        name: "Reeks".into(),
        source,
        steps: vec![ModifierStep { modifier, enabled: true }],
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: EvalMode::Inline,
    }
}

#[test]
fn ar02_grid_array_has_count_spacing_and_offsets_of_a_two_part_source() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    // A compound source of two squares (a "group" of outlines in one object).
    let mut subs = rect(0.0, 0.0, 10.0, 10.0).subpaths().to_vec();
    subs.extend(rect(14.0, 0.0, 4.0, 4.0).subpaths().iter().cloned());
    let src = run(
        &mut s,
        CommandPayload::Edit(GeometryCommand::CreatePath { path: Path::from_subpaths(subs), name: "Bron".into() }),
    )
    .changes
    .created[0];
    let grid = Modifier::GridArray(GridArray { columns: 4, rows: 3, pitch_x: mm(25.0), pitch_y: mm(18.0) });
    let out = run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain: chain(src, grid) }));
    let snap = s.snapshot();
    let obj = snap.object(out.changes.created[0]).unwrap();
    assert_eq!(obj.path().subpaths().len(), 4 * 3 * 2, "12 copies of 2 contours");
    // Every contour is at its analytic place: copy (i, j), origin (25 i, 18 j), second part +14.
    let mut starts: Vec<(i64, i64)> = obj
        .path()
        .subpaths()
        .iter()
        .map(|sp| ((sp.start().x * 1000.0).round() as i64, (sp.start().y * 1000.0).round() as i64))
        .collect();
    starts.sort();
    let mut expected = Vec::new();
    for j in 0..3 {
        for i in 0..4 {
            for dx in [0.0, 14.0] {
                expected.push((((25.0 * f64::from(i) + dx) * 1000.0) as i64, ((18.0 * f64::from(j)) * 1000.0) as i64));
            }
        }
    }
    expected.sort();
    assert_eq!(starts, expected);
    let b = obj.bounds().unwrap();
    assert_eq!((b.max().x, b.max().y), (75.0 + 18.0, 36.0 + 10.0));
}

#[test]
fn ar02_circular_array_orients_every_copy_by_its_own_angle_and_a_partial_sweep_includes_both_ends() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let src = run(
        &mut s,
        CommandPayload::Edit(GeometryCommand::CreatePath { path: rect(60.0, 50.0, 6.0, 2.0), name: "Tand".into() }),
    )
    .changes
    .created[0];
    let centre = Point::new(50.0, 50.0);
    for (count, sweep, step_deg) in [(6_u32, 360.0, 60.0), (4, 90.0, 30.0)] {
        let circ = Modifier::CircularArray(CircularArray {
            count,
            center: centre,
            sweep_deg: sweep,
            rotate_items: true,
            reference: centre,
        });
        let out = run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain: chain(src, circ) }));
        let snap = s.snapshot();
        let obj = snap.object(out.changes.created[0]).unwrap();
        assert_eq!(obj.path().subpaths().len(), count as usize);
        for (k, sp) in obj.path().subpaths().iter().enumerate() {
            let a = (step_deg * k as f64).to_radians();
            let want = {
                let p = Point::new(10.0, 0.0); // source start relative to the centre
                Point::new(50.0 + p.x * a.cos() - p.y * a.sin(), 50.0 + p.x * a.sin() + p.y * a.cos())
            };
            assert!(
                sp.start().distance(want) < 1e-9,
                "count {count}: copy {k} starts at {:?}, want {want:?}",
                sp.start()
            );
        }
    }
}

#[test]
fn ar02_invalid_counts_are_refused_and_change_nothing() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let src = run(
        &mut s,
        CommandPayload::Edit(GeometryCommand::CreatePath { path: rect(0.0, 0.0, 5.0, 5.0), name: "x".into() }),
    )
    .changes
    .created[0];
    let rev = s.revision();
    for bad in [
        Modifier::GridArray(GridArray { columns: 0, rows: 3, pitch_x: mm(1.0), pitch_y: mm(1.0) }),
        Modifier::LinearArray { count: 0, step: Vector::new(1.0, 0.0) },
    ] {
        let cmd = DocumentCommand {
            document: s.id(),
            expected: s.revision(),
            request: RequestId::from_raw(2),
            payload: CommandPayload::Structure(StructureCommand::CreateModifier { chain: chain(src, bad) }),
        };
        assert!(s.apply(cmd).is_err());
    }
    assert_eq!(s.revision(), rev);
}

// ---------------------------------------------------------------------------------------------
// AR03 nesting with holes, locked rotations and padding
// ---------------------------------------------------------------------------------------------

fn l_shape(w: f64, h: f64, t: f64) -> Path {
    let pts = [(0.0, 0.0), (w, 0.0), (w, t), (t, t), (t, h), (0.0, h)];
    Path::polyline(&pts.iter().map(|&(x, y)| Point::new(x, y)).collect::<Vec<_>>(), true).unwrap()
}

/// Square ring: outer `s`, inner hole `s - 2 t`, counter-clockwise outer and clockwise hole.
fn ring(s: f64, t: f64) -> Path {
    let hole = Path::polyline(
        &[(t, t), (t, s - t), (s - t, s - t), (s - t, t)].iter().map(|&(x, y)| Point::new(x, y)).collect::<Vec<_>>(),
        true,
    )
    .unwrap();
    let mut subs = rect(0.0, 0.0, s, s).subpaths().to_vec();
    subs.extend(hole.subpaths().iter().cloned());
    Path::from_subpaths(subs)
}

/// 200 x 120 sheet with a 40 x 40 hole at (80, 40).
fn sheet() -> Stock {
    let mut subs = rect(0.0, 0.0, 200.0, 120.0).subpaths().to_vec();
    let hole = Path::polyline(
        &[(80.0, 40.0), (80.0, 80.0), (120.0, 80.0), (120.0, 40.0)]
            .iter()
            .map(|&(x, y)| Point::new(x, y))
            .collect::<Vec<_>>(),
        true,
    )
    .unwrap();
    subs.extend(hole.subpaths().iter().cloned());
    Stock { id: StockId(1), outline: Path::from_subpaths(subs), keep_out: vec![], grain_deg: None }
}

fn params(spacing: f64, margin: f64) -> NestParams {
    NestParams {
        spacing: mm(spacing),
        edge_margin: mm(margin),
        step: mm(1.0),
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        max_candidates: 20_000_000,
        basis: UtilisationBasis::StockArea,
    }
}

fn bool_area(a: &Path, b: &Path, op: BooleanOp) -> f64 {
    let o = BooleanOptions { fill: FillRule::EvenOdd, tolerance: ProductionTolerance::new(mm(0.005)).unwrap() };
    boolean(a, b, op, &o, &CancellationToken::never()).unwrap().signed_area()
}

/// The invariants of a nest result, checked with an independent geometry oracle: no two placed
/// outlines come closer than `spacing`, every outline lies inside the sheet shrunk by `margin`
/// (so also clear of the sheet's hole), rotations stay in the part's set.
fn check_no_collisions(parts: &[NestPart], stock: &Stock, p: &NestParams, result: &burnburn_cam::nest::NestResult) {
    let slack = 0.06; // the nest works on a 0.01 mm chord polygon of a 1 mm grid
    let placed: Vec<(usize, Path)> = result
        .placements
        .iter()
        .map(|pl| {
            let part = parts.iter().find(|q| q.key == pl.part).unwrap();
            assert!(
                part.rotations.degrees().iter().any(|d| (d - pl.rotation_deg).abs() < 1e-9),
                "rotation outside set"
            );
            (pl.copy as usize, part.outline.transform(&pl.transform))
        })
        .collect();
    let usable = offset(
        &stock.outline,
        -(p.edge_margin.mm() - slack),
        OffsetJoin::Miter,
        &p.tolerance,
        &CancellationToken::never(),
    )
    .unwrap();
    for (i, (_, a)) in placed.iter().enumerate() {
        let outside = bool_area(a, &usable, BooleanOp::Difference);
        assert!(outside.abs() < 1e-3, "placement {i} sticks out of the usable sheet by {outside} mm2");
        let grown =
            offset(a, p.spacing.mm() - slack, OffsetJoin::Round, &p.tolerance, &CancellationToken::never()).unwrap();
        for (j, (_, b)) in placed.iter().enumerate().skip(i + 1) {
            let overlap = bool_area(&grown, b, BooleanOp::Intersection);
            assert!(overlap.abs() < 1e-3, "placements {i} and {j} are closer than the padding (overlap {overlap} mm2)");
        }
    }
}

fn nest_parts(a: u32, b: u32, c: u32, locked: bool) -> Vec<NestPart> {
    let rots = |all: bool| RotationSet::new(if all { vec![0.0, 90.0, 180.0, 270.0] } else { vec![0.0] }).unwrap();
    vec![
        NestPart {
            key: PartKey(1),
            outline: l_shape(40.0, 30.0, 10.0),
            count: a,
            rotations: rots(!locked),
            grain_deg: None,
        },
        NestPart { key: PartKey(2), outline: ring(30.0, 6.0), count: b, rotations: rots(!locked), grain_deg: None },
        NestPart {
            key: PartKey(3),
            outline: rect(0.0, 0.0, 14.0, 9.0),
            count: c,
            rotations: rots(false),
            grain_deg: None,
        },
    ]
}

#[test]
fn ar03_parts_with_holes_do_not_collide_report_what_did_not_fit_and_repeat_exactly() {
    let stock = sheet();
    let p = params(3.0, 2.0);
    // More small ring/L parts than can fit: the rest must be reported, not squeezed in.
    let parts = nest_parts(30, 30, 10, true);
    let cancel = CancellationToken::never();
    let r = nest(&parts, std::slice::from_ref(&stock), &p, &cancel).unwrap();
    check_no_collisions(&parts, &stock, &p, &r);
    let ordered: u32 = parts.iter().map(|q| q.count).sum();
    let unplaced: u32 = r.unplaced.iter().map(|u| u.count).sum();
    assert_eq!(r.placements.len() as u32 + unplaced, ordered, "every ordered copy is placed or reported");
    assert!(unplaced > 0 && r.unplaced.iter().all(|u| u.reason == UnplacedReason::NoFit));
    assert!(r.placements.iter().all(|pl| pl.rotation_deg == 0.0), "locked rotation = {{0}}");
    // Deterministic: the same input gives the identical result (no seed in play).
    let again = nest(&parts, std::slice::from_ref(&stock), &p, &cancel).unwrap();
    assert_eq!(r, again);
}

#[test]
fn ar03_a_small_part_may_sit_inside_the_hole_of_a_ring_but_never_touch_it() {
    // One ring (outer 60, hole 48) and a 20 x 20 square on a sheet barely larger than the ring:
    // the square can only go inside the ring's hole.
    let stock = Stock { id: StockId(1), outline: rect(0.0, 0.0, 66.0, 66.0), keep_out: vec![], grain_deg: None };
    let parts = vec![
        NestPart {
            key: PartKey(1),
            outline: ring(60.0, 6.0),
            count: 1,
            rotations: RotationSet::new(vec![0.0]).unwrap(),
            grain_deg: None,
        },
        NestPart {
            key: PartKey(2),
            outline: rect(0.0, 0.0, 20.0, 20.0),
            count: 1,
            rotations: RotationSet::new(vec![0.0]).unwrap(),
            grain_deg: None,
        },
    ];
    let p = params(3.0, 2.0);
    let r = nest(&parts, std::slice::from_ref(&stock), &p, &CancellationToken::never()).unwrap();
    assert_eq!(r.placements.len(), 2, "{r:?}");
    check_no_collisions(&parts, &stock, &p, &r);
    let sq = r.placements.iter().find(|pl| pl.part == PartKey(2)).unwrap();
    let placed = parts[1].outline.transform(&sq.transform).bounds().unwrap();
    assert!(placed.min().x > 6.0 && placed.max().x < 60.0 + 2.0 - 6.0, "inside the ring hole: {placed:?}");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, failure_persistence: None, ..ProptestConfig::default() })]

    /// Random orders over a sheet with a hole: whatever is placed never collides or leaves the
    /// usable area, with free or locked rotations, and the totals add up.
    #[test]
    fn ar03_property_no_collisions_and_complete_accounting(
        a in 0_u32..6, b in 0_u32..5, c in 0_u32..8, locked in any::<bool>(),
        spacing in 1.0_f64..6.0, margin in 1.0_f64..4.0,
    ) {
        let stock = sheet();
        let p = params(spacing, margin);
        let parts = nest_parts(a, b, c, locked);
        let r = nest(&parts, std::slice::from_ref(&stock), &p, &CancellationToken::never()).unwrap();
        check_no_collisions(&parts, &stock, &p, &r);
        let ordered: u32 = parts.iter().map(|q| q.count).sum();
        let unplaced: u32 = r.unplaced.iter().map(|u| u.count).sum();
        prop_assert_eq!(r.placements.len() as u32 + unplaced, ordered);
        if locked { prop_assert!(r.placements.iter().all(|pl| pl.rotation_deg == 0.0)); }
    }
}
