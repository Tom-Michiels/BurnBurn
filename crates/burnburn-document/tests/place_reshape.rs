//! W2-L2: Dock, Move Together, Warp 4 points, Deform 16 points and Resize Slots as document
//! commands. Each is one transaction and one history entry; a failing command changes nothing;
//! undo restores content and identity.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::{
    CommandError, CommandPayload, DocumentCommand, DocumentSession, EditError, EditLabel, GeometryCommand, MoveAnchor,
    PlaceCommand, PlaceUnits, ReshapeCommand, ShapeCommand, UndoLabel,
};
use burnburn_geometry::{
    Bounds, DeformGrid, Direction, DockContact, DockDirection, DockError, ParametricShape, Path, Point, PointMap,
    ProductionTolerance, QuadWarp, Rect, ResizeSlots, Shape, ShapeParams, SlotSelect, SlotsError, TogetherError,
    WarpError, WarpKind,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn tol() -> ProductionTolerance {
    ProductionTolerance::new(mm(0.001)).unwrap()
}

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

fn edit(s: &mut DocumentSession, c: GeometryCommand) -> Result<burnburn_document::CommandOutcome, CommandError> {
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(7),
        payload: CommandPayload::Edit(c),
    };
    s.apply(cmd)
}

fn rect(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64) -> GeometryId {
    let shape = Shape::Rectangle(Rect::new(p(x, y), mm(w), mm(h)).unwrap());
    edit(s, GeometryCommand::CreateShape { shape, name: "r".into() }).unwrap().changes.created[0]
}

fn polygon(s: &mut DocumentSession, pts: &[(f64, f64)]) -> GeometryId {
    let points = pts.iter().map(|&(x, y)| p(x, y)).collect();
    let shape = Shape::Polyline { points, closed: true };
    edit(s, GeometryCommand::CreateShape { shape, name: "poly".into() }).unwrap().changes.created[0]
}

fn bounds(s: &DocumentSession, id: GeometryId) -> Bounds {
    s.snapshot().object(id).unwrap().bounds().unwrap()
}

fn path_of(s: &DocumentSession, id: GeometryId) -> Path {
    s.snapshot().object(id).unwrap().path().as_ref().clone()
}

fn place(s: &mut DocumentSession, c: PlaceCommand) -> Result<burnburn_document::CommandOutcome, CommandError> {
    edit(s, GeometryCommand::Place(c))
}

fn reshape(s: &mut DocumentSession, c: ReshapeCommand) -> Result<burnburn_document::CommandOutcome, CommandError> {
    edit(s, GeometryCommand::Reshape(c))
}

fn dock(targets: &[GeometryId], reference: &[GeometryId], units: PlaceUnits) -> PlaceCommand {
    PlaceCommand::Dock {
        targets: targets.to_vec(),
        reference: reference.to_vec(),
        direction: DockDirection::Right,
        contact: DockContact::Geometry,
        gap: mm(0.0),
        units,
        tolerance: tol(),
    }
}

fn together(targets: &[GeometryId], anchor: MoveAnchor, units: PlaceUnits) -> PlaceCommand {
    PlaceCommand::MoveTogether {
        targets: targets.to_vec(),
        direction: Direction::Horizontal,
        anchor,
        gap: mm(0.0),
        units,
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn dock_is_one_undoable_translation_that_keeps_identity_and_the_reference() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 4.0, 4.0);
    let wall = rect(&mut s, 20.0, 0.0, 2.0, 10.0);
    let (before, wall_before) = (path_of(&s, a), path_of(&s, wall));
    let out = place(&mut s, dock(&[a], &[wall], PlaceUnits::Individual)).unwrap();
    assert_eq!(out.changes.modified, vec![a]);
    assert!(out.changes.created.is_empty() && out.changes.deleted.is_empty());
    assert!(near(bounds(&s, a).max().x, 20.0) && near(bounds(&s, a).min().y, 0.0));
    assert_eq!(path_of(&s, wall), wall_before);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::Dock)));
    s.undo(s.revision()).unwrap();
    assert_eq!(path_of(&s, a), before, "undo restores the exact outline");
}

#[test]
fn dock_moves_each_target_or_each_group_as_chosen() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 4.0, 4.0);
    let b = rect(&mut s, 6.0, 0.0, 4.0, 4.0);
    let wall = rect(&mut s, 30.0, 0.0, 2.0, 10.0);
    let group = edit(&mut s, GeometryCommand::Group { targets: vec![a, b] }).unwrap().changes.groups[0];
    let rev = s.revision();

    // Individually both reach the wall.
    place(&mut s, dock(&[a, b], &[wall], PlaceUnits::Individual)).unwrap();
    assert!(near(bounds(&s, a).max().x, 30.0) && near(bounds(&s, b).max().x, 30.0));
    s.undo(s.revision()).unwrap();
    assert_ne!(s.revision(), rev, "an undo is a new revision");

    // As a group the front member decides and the spacing inside the group is kept.
    place(&mut s, dock(&[a, b], &[wall], PlaceUnits::ByGroup)).unwrap();
    assert!(near(bounds(&s, b).max().x, 30.0));
    assert!(near(bounds(&s, b).min().x - bounds(&s, a).max().x, 2.0), "the 2 mm between a and b stays");
    assert_eq!(s.snapshot().object(a).unwrap().groups(), &[group]);
    // Objects that are not targets never move with a group.
    s.undo(s.revision()).unwrap();
    place(&mut s, dock(&[b], &[wall], PlaceUnits::ByGroup)).unwrap();
    assert!(near(bounds(&s, a).min().x, 0.0));
}

#[test]
fn a_dock_that_cannot_complete_changes_nothing() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 4.0, 4.0);
    let beside = rect(&mut s, 0.0, 20.0, 4.0, 4.0);
    let wall = rect(&mut s, 30.0, 0.0, 2.0, 10.0);
    let rev = s.revision();
    let before = (path_of(&s, a), path_of(&s, beside));
    // One of two targets has no wall in front of it: the whole command fails.
    let r = place(&mut s, dock(&[a, beside], &[wall], PlaceUnits::Individual));
    assert_eq!(r, Err(CommandError::Edit(EditError::Dock(DockError::NoContact))));
    assert_eq!((s.revision(), path_of(&s, a), path_of(&s, beside)), (rev, before.0, before.1));

    assert_eq!(
        place(&mut s, dock(&[a, wall], &[wall], PlaceUnits::Individual)),
        Err(CommandError::Edit(EditError::ReferenceInTargets(wall)))
    );
    assert_eq!(
        place(&mut s, dock(&[a], &[], PlaceUnits::Individual)),
        Err(CommandError::Edit(EditError::NotEnoughTargets))
    );
    let ghost = GeometryId::from_raw(999);
    assert_eq!(
        place(&mut s, dock(&[a, ghost], &[wall], PlaceUnits::Individual)),
        Err(CommandError::Edit(EditError::UnknownGeometry(ghost)))
    );
    // Docking an object that already touches changes nothing and leaves no history entry.
    place(&mut s, dock(&[a], &[wall], PlaceUnits::Individual)).unwrap();
    assert_eq!(
        place(&mut s, dock(&[a], &[wall], PlaceUnits::Individual)),
        Err(CommandError::Edit(EditError::NothingChanged))
    );
}

#[test]
fn dock_by_bounds_stops_at_the_box_and_by_geometry_enters_a_notch() {
    let mut s = session();
    let tri = polygon(&mut s, &[(0.0, 4.0), (8.0, 5.0), (0.0, 6.0)]);
    let block =
        polygon(&mut s, &[(20.0, 0.0), (20.0, 3.0), (30.0, 5.0), (20.0, 7.0), (20.0, 10.0), (40.0, 10.0), (40.0, 0.0)]);
    let mut by_bounds = dock(&[tri], &[block], PlaceUnits::Individual);
    if let PlaceCommand::Dock { contact, .. } = &mut by_bounds {
        *contact = DockContact::Bounds;
    }
    place(&mut s, by_bounds).unwrap();
    assert!(near(bounds(&s, tri).max().x, 20.0));
    s.undo(s.revision()).unwrap();
    place(&mut s, dock(&[tri], &[block], PlaceUnits::Individual)).unwrap();
    assert!(near(bounds(&s, tri).max().x, 30.0));
}

#[test]
fn move_together_closes_the_gaps_towards_the_chosen_anchor_in_one_entry() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 2.0, 2.0);
    let b = rect(&mut s, 5.0, 1.0, 3.0, 2.0);
    let c = rect(&mut s, 20.0, 0.0, 1.0, 2.0);
    let out = place(&mut s, together(&[c, a, b], MoveAnchor::Start, PlaceUnits::Individual)).unwrap();
    assert_eq!(out.changes.modified.len(), 2, "the anchor does not move");
    assert!(near(bounds(&s, a).min().x, 0.0) && near(bounds(&s, b).min().x, 2.0) && near(bounds(&s, c).min().x, 5.0));
    assert!(near(bounds(&s, b).min().y, 1.0), "only the chosen axis changes");
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::MoveTogether)));
    s.undo(s.revision()).unwrap();
    assert!(near(bounds(&s, c).min().x, 20.0));

    // The middle object as the anchor; the selection order does not matter.
    for order in [[a, b, c], [c, b, a], [b, c, a]] {
        place(&mut s, together(&order, MoveAnchor::Object(b), PlaceUnits::Individual)).unwrap();
        assert!(
            near(bounds(&s, b).min().x, 5.0) && near(bounds(&s, a).max().x, 5.0) && near(bounds(&s, c).min().x, 8.0)
        );
        s.undo(s.revision()).unwrap();
    }
}

#[test]
fn move_together_moves_groups_as_units_and_validates_its_input() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 2.0, 2.0);
    let b = rect(&mut s, 4.0, 0.0, 2.0, 2.0);
    let c = rect(&mut s, 30.0, 0.0, 2.0, 2.0);
    edit(&mut s, GeometryCommand::Group { targets: vec![b, c] }).unwrap();
    // As units: {b, c} spans 4..32 and moves next to a; its inner gap stays.
    place(&mut s, together(&[a, b, c], MoveAnchor::Start, PlaceUnits::ByGroup)).unwrap();
    assert!(near(bounds(&s, b).min().x, 2.0) && near(bounds(&s, c).min().x, 28.0));
    s.undo(s.revision()).unwrap();

    let rev = s.revision();
    assert_eq!(
        place(&mut s, together(&[a], MoveAnchor::Start, PlaceUnits::Individual)),
        Err(CommandError::Edit(EditError::Together(TogetherError::TooFew)))
    );
    assert_eq!(
        place(&mut s, together(&[b, c], MoveAnchor::Start, PlaceUnits::ByGroup)),
        Err(CommandError::Edit(EditError::Together(TogetherError::TooFew))),
        "one group is one unit"
    );
    assert_eq!(
        place(&mut s, together(&[a, b], MoveAnchor::Object(c), PlaceUnits::Individual)),
        Err(CommandError::Edit(EditError::ReferenceNotInTargets(c)))
    );
    // Already together: no change, no entry.
    place(&mut s, together(&[a, b], MoveAnchor::Start, PlaceUnits::Individual)).unwrap();
    assert_eq!(
        place(&mut s, together(&[a, b], MoveAnchor::Start, PlaceUnits::Individual)),
        Err(CommandError::Edit(EditError::NothingChanged))
    );
    assert_ne!(s.revision(), rev);
}

fn corners() -> [Point; 4] {
    [p(5.0, -3.0), p(120.0, 10.0), p(90.0, 70.0), p(-10.0, 40.0)]
}

/// A five-pointed parametric star fitted to the given box.
fn star(s: &mut DocumentSession, b: Bounds) -> GeometryId {
    let shape = ParametricShape::fit(ShapeParams::Star { points: 5, ratio: 0.4 }, b).unwrap();
    edit_payload(s, CommandPayload::Shape(ShapeCommand::Create { shape, name: "star".into() })).changes.created[0]
}

fn edit_payload(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    s.apply(DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(7), payload })
        .unwrap()
}

fn vertices(path: &Path) -> Vec<Point> {
    let sp = &path.subpaths()[0];
    std::iter::once(sp.start()).chain(sp.segments().iter().map(burnburn_geometry::Segment::end)).collect()
}

#[test]
fn warp4_maps_the_common_box_of_all_targets_in_one_step() {
    let mut s = session();
    let box_a = Bounds::from_corners(p(0.0, 0.0), p(40.0, 50.0));
    let box_b = Bounds::from_corners(p(60.0, 0.0), p(100.0, 50.0));
    let (a, b) = (star(&mut s, box_a), star(&mut s, box_b));
    assert!(s.snapshot().object(a).unwrap().parametric().is_some());
    let before = (path_of(&s, a), path_of(&s, b));
    let common = bounds(&s, a).union(bounds(&s, b));
    let out = reshape(
        &mut s,
        ReshapeCommand::Warp4 { targets: vec![a, b], corners: corners(), kind: WarpKind::Projective, tolerance: tol() },
    )
    .unwrap();
    assert_eq!(out.changes.modified.len(), 2);
    // Every vertex went where one projective map of the common box puts it; straight edges stay lines.
    let map = QuadWarp::new(common, corners(), WarpKind::Projective).unwrap();
    for (id, original) in [(a, &before.0), (b, &before.1)] {
        let warped = path_of(&s, id);
        let (from, to) = (vertices(original), vertices(&warped));
        assert_eq!(from.len(), to.len());
        for (f, t) in from.iter().zip(&to) {
            assert!(map.eval(*f).unwrap().distance(*t) < 1e-9, "{f:?} -> {t:?}");
        }
    }
    assert!(s.snapshot().object(a).unwrap().parametric().is_none(), "a warped star is a plain path");
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::Warp)));
    s.undo(s.revision()).unwrap();
    assert_eq!((path_of(&s, a), path_of(&s, b)), before);
    assert!(s.snapshot().object(a).unwrap().parametric().is_some(), "undo restores the parametric form");
}

#[test]
fn warp4_and_deform16_report_why_they_cannot_run_and_change_nothing() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 40.0, 50.0);
    let line = polygon(&mut s, &[(0.0, 0.0), (0.0, 10.0), (0.0, 20.0)]);
    let rev = s.revision();
    let dart = [p(0.0, 0.0), p(100.0, 0.0), p(40.0, 10.0), p(0.0, 50.0)];
    assert_eq!(
        reshape(
            &mut s,
            ReshapeCommand::Warp4 { targets: vec![a], corners: dart, kind: WarpKind::Bilinear, tolerance: tol() }
        ),
        Err(CommandError::Edit(EditError::Warp(WarpError::NotConvex)))
    );
    assert_eq!(
        reshape(
            &mut s,
            ReshapeCommand::Warp4 {
                targets: vec![line],
                corners: corners(),
                kind: WarpKind::Bilinear,
                tolerance: tol()
            }
        ),
        Err(CommandError::Edit(EditError::Warp(WarpError::DegenerateSource)))
    );
    let mut net = *DeformGrid::identity(bounds(&s, a)).net();
    net[1][1] = p(1000.0, 1000.0);
    assert_eq!(
        reshape(&mut s, ReshapeCommand::Deform16 { targets: vec![a], net: Box::new(net), tolerance: tol() }),
        Err(CommandError::Edit(EditError::Warp(WarpError::Folded)))
    );
    assert_eq!(
        reshape(
            &mut s,
            ReshapeCommand::Warp4 { targets: vec![], corners: corners(), kind: WarpKind::Bilinear, tolerance: tol() }
        ),
        Err(CommandError::Edit(EditError::NotEnoughTargets))
    );
    assert_eq!(s.revision(), rev);
}

#[test]
fn deform16_moves_the_interior_of_the_shape_and_undo_restores_it() {
    let mut s = session();
    let a = star(&mut s, Bounds::from_corners(p(0.0, 0.0), p(90.0, 60.0)));
    let mut net = *DeformGrid::identity(bounds(&s, a)).net();
    net[1][1] = p(40.0, 40.0);
    net[2][2] = p(50.0, 10.0);
    let before = path_of(&s, a);
    reshape(&mut s, ReshapeCommand::Deform16 { targets: vec![a], net: Box::new(net), tolerance: tol() }).unwrap();
    let after = path_of(&s, a);
    assert_ne!(after, before);
    assert_eq!(after.subpaths().len(), 1);
    assert!(after.subpaths()[0].is_closed());
    // The map interpolates the corner control points; the first star tip is mapped like any other point.
    let map = DeformGrid::new(bounds_of(&before), net).unwrap();
    assert!(map.eval(before.subpaths()[0].start()).unwrap().distance(after.subpaths()[0].start()) < 1e-9);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::Deform)));
    s.undo(s.revision()).unwrap();
    assert_eq!(path_of(&s, a), before);
    assert!(s.snapshot().object(a).unwrap().parametric().is_some());
}

fn bounds_of(path: &Path) -> Bounds {
    path.bounds().unwrap()
}

fn notched_panel(s: &mut DocumentSession, slot: f64) -> GeometryId {
    polygon(
        s,
        &[
            (0.0, 0.0),
            (60.0, 0.0),
            (60.0, 30.0),
            (28.0 + slot, 30.0),
            (28.0 + slot, 26.0),
            (28.0, 26.0),
            (28.0, 30.0),
            (0.0, 30.0),
        ],
    )
}

fn spec(from: f64, to: f64) -> ResizeSlots {
    ResizeSlots { from_width: from, to_width: to, match_tolerance: 0.01, select: SlotSelect::Slots }
}

#[test]
fn resize_slots_changes_only_targets_that_have_a_matching_slot() {
    let mut s = session();
    let three = notched_panel(&mut s, 3.0);
    let five = notched_panel(&mut s, 5.0);
    let before_five = path_of(&s, five);
    let area = |s: &DocumentSession, id| path_of(s, id).signed_area();
    let out =
        reshape(&mut s, ReshapeCommand::ResizeSlots { targets: vec![three, five], spec: spec(3.0, 3.6) }).unwrap();
    assert_eq!(out.changes.modified, vec![three]);
    assert!(near(area(&s, three), 1800.0 - 3.6 * 4.0));
    assert_eq!(path_of(&s, five), before_five);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::ResizeSlots)));
    s.undo(s.revision()).unwrap();
    assert!(near(area(&s, three), 1800.0 - 3.0 * 4.0));

    let rev = s.revision();
    assert_eq!(
        reshape(&mut s, ReshapeCommand::ResizeSlots { targets: vec![five], spec: spec(3.0, 3.6) }),
        Err(CommandError::Edit(EditError::Slots(SlotsError::NoSlotsFound)))
    );
    assert_eq!(
        reshape(&mut s, ReshapeCommand::ResizeSlots { targets: vec![five, three], spec: spec(2.0, 3.6) }),
        Err(CommandError::Edit(EditError::NothingChanged))
    );
    assert_eq!(s.revision(), rev);
}

#[test]
fn a_collision_in_one_target_leaves_the_others_untouched() {
    let mut s = session();
    let fine = notched_panel(&mut s, 3.0);
    // Two slots 1 mm apart that cannot both grow to 5 mm.
    let tight = polygon(
        &mut s,
        &[
            (0.0, 0.0),
            (60.0, 0.0),
            (60.0, 30.0),
            (17.0, 30.0),
            (17.0, 26.0),
            (14.0, 26.0),
            (14.0, 30.0),
            (13.0, 30.0),
            (13.0, 26.0),
            (10.0, 26.0),
            (10.0, 30.0),
            (0.0, 30.0),
        ],
    );
    let rev = s.revision();
    let before = path_of(&s, fine);
    assert_eq!(
        reshape(&mut s, ReshapeCommand::ResizeSlots { targets: vec![fine, tight], spec: spec(3.0, 5.0) }),
        Err(CommandError::Edit(EditError::Slots(SlotsError::Collision)))
    );
    assert_eq!((s.revision(), path_of(&s, fine)), (rev, before));
}
