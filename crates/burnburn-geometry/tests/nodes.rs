//! Public contract: node and handle editing (E02). A local edit must leave every other span
//! bit-identical; oracles are the untouched input segments and hand-derived De Casteljau points.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use burnburn_geometry::{
    EndpointRef, HandleMode, HandleRef, HandleSide, NodeEdit, NodeEditError, NodeRef, Path, PathBuilder, Point, Segment,
};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

/// Three cubic spans: (0,0) → (10,0) → (20,0) → (30,0), each bulging up.
fn three_spans() -> Path {
    PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .cubic_to(p(2.0, 5.0), p(8.0, 5.0), p(10.0, 0.0))
        .cubic_to(p(12.0, 5.0), p(18.0, 5.0), p(20.0, 0.0))
        .cubic_to(p(22.0, 5.0), p(28.0, 5.0), p(30.0, 0.0))
        .build()
        .unwrap()
}

fn node(subpath: usize, node: usize) -> NodeRef {
    NodeRef { subpath, node }
}

#[test]
fn editing_one_bezier_span_preserves_other_spans() {
    let before = three_spans();
    // Move the outgoing handle of node 1 (= c1 of span 1) without touching the node itself.
    let edit = NodeEdit::MoveHandle {
        handle: HandleRef { node: node(0, 1), side: HandleSide::Out },
        to: p(13.0, 9.0),
        mode: HandleMode::Corner,
    };
    let after = before.edit_nodes(&edit).unwrap();
    let (b, a) = (before.subpaths()[0].segments(), after.subpaths()[0].segments());
    assert_eq!(a.len(), 3);
    assert_eq!(a[0], b[0], "span 0 must be bit-identical");
    assert_eq!(a[2], b[2], "span 2 must be bit-identical");
    assert_eq!(a[1], Segment::Cubic { c1: p(13.0, 9.0), c2: p(18.0, 5.0), to: p(20.0, 0.0) });
    assert_eq!(after.subpaths()[0].start(), before.subpaths()[0].start());
}

#[test]
fn symmetric_handle_mode_mirrors_the_opposite_handle_only() {
    let before = three_spans();
    let edit = NodeEdit::MoveHandle {
        handle: HandleRef { node: node(0, 1), side: HandleSide::Out },
        to: p(13.0, 4.0),
        mode: HandleMode::Symmetric,
    };
    let after = before.edit_nodes(&edit).unwrap();
    let a = after.subpaths()[0].segments();
    // The node is (10,0); out handle (13,4) mirrors to in handle (7,-4) = c2 of span 0.
    assert_eq!(a[0], Segment::Cubic { c1: p(2.0, 5.0), c2: p(7.0, -4.0), to: p(10.0, 0.0) });
    assert_eq!(a[2], before.subpaths()[0].segments()[2]);

    // Smooth keeps the opposite handle's length (in handle (8,5) from node (10,0): √(2²+5²)) but
    // makes it collinear.
    let smooth = NodeEdit::MoveHandle {
        handle: HandleRef { node: node(0, 1), side: HandleSide::Out },
        to: p(16.0, 0.0),
        mode: HandleMode::Smooth,
    };
    let s = before.edit_nodes(&smooth).unwrap();
    let Segment::Cubic { c2, .. } = s.subpaths()[0].segments()[0] else { panic!("cubic") };
    assert!((c2.x - (10.0 - 29f64.sqrt())).abs() < 1e-12 && c2.y.abs() < 1e-12, "{c2:?}");
}

#[test]
fn moving_a_node_carries_its_handles_and_leaves_far_spans() {
    let before = three_spans();
    let after = before.edit_nodes(&NodeEdit::MoveNode { node: node(0, 1), to: p(10.0, 3.0) }).unwrap();
    let a = after.subpaths()[0].segments();
    assert_eq!(a[0], Segment::Cubic { c1: p(2.0, 5.0), c2: p(8.0, 8.0), to: p(10.0, 3.0) });
    assert_eq!(a[1], Segment::Cubic { c1: p(12.0, 8.0), c2: p(18.0, 5.0), to: p(20.0, 0.0) });
    assert_eq!(a[2], before.subpaths()[0].segments()[2]);
}

#[test]
fn inserting_a_node_splits_by_de_casteljau_without_changing_the_curve() {
    let before = three_spans();
    let after = before.edit_nodes(&NodeEdit::InsertNode { subpath: 0, segment: 1, t: 0.5 }).unwrap();
    let a = after.subpaths()[0].segments();
    assert_eq!(a.len(), 4);
    // Hand-derived: P0=(10,0) P1=(12,5) P2=(18,5) P3=(20,0) at t=1/2:
    // midpoints (11,2.5) (15,5) (19,2.5) → (13,3.75) (17,3.75) → (15,3.75).
    assert_eq!(a[1], Segment::Cubic { c1: p(11.0, 2.5), c2: p(13.0, 3.75), to: p(15.0, 3.75) });
    assert_eq!(a[2], Segment::Cubic { c1: p(17.0, 3.75), c2: p(19.0, 2.5), to: p(20.0, 0.0) });
    assert_eq!((a[0], a[3]), (before.subpaths()[0].segments()[0], before.subpaths()[0].segments()[2]));
    // A line splits into two collinear lines; an arc into two arcs with the same total sweep.
    let line = Path::line(p(0.0, 0.0), p(10.0, 0.0)).unwrap();
    let l = line.edit_nodes(&NodeEdit::InsertNode { subpath: 0, segment: 0, t: 0.25 }).unwrap();
    assert_eq!(l.subpaths()[0].segments(), &[Segment::Line { to: p(2.5, 0.0) }, Segment::Line { to: p(10.0, 0.0) }]);
}

#[test]
fn inserting_outside_the_open_unit_interval_is_rejected() {
    let before = three_spans();
    for t in [0.0, 1.0, -0.1, f64::NAN] {
        let r = before.edit_nodes(&NodeEdit::InsertNode { subpath: 0, segment: 1, t });
        assert_eq!(r, Err(NodeEditError::InvalidParameter), "t = {t}");
    }
    let r = before.edit_nodes(&NodeEdit::InsertNode { subpath: 0, segment: 3, t: 0.5 });
    assert_eq!(r, Err(NodeEditError::NoSuchSegment { subpath: 0, segment: 3 }));
}

#[test]
fn deleting_an_interior_node_merges_its_spans_and_keeps_outer_handles() {
    let before = three_spans();
    let after = before.edit_nodes(&NodeEdit::DeleteNode { node: node(0, 1) }).unwrap();
    let a = after.subpaths()[0].segments();
    assert_eq!(a.len(), 2);
    assert_eq!(a[0], Segment::Cubic { c1: p(2.0, 5.0), c2: p(18.0, 5.0), to: p(20.0, 0.0) });
    assert_eq!(a[1], before.subpaths()[0].segments()[2]);
    // Deleting the open start drops the first span.
    let s = before.edit_nodes(&NodeEdit::DeleteNode { node: node(0, 0) }).unwrap();
    assert_eq!(s.subpaths()[0].start(), p(10.0, 0.0));
    assert_eq!(s.subpaths()[0].segments(), &before.subpaths()[0].segments()[1..]);
    // A single segment cannot lose a node.
    let line = Path::line(p(0.0, 0.0), p(1.0, 0.0)).unwrap();
    assert_eq!(line.edit_nodes(&NodeEdit::DeleteNode { node: node(0, 1) }), Err(NodeEditError::TooFewNodes));
}

#[test]
fn close_break_and_reverse_keep_the_drawn_segments() {
    let open = PathBuilder::new().move_to(p(0.0, 0.0)).line_to(p(10.0, 0.0)).line_to(p(10.0, 10.0)).build().unwrap();
    let closed = open.edit_nodes(&NodeEdit::CloseSubpath { subpath: 0 }).unwrap();
    assert!(closed.subpaths()[0].is_closed());
    assert!((closed.signed_area() - 50.0).abs() < 1e-12);

    // Breaking the closed triangle at node 1 gives an open path starting there with the
    // closing line made explicit: (10,0) → (10,10) → (0,0) → (10,0).
    let broken = closed.edit_nodes(&NodeEdit::BreakAt { node: node(0, 1) }).unwrap();
    let sp = &broken.subpaths()[0];
    assert!(!sp.is_closed());
    assert_eq!(sp.start(), p(10.0, 0.0));
    assert_eq!(
        sp.segments(),
        &[Segment::Line { to: p(10.0, 10.0) }, Segment::Line { to: p(0.0, 0.0) }, Segment::Line { to: p(10.0, 0.0) }]
    );

    // Breaking an open path at an interior node splits it into two subpaths.
    let split = three_spans().edit_nodes(&NodeEdit::BreakAt { node: node(0, 2) }).unwrap();
    assert_eq!(split.subpaths().len(), 2);
    assert_eq!(split.subpaths()[0].end(), p(20.0, 0.0));
    assert_eq!(split.subpaths()[1].start(), p(20.0, 0.0));
    assert_eq!(split.subpaths()[1].segments(), &three_spans().subpaths()[0].segments()[2..]);

    let rev = three_spans().edit_nodes(&NodeEdit::ReverseSubpath { subpath: 0 }).unwrap();
    let r = &rev.subpaths()[0];
    assert_eq!((r.start(), r.end()), (p(30.0, 0.0), p(0.0, 0.0)));
    assert_eq!(r.segments()[0], Segment::Cubic { c1: p(28.0, 5.0), c2: p(22.0, 5.0), to: p(20.0, 0.0) });
}

#[test]
fn joining_endpoints_merges_two_open_subpaths_or_closes_one() {
    let two = PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .line_to(p(10.0, 0.0))
        .move_to(p(10.0, 0.0))
        .line_to(p(10.0, 10.0))
        .build()
        .unwrap();
    let joined = two.edit_nodes(&NodeEdit::JoinEndpoints { a: EndpointRef::End(0), b: EndpointRef::Start(1) }).unwrap();
    assert_eq!(joined.subpaths().len(), 1);
    assert_eq!(
        joined.subpaths()[0].segments(),
        &[Segment::Line { to: p(10.0, 0.0) }, Segment::Line { to: p(10.0, 10.0) }]
    );
    // A gap is bridged by a line (nothing is moved silently).
    let gap = PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .line_to(p(10.0, 0.0))
        .move_to(p(12.0, 0.0))
        .line_to(p(20.0, 0.0))
        .build()
        .unwrap();
    let g = gap.edit_nodes(&NodeEdit::JoinEndpoints { a: EndpointRef::End(0), b: EndpointRef::Start(1) }).unwrap();
    assert_eq!(g.subpaths()[0].segments().len(), 3);
    // The two ends of one subpath close it.
    let c = gap.edit_nodes(&NodeEdit::JoinEndpoints { a: EndpointRef::End(1), b: EndpointRef::Start(1) }).unwrap();
    assert!(c.subpaths()[1].is_closed());
    // Joining at a closed subpath is an error.
    let closed = c.edit_nodes(&NodeEdit::JoinEndpoints { a: EndpointRef::End(0), b: EndpointRef::Start(1) });
    assert_eq!(closed, Err(NodeEditError::SubpathClosed(1)));
}

#[test]
fn arc_endpoints_need_an_explicit_conversion_before_node_moves() {
    let circle = Path::circle(p(0.0, 0.0), 10.0).unwrap();
    let r = circle.edit_nodes(&NodeEdit::MoveNode { node: node(0, 0), to: p(11.0, 0.0) });
    assert!(matches!(r, Err(NodeEditError::ArcSegment { .. })), "{r:?}");
    let cubic = circle.edit_nodes(&NodeEdit::ConvertToCubic { subpath: 0 }).unwrap();
    assert!(cubic.subpaths()[0].segments().iter().all(|s| matches!(s, Segment::Cubic { .. } | Segment::Line { .. })));
    // Four quarter cubics approximate the circle: area within 0.03 % of π r².
    let area = cubic.signed_area();
    assert!((area / (std::f64::consts::PI * 100.0) - 1.0).abs() < 3e-4, "{area}");
    assert!(cubic.edit_nodes(&NodeEdit::MoveNode { node: node(0, 0), to: p(11.0, 0.0) }).is_ok());
}
