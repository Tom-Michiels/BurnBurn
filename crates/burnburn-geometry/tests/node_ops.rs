//! W1-L2a: node type, segment type, start point and direction. Oracles are hand-derived tangent
//! geometry and the invariants "outline unchanged" / "untouched spans bit-identical".

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use burnburn_geometry::{
    NodeEdit, NodeEditError, NodeKind, NodeRef, Path, PathBuilder, Point, Segment, SegmentKind, Winding,
};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn node(subpath: usize, node: usize) -> NodeRef {
    NodeRef { subpath, node }
}

fn near(a: Point, b: Point) -> bool {
    a.distance(b) < 1e-9
}

/// (0,0) → (10,0) → (10,10): two lines with a right-angle corner at node 1.
fn corner() -> Path {
    PathBuilder::new().move_to(p(0.0, 0.0)).line_to(p(10.0, 0.0)).line_to(p(10.0, 10.0)).build().unwrap()
}

/// Cubic wave (0,0) → (10,0) → (20,0): node 1 is a symmetric smooth node, handles (8,5)/(12,-5).
fn wave() -> Path {
    PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .cubic_to(p(2.0, 5.0), p(8.0, 5.0), p(10.0, 0.0))
        .cubic_to(p(12.0, -5.0), p(18.0, -5.0), p(20.0, 0.0))
        .build()
        .unwrap()
}

fn square(ccw: bool) -> Path {
    let pts = if ccw {
        [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)]
    } else {
        [p(0.0, 0.0), p(0.0, 10.0), p(10.0, 10.0), p(10.0, 0.0)]
    };
    Path::polyline(&pts, true).unwrap()
}

fn cubic(seg: &Segment) -> (Point, Point, Point) {
    match *seg {
        Segment::Cubic { c1, c2, to } => (c1, c2, to),
        other => panic!("expected a cubic, got {other:?}"),
    }
}

#[test]
fn smoothing_a_right_angle_corner_makes_collinear_handles_along_the_bisector() {
    let out = corner().edit_nodes(&NodeEdit::SetNodeKind { node: node(0, 1), kind: NodeKind::Smooth }).unwrap();
    let segs = out.subpaths()[0].segments();
    let (_, in_handle, to0) = cubic(&segs[0]);
    let (out_handle, _, to1) = cubic(&segs[1]);
    assert_eq!((to0, to1), (p(10.0, 0.0), p(10.0, 10.0)), "the nodes do not move");
    // Tangent at the corner = (1,0)+(0,1) normalised = (1,1)/√2; each handle is a third of its chord.
    let h = 10.0 / 3.0 / 2f64.sqrt();
    assert!(near(in_handle, p(10.0 - h, -h)), "{in_handle:?}");
    assert!(near(out_handle, p(10.0 + h, h)), "{out_handle:?}");
}

#[test]
fn smooth_handles_are_collinear_through_the_node_and_keep_their_lengths() {
    // Skew a smooth node: its handles (8,5) and (12,-5) are already collinear; make them not.
    let bent = wave()
        .edit_nodes(&NodeEdit::MoveHandle {
            handle: burnburn_geometry::HandleRef { node: node(0, 1), side: burnburn_geometry::HandleSide::Out },
            to: p(14.0, -2.0),
            mode: burnburn_geometry::HandleMode::Corner,
        })
        .unwrap();
    let out = bent.edit_nodes(&NodeEdit::SetNodeKind { node: node(0, 1), kind: NodeKind::Smooth }).unwrap();
    let segs = out.subpaths()[0].segments();
    let (_, c2, n) = cubic(&segs[0]);
    let (c1, _, _) = cubic(&segs[1]);
    let (a, b) = (n - c2, c1 - n);
    assert!(a.cross(b).abs() < 1e-9 && a.dot(b) > 0.0, "collinear and pointing the same way");
    let (len_in, len_out) = (5.0f64.hypot(2.0), 4.0f64.hypot(2.0));
    assert!((a.length() - len_in).abs() < 1e-9 && (b.length() - len_out).abs() < 1e-9, "lengths kept");
    assert_eq!(cubic(&segs[0]).0, p(2.0, 5.0), "the far handle of the neighbouring span did not move");
    assert_eq!(cubic(&segs[1]).1, p(18.0, -5.0));
}

#[test]
fn symmetric_makes_both_handles_equally_long() {
    let out = corner().edit_nodes(&NodeEdit::SetNodeKind { node: node(0, 1), kind: NodeKind::Symmetric }).unwrap();
    let segs = out.subpaths()[0].segments();
    let (_, c2, n) = cubic(&segs[0]);
    let (c1, _, _) = cubic(&segs[1]);
    assert!(((n - c2).length() - (c1 - n).length()).abs() < 1e-12);
    assert!((n - c2).cross(c1 - n).abs() < 1e-9);
}

#[test]
fn corner_retracts_the_handles_onto_the_node() {
    let out = wave().edit_nodes(&NodeEdit::SetNodeKind { node: node(0, 1), kind: NodeKind::Corner }).unwrap();
    let segs = out.subpaths()[0].segments();
    assert_eq!(segs[0], Segment::Cubic { c1: p(2.0, 5.0), c2: p(10.0, 0.0), to: p(10.0, 0.0) });
    assert_eq!(segs[1], Segment::Cubic { c1: p(10.0, 0.0), c2: p(18.0, -5.0), to: p(20.0, 0.0) });
}

#[test]
fn node_kind_changes_that_change_nothing_are_rejected() {
    // A corner between two lines is already a corner; an open end has no second tangent to match.
    let c = corner();
    assert_eq!(
        c.edit_nodes(&NodeEdit::SetNodeKind { node: node(0, 1), kind: NodeKind::Corner }),
        Err(NodeEditError::NoChange)
    );
    assert_eq!(
        c.edit_nodes(&NodeEdit::SetNodeKind { node: node(0, 0), kind: NodeKind::Smooth }),
        Err(NodeEditError::NoChange)
    );
    // Smoothing twice is a no-op the second time.
    let once = c.edit_nodes(&NodeEdit::SetNodeKind { node: node(0, 1), kind: NodeKind::Smooth }).unwrap();
    assert_eq!(
        once.edit_nodes(&NodeEdit::SetNodeKind { node: node(0, 1), kind: NodeKind::Smooth }),
        Err(NodeEditError::NoChange)
    );
    assert_eq!(
        c.edit_nodes(&NodeEdit::SetNodeKind { node: node(0, 9), kind: NodeKind::Smooth }),
        Err(NodeEditError::NoSuchNode(node(0, 9)))
    );
}

#[test]
fn a_full_reversal_has_no_tangent_and_stays_unchanged() {
    // Out and back along the same line: the two tangents cancel, there is no smooth direction.
    let spike = PathBuilder::new().move_to(p(0.0, 0.0)).line_to(p(10.0, 0.0)).line_to(p(0.0, 0.0)).build().unwrap();
    assert_eq!(
        spike.edit_nodes(&NodeEdit::SetNodeKind { node: node(0, 1), kind: NodeKind::Smooth }),
        Err(NodeEditError::NoChange)
    );
}

#[test]
fn smoothing_the_start_node_of_a_closed_subpath_uses_both_sides() {
    let out = square(true).edit_nodes(&NodeEdit::SetNodeKind { node: node(0, 0), kind: NodeKind::Smooth }).unwrap();
    let sp = &out.subpaths()[0];
    // Tangent at (0,0) = (0,-1)+(1,0) hence (1,-1)/√2, i.e. the incoming side arrives from above-left.
    let (c1, _, _) = cubic(&sp.segments()[0]);
    let h = 10.0 / 3.0 / 2f64.sqrt();
    assert!(near(c1, p(h, -h)), "{c1:?}");
    // The closing side became a cubic whose last handle sits at the start node's mirror.
    let last = sp.segments().last().unwrap();
    let (_, c2, to) = cubic(last);
    assert!(near(to, p(0.0, 0.0)) && near(c2, p(-h, h)), "{c2:?} {to:?}");
    assert!(sp.is_closed());
}

#[test]
fn line_to_curve_keeps_the_points_and_puts_the_handles_at_the_thirds() {
    let out =
        corner().edit_nodes(&NodeEdit::SetSegmentKind { subpath: 0, segment: 0, kind: SegmentKind::Curve }).unwrap();
    let segs = out.subpaths()[0].segments();
    let (c1, c2, to) = cubic(&segs[0]);
    assert!(near(c1, p(10.0 / 3.0, 0.0)) && near(c2, p(20.0 / 3.0, 0.0)) && to == p(10.0, 0.0), "{segs:?}");
    assert_eq!(segs[1], corner().subpaths()[0].segments()[1], "the other span is bit-identical");
    for t in [0.1, 0.5, 0.9] {
        assert!(near(segs[0].point_at(p(0.0, 0.0), t), p(10.0 * t, 0.0)), "a straight cubic traces the line");
    }
}

#[test]
fn curve_to_line_keeps_the_end_points_only() {
    let out = wave().edit_nodes(&NodeEdit::SetSegmentKind { subpath: 0, segment: 0, kind: SegmentKind::Line }).unwrap();
    assert_eq!(out.subpaths()[0].segments()[0], Segment::Line { to: p(10.0, 0.0) });
    assert_eq!(out.subpaths()[0].segments()[1], wave().subpaths()[0].segments()[1]);
}

#[test]
fn an_arc_can_become_a_line_but_not_a_curve_and_the_closing_line_is_addressable() {
    let circle = Path::circle(p(0.0, 0.0), 5.0).unwrap();
    let line =
        circle.edit_nodes(&NodeEdit::SetSegmentKind { subpath: 0, segment: 0, kind: SegmentKind::Line }).unwrap();
    assert!(matches!(line.subpaths()[0].segments()[0], Segment::Line { .. }));
    assert_eq!(
        circle.edit_nodes(&NodeEdit::SetSegmentKind { subpath: 0, segment: 0, kind: SegmentKind::Curve }),
        Err(NodeEditError::NoChange),
        "arcs keep their exact form; convert them to cubics explicitly"
    );
    // The implicit closing line of an open-looking triangle (segment index == len) becomes a cubic.
    let tri = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(5.0, 8.0)], true).unwrap();
    let n = tri.subpaths()[0].segments().len();
    let curved =
        tri.edit_nodes(&NodeEdit::SetSegmentKind { subpath: 0, segment: n, kind: SegmentKind::Curve }).unwrap();
    let last = curved.subpaths()[0].segments().last().unwrap();
    assert!(matches!(last, Segment::Cubic { to, .. } if near(*to, p(0.0, 0.0))));
    assert!(near(Point::new(curved.signed_area(), 0.0), Point::new(tri.signed_area(), 0.0)), "still the same triangle");
    assert_eq!(
        tri.edit_nodes(&NodeEdit::SetSegmentKind { subpath: 0, segment: n + 1, kind: SegmentKind::Line }),
        Err(NodeEditError::NoSuchSegment { subpath: 0, segment: n + 1 })
    );
}

#[test]
fn start_point_rotates_a_closed_subpath_without_changing_the_outline() {
    let sq = square(true);
    let out = sq.edit_nodes(&NodeEdit::SetStartPoint { node: node(0, 2) }).unwrap();
    let sp = &out.subpaths()[0];
    assert_eq!(sp.start(), p(10.0, 10.0));
    assert!(sp.is_closed());
    assert_eq!(out.signed_area(), sq.signed_area());
    assert_eq!(out.bounds(), sq.bounds());
    // The last side (back to the new start) is the implicit closing line.
    assert_eq!(
        sp.segments(),
        &[Segment::Line { to: p(0.0, 10.0) }, Segment::Line { to: p(0.0, 0.0) }, Segment::Line { to: p(10.0, 0.0) }]
    );
}

#[test]
fn start_point_errors_are_typed() {
    let sq = square(true);
    assert_eq!(sq.edit_nodes(&NodeEdit::SetStartPoint { node: node(0, 0) }), Err(NodeEditError::NoChange));
    assert_eq!(
        sq.edit_nodes(&NodeEdit::SetStartPoint { node: node(0, 9) }),
        Err(NodeEditError::NoSuchNode(node(0, 9)))
    );
    assert_eq!(
        corner().edit_nodes(&NodeEdit::SetStartPoint { node: node(0, 1) }),
        Err(NodeEditError::SubpathOpen(0)),
        "an open path starts at its first node by definition; reverse it to start at the other end"
    );
}

#[test]
fn set_winding_reverses_only_when_needed_and_keeps_the_start_node() {
    let cw = square(false);
    assert_eq!(cw.subpaths()[0].winding(), Some(Winding::Clockwise));
    let out = cw.edit_nodes(&NodeEdit::SetWinding { subpath: 0, winding: Winding::CounterClockwise }).unwrap();
    let sp = &out.subpaths()[0];
    assert_eq!(sp.winding(), Some(Winding::CounterClockwise));
    assert_eq!(sp.start(), p(0.0, 0.0), "the start node is kept");
    assert!((out.signed_area() + cw.signed_area()).abs() < 1e-12, "area flips sign, magnitude unchanged");
    assert_eq!(
        out.edit_nodes(&NodeEdit::SetWinding { subpath: 0, winding: Winding::CounterClockwise }),
        Err(NodeEditError::NoChange)
    );
    assert_eq!(
        corner().edit_nodes(&NodeEdit::SetWinding { subpath: 0, winding: Winding::Clockwise }),
        Err(NodeEditError::SubpathOpen(0))
    );
    assert_eq!(
        cw.edit_nodes(&NodeEdit::SetWinding { subpath: 0, winding: Winding::Degenerate }),
        Err(NodeEditError::InvalidParameter)
    );
    // A zero-area closed subpath has no direction to set.
    let flat = Path::polyline(&[p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0)], true).unwrap();
    assert_eq!(
        flat.edit_nodes(&NodeEdit::SetWinding { subpath: 0, winding: Winding::Clockwise }),
        Err(NodeEditError::NoChange)
    );
}

#[test]
fn set_winding_handles_curves_and_arcs() {
    let circle = Path::circle(p(0.0, 0.0), 5.0).unwrap();
    let cw = circle.edit_nodes(&NodeEdit::SetWinding { subpath: 0, winding: Winding::Clockwise }).unwrap();
    assert_eq!(cw.subpaths()[0].winding(), Some(Winding::Clockwise));
    assert!((cw.signed_area() + circle.signed_area()).abs() < 1e-9);
    assert!(near(cw.subpaths()[0].start(), circle.subpaths()[0].start()));
}
