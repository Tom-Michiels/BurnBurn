//! W1-L2a multi-node and whole-path operations (`PathOp`): one operation, consistent indices.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use burnburn_geometry::{
    EndpointRef, NodeEdit, NodeEditError, NodeRef, NodeSetOp, Path, PathBuilder, PathOp, PathOpError, Point,
    ProductionTolerance, RoundError, Segment, SegmentKind, SegmentRef, Winding,
};
use burnburn_types::{CancellationSource, CancellationToken, Length};
use proptest::prelude::*;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn node(subpath: usize, node: usize) -> NodeRef {
    NodeRef { subpath, node }
}

fn apply(path: &Path, op: &PathOp) -> Result<Path, PathOpError> {
    path.apply_op(op, &CancellationToken::never())
}

fn vertices(path: &Path) -> Vec<Vec<Point>> {
    path.subpaths()
        .iter()
        .map(|sp| std::iter::once(sp.start()).chain(sp.segments().iter().map(Segment::end)).collect())
        .collect()
}

fn zigzag() -> Path {
    // Nodes 0..=5.
    Path::polyline(&[p(0.0, 0.0), p(1.0, 1.0), p(2.0, 0.0), p(3.0, 1.0), p(4.0, 0.0), p(5.0, 1.0)], false).unwrap()
}

fn square() -> Path {
    Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)], true).unwrap()
}

#[test]
fn deleting_several_nodes_at_once_uses_the_original_numbering() {
    let op = PathOp::NodeSet { op: NodeSetOp::Delete, nodes: vec![node(0, 1), node(0, 3), node(0, 4)] };
    let out = apply(&zigzag(), &op).unwrap();
    assert_eq!(vertices(&out), vec![vec![p(0.0, 0.0), p(2.0, 0.0), p(5.0, 1.0)]]);
}

#[test]
fn deleting_end_nodes_shortens_an_open_path_and_the_start_of_a_closed_one_moves_on() {
    let op = PathOp::NodeSet { op: NodeSetOp::Delete, nodes: vec![node(0, 0), node(0, 5)] };
    assert_eq!(
        vertices(&apply(&zigzag(), &op).unwrap()),
        vec![vec![p(1.0, 1.0), p(2.0, 0.0), p(3.0, 1.0), p(4.0, 0.0)]]
    );
    let op = PathOp::NodeSet { op: NodeSetOp::Delete, nodes: vec![node(0, 0)] };
    let out = apply(&square(), &op).unwrap();
    let sp = &out.subpaths()[0];
    assert!(sp.is_closed());
    assert_eq!(vertices(&out), vec![vec![p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)]]);
    assert!((out.signed_area() - 50.0).abs() < 1e-9, "the triangle left over");
}

#[test]
fn deleting_too_many_nodes_is_refused_and_bad_nodes_are_named() {
    let all_but_one = PathOp::NodeSet { op: NodeSetOp::Delete, nodes: (1..4).map(|i| node(0, i)).collect() };
    assert_eq!(apply(&square(), &all_but_one), Err(PathOpError::Node(NodeEditError::TooFewNodes)));
    let bad = PathOp::NodeSet { op: NodeSetOp::Delete, nodes: vec![node(0, 9)] };
    assert_eq!(apply(&square(), &bad), Err(PathOpError::Node(NodeEditError::NoSuchNode(node(0, 9)))));
    let none = PathOp::NodeSet { op: NodeSetOp::Delete, nodes: vec![] };
    assert_eq!(apply(&square(), &none), Err(PathOpError::InvalidParameter));
}

#[test]
fn deleting_between_cubics_keeps_the_outer_handles() {
    let wave = PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .cubic_to(p(2.0, 5.0), p(8.0, 5.0), p(10.0, 0.0))
        .cubic_to(p(12.0, -5.0), p(18.0, -5.0), p(20.0, 0.0))
        .build()
        .unwrap();
    let op = PathOp::NodeSet { op: NodeSetOp::Delete, nodes: vec![node(0, 1)] };
    let out = apply(&wave, &op).unwrap();
    assert_eq!(
        out.subpaths()[0].segments(),
        &[Segment::Cubic { c1: p(2.0, 5.0), c2: p(18.0, -5.0), to: p(20.0, 0.0) }]
    );
    // One node delete through the op equals the single-node edit.
    assert_eq!(out, wave.edit_nodes(&NodeEdit::DeleteNode { node: node(0, 1) }).unwrap());
}

#[test]
fn breaking_at_several_nodes_splits_an_open_path_and_ignores_its_ends() {
    let op = PathOp::NodeSet { op: NodeSetOp::Break, nodes: vec![node(0, 0), node(0, 2), node(0, 4), node(0, 5)] };
    let out = apply(&zigzag(), &op).unwrap();
    assert_eq!(
        vertices(&out),
        vec![
            vec![p(0.0, 0.0), p(1.0, 1.0), p(2.0, 0.0)],
            vec![p(2.0, 0.0), p(3.0, 1.0), p(4.0, 0.0)],
            vec![p(4.0, 0.0), p(5.0, 1.0)],
        ]
    );
    let only_ends = PathOp::NodeSet { op: NodeSetOp::Break, nodes: vec![node(0, 0), node(0, 5)] };
    assert_eq!(apply(&zigzag(), &only_ends), Err(PathOpError::NoChange));
}

#[test]
fn breaking_a_closed_path_at_two_nodes_gives_two_open_pieces_that_cover_it_exactly() {
    let op = PathOp::NodeSet { op: NodeSetOp::Break, nodes: vec![node(0, 0), node(0, 2)] };
    let out = apply(&square(), &op).unwrap();
    assert_eq!(
        vertices(&out),
        vec![vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)], vec![p(10.0, 10.0), p(0.0, 10.0), p(0.0, 0.0)]]
    );
    assert!(out.subpaths().iter().all(|s| !s.is_closed()));
    // One break opens it at that node (same as the single-node edit).
    let one = PathOp::NodeSet { op: NodeSetOp::Break, nodes: vec![node(0, 3)] };
    assert_eq!(apply(&square(), &one).unwrap(), square().edit_nodes(&NodeEdit::BreakAt { node: node(0, 3) }).unwrap());
}

#[test]
fn breaking_one_subpath_leaves_the_others_in_place() {
    let both = Path::from_subpaths(vec![zigzag().subpaths()[0].clone(), square().subpaths()[0].clone()]);
    let op = PathOp::NodeSet { op: NodeSetOp::Break, nodes: vec![node(0, 2)] };
    let out = apply(&both, &op).unwrap();
    assert_eq!(out.subpaths().len(), 3);
    assert_eq!(out.subpaths()[2], square().subpaths()[0]);
}

#[test]
fn node_kind_sets_apply_to_every_node_and_skip_those_that_cannot_change() {
    let op = PathOp::NodeSet {
        op: NodeSetOp::Smooth,
        nodes: vec![node(0, 0), node(0, 1), node(0, 2), node(0, 3), node(0, 5)],
    };
    let out = apply(&zigzag(), &op).unwrap();
    let segs = out.subpaths()[0].segments();
    let cubic: Vec<bool> = segs.iter().map(|s| matches!(s, Segment::Cubic { .. })).collect();
    assert_eq!(cubic, vec![true, true, true, true, false], "nodes 1..=3 smoothed the four spans around them");
    assert_eq!(segs.last().unwrap().end(), p(5.0, 1.0));
    let corner = PathOp::NodeSet { op: NodeSetOp::Corner, nodes: vec![node(0, 1)] };
    assert_eq!(apply(&zigzag(), &corner), Err(PathOpError::NoChange), "already a corner");
}

#[test]
fn segment_sets_convert_line_and_curve() {
    let op = PathOp::SegmentSet {
        kind: SegmentKind::Curve,
        segments: vec![SegmentRef { subpath: 0, segment: 0 }, SegmentRef { subpath: 0, segment: 2 }],
    };
    let out = apply(&zigzag(), &op).unwrap();
    let kinds: Vec<bool> = out.subpaths()[0].segments().iter().map(|s| matches!(s, Segment::Cubic { .. })).collect();
    assert_eq!(kinds, vec![true, false, true, false, false]);
    let back = PathOp::SegmentSet { kind: SegmentKind::Line, segments: vec![SegmentRef { subpath: 0, segment: 0 }] };
    assert!(matches!(apply(&out, &back).unwrap().subpaths()[0].segments()[0], Segment::Line { .. }));
}

#[test]
fn round_corners_and_optimize_run_through_the_same_entry_point() {
    let r = PathOp::RoundCorners { radius: 2.0, nodes: None };
    assert_eq!(apply(&square(), &r).unwrap(), square().round_corners(2.0, None).unwrap().path);
    assert_eq!(
        apply(&square(), &PathOp::RoundCorners { radius: 0.0, nodes: None }),
        Err(PathOpError::Round(RoundError::InvalidRadius))
    );
    let noisy = Path::polyline(&[p(0.0, 0.0), p(5.0, 0.001), p(10.0, 0.0)], false).unwrap();
    let tol = ProductionTolerance::new(Length::from_mm(0.01).unwrap()).unwrap();
    assert_eq!(apply(&noisy, &PathOp::Optimize { tolerance: tol }).unwrap().subpaths()[0].segments().len(), 1);
    assert_eq!(apply(&square(), &PathOp::Optimize { tolerance: tol }), Err(PathOpError::NoChange));
    let source = CancellationSource::new();
    source.cancel();
    assert_eq!(
        noisy.apply_op(&PathOp::Optimize { tolerance: tol }, &source.token()),
        Err(PathOpError::Optimize(burnburn_geometry::OptimizeError::Cancelled))
    );
}

#[test]
fn winding_and_reverse_keep_start_nodes_and_open_ends_swap() {
    let cw = apply(&square(), &PathOp::SetWinding { winding: Winding::Clockwise }).unwrap();
    assert_eq!(cw.subpaths()[0].winding(), Some(Winding::Clockwise));
    assert_eq!(cw.subpaths()[0].start(), p(0.0, 0.0));
    assert_eq!(apply(&cw, &PathOp::SetWinding { winding: Winding::Clockwise }), Err(PathOpError::NoChange));
    let rev = apply(&square(), &PathOp::Reverse { subpaths: None }).unwrap();
    assert_eq!(rev.subpaths()[0].start(), p(0.0, 0.0), "a closed contour reverses around its start node");
    assert_eq!(rev.subpaths()[0].winding(), Some(Winding::Clockwise));
    let open = apply(&zigzag(), &PathOp::Reverse { subpaths: Some(vec![0]) }).unwrap();
    assert_eq!((open.subpaths()[0].start(), open.subpaths()[0].end()), (p(5.0, 1.0), p(0.0, 0.0)));
    assert_eq!(
        apply(&zigzag(), &PathOp::Reverse { subpaths: Some(vec![4]) }),
        Err(PathOpError::Node(NodeEditError::NoSuchSubpath(4)))
    );
}

#[test]
fn edits_apply_in_order_and_fail_as_a_whole() {
    let two = Path::from_subpaths(vec![
        Path::line(p(0.0, 0.0), p(10.0, 0.0)).unwrap().subpaths()[0].clone(),
        Path::line(p(10.0, 0.0), p(10.0, 10.0)).unwrap().subpaths()[0].clone(),
    ]);
    let join = PathOp::Edits(vec![NodeEdit::JoinEndpoints { a: EndpointRef::End(0), b: EndpointRef::Start(1) }]);
    assert_eq!(apply(&two, &join).unwrap().subpaths().len(), 1);
    let bad = PathOp::Edits(vec![
        NodeEdit::JoinEndpoints { a: EndpointRef::End(0), b: EndpointRef::Start(1) },
        NodeEdit::BreakAt { node: node(5, 0) },
    ]);
    assert_eq!(apply(&two, &bad), Err(PathOpError::Node(NodeEditError::NoSuchSubpath(5))));
    assert_eq!(apply(&two, &PathOp::Edits(vec![])), Err(PathOpError::InvalidParameter));
}

proptest! {
    /// Deleting a node set from a polygon leaves exactly the other vertices, in order.
    #[test]
    fn deleting_a_node_set_keeps_the_remaining_vertices_in_order(
        n in 4usize..14,
        mask in proptest::collection::vec(any::<bool>(), 14),
    ) {
        let pts: Vec<Point> = (0..n).map(|i| p(i as f64, ((i * i) % 7) as f64)).collect();
        let poly = Path::polyline(&pts, true).unwrap();
        let del: Vec<usize> = (0..n).filter(|&i| mask[i]).collect();
        prop_assume!(!del.is_empty());
        let nodes: Vec<NodeRef> = del.iter().map(|&i| node(0, i)).collect();
        let op = PathOp::NodeSet { op: NodeSetOp::Delete, nodes };
        match apply(&poly, &op) {
            Ok(out) => {
                let kept: Vec<Point> = (0..n).filter(|i| !del.contains(i)).map(|i| pts[i]).collect();
                prop_assert_eq!(&vertices(&out)[0], &kept);
                prop_assert!(out.subpaths()[0].is_closed());
            }
            Err(e) => {
                prop_assert_eq!(e, PathOpError::Node(NodeEditError::TooFewNodes));
                prop_assert!(n - del.len() < 2);
            }
        }
    }

    /// Breaking at a node set partitions the contour: every edge appears in exactly one piece.
    #[test]
    fn breaking_a_polygon_covers_each_edge_exactly_once(
        n in 3usize..12,
        mask in proptest::collection::vec(any::<bool>(), 12),
    ) {
        let pts: Vec<Point> = (0..n).map(|i| p(i as f64 * 2.0, ((i * 5) % 3) as f64 * 3.0)).collect();
        let poly = Path::polyline(&pts, true).unwrap();
        let br: Vec<usize> = (0..n).filter(|&i| mask[i]).collect();
        prop_assume!(!br.is_empty());
        let op = PathOp::NodeSet { op: NodeSetOp::Break, nodes: br.iter().map(|&i| node(0, i)).collect() };
        let out = apply(&poly, &op).unwrap();
        prop_assert_eq!(out.subpaths().len(), br.len());
        let edges: usize = out.subpaths().iter().map(|s| s.segments().len()).sum();
        prop_assert_eq!(edges, n);
        prop_assert!(out.subpaths().iter().all(|s| !s.is_closed()));
    }
}
