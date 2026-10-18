//! Public contract: trim/extend for lines (E05), geometry diagnostics and gap repair (E09),
//! align/distribute (E04). Oracles are hand-placed coordinates.

#![allow(clippy::unwrap_used, clippy::unreachable)] // test helpers and let-else in tests

use burnburn_geometry::{
    AlignEdge, AlignTo, Bounds, DiagnosticTolerance, Direction, EndpointRef, ExtendError, IssueKind, Path, PathBuilder,
    Point, Rect, Segment, Spacing, TrimError, Vector, align_offsets, close_gaps, diagnose, distribute_offsets,
};
use burnburn_types::Length;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn dtol(mm: f64) -> DiagnosticTolerance {
    DiagnosticTolerance::new(Length::from_mm(mm).unwrap()).unwrap()
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(p(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap())
}

#[test]
fn trim_removes_the_picked_piece_between_cutters() {
    let line = Path::line(p(0.0, 0.0), p(10.0, 0.0)).unwrap();
    let c1 = Path::line(p(3.0, -1.0), p(3.0, 1.0)).unwrap();
    let c2 = Path::line(p(7.0, -1.0), p(7.0, 1.0)).unwrap();
    // Middle piece: two lines remain.
    let mid = line.trim_line(p(5.0, 0.1), &[&c1, &c2]).unwrap();
    assert_eq!(mid.subpaths().len(), 2);
    assert_eq!((mid.subpaths()[0].start(), mid.subpaths()[0].end()), (p(0.0, 0.0), p(3.0, 0.0)));
    assert_eq!((mid.subpaths()[1].start(), mid.subpaths()[1].end()), (p(7.0, 0.0), p(10.0, 0.0)));
    // End piece: one line remains.
    let end = line.trim_line(p(9.0, 0.0), &[&c1, &c2]).unwrap();
    assert_eq!(end.subpaths().len(), 1);
    assert_eq!(end.subpaths()[0].end(), p(7.0, 0.0));
    // No cutter crosses: typed error, nothing changed.
    let far = Path::line(p(20.0, -1.0), p(20.0, 1.0)).unwrap();
    assert_eq!(line.trim_line(p(5.0, 0.0), &[&far]), Err(TrimError::NoIntersection));
    // Picking a curve is not a line trim.
    let arc = Path::circle(p(0.0, 0.0), 5.0).unwrap();
    assert_eq!(arc.trim_line(p(5.0, 0.0), &[&c1]), Err(TrimError::NotALine));
}

#[test]
fn extend_moves_a_line_end_to_the_nearest_boundary_ahead() {
    let line = Path::line(p(0.0, 0.0), p(4.0, 0.0)).unwrap();
    let wall = Path::line(p(10.0, -5.0), p(10.0, 5.0)).unwrap();
    let nearer = Path::circle(p(7.0, 0.0), 1.0).unwrap();
    let e = line.extend_line(EndpointRef::End(0), &[&wall, &nearer]).unwrap();
    assert_eq!(e.subpaths()[0].segments(), &[Segment::Line { to: p(6.0, 0.0) }]);
    // Extending the start goes backwards; nothing behind → typed error.
    assert_eq!(line.extend_line(EndpointRef::Start(0), &[&wall]), Err(ExtendError::NoBoundaryAhead));
}

#[test]
fn diagnostics_locate_open_contours_self_intersections_duplicates_and_zero_length() {
    let open = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.05, 10.0)], false).unwrap();
    let bow = Path::polyline(&[p(20.0, 0.0), p(30.0, 10.0), p(30.0, 0.0), p(20.0, 10.0)], true).unwrap();
    let dup_a = Path::line(p(40.0, 0.0), p(50.0, 0.0)).unwrap();
    let dup_b = Path::line(p(50.0, 0.0), p(40.0, 0.0)).unwrap(); // reversed duplicate
    let tiny = PathBuilder::new()
        .move_to(p(60.0, 0.0))
        .line_to(p(70.0, 0.0))
        .line_to(p(70.0, 0.00001))
        .line_to(p(70.0, 10.0))
        .build()
        .unwrap();
    let objects = [(1u32, &open), (2, &bow), (3, &dup_a), (4, &dup_b), (5, &tiny)];
    let issues = diagnose(&objects, &dtol(0.001));

    let gap = issues.iter().find(|i| i.object == 1 && matches!(i.kind, IssueKind::OpenContour { .. })).unwrap();
    let IssueKind::OpenContour { gap: g } = gap.kind else { unreachable!() };
    assert!((g - 10.000_124_999_218_788).abs() < 1e-9, "physical gap {g}");
    assert_eq!(gap.location, p(0.05, 10.0));

    let x = issues.iter().find(|i| i.kind == IssueKind::SelfIntersection).unwrap();
    assert_eq!((x.object, x.location), (2, p(25.0, 5.0)));

    let d = issues.iter().find(|i| matches!(i.kind, IssueKind::DuplicateSegment { .. })).unwrap();
    assert_eq!(d.object, 3);
    assert!(matches!(d.kind, IssueKind::DuplicateSegment { other: 4, overlap } if (overlap - 10.0).abs() < 1e-12));

    let z = issues.iter().find(|i| matches!(i.kind, IssueKind::ZeroLength { .. })).unwrap();
    assert_eq!(z.object, 5);
    assert_eq!(z.at.map(|s| s.segment), Some(1));

    // A clean closed rectangle has no issue.
    assert!(diagnose(&[(9u32, &rect(0.0, 0.0, 5.0, 5.0))], &dtol(0.001)).is_empty());
}

#[test]
fn closing_a_seam_keeps_an_intended_slot() {
    // E09: a 0.05 mm seam closes with a 0.1 mm repair tolerance; a 0.15 mm slot stays open.
    let seam = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0), p(0.0, 0.05)], false).unwrap();
    let slot =
        Path::polyline(&[p(20.0, 0.0), p(30.0, 0.0), p(30.0, 10.0), p(20.0, 10.0), p(20.0, 0.15)], false).unwrap();
    let (fixed_seam, closed) = close_gaps(&seam, &dtol(0.1));
    assert_eq!(closed, vec![0]);
    let sp = &fixed_seam.subpaths()[0];
    assert!(sp.is_closed() && sp.end() == sp.start(), "seam snapped exactly onto the start");
    assert!((fixed_seam.signed_area() - 100.0).abs() < 1e-12);
    let (same_slot, none) = close_gaps(&slot, &dtol(0.1));
    assert!(none.is_empty());
    assert_eq!(same_slot, slot);
    // Two pieces of one contour (as DXF lines arrive) are chained when their ends meet within tolerance.
    let pieces = PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .line_to(p(10.0, 0.0))
        .line_to(p(10.0, 10.0))
        .move_to(p(10.0, 10.02))
        .line_to(p(0.0, 10.0))
        .line_to(p(0.0, 0.01))
        .build()
        .unwrap();
    let (chained, _) = close_gaps(&pieces, &dtol(0.1));
    assert_eq!(chained.subpaths().len(), 1);
    assert!(chained.subpaths()[0].is_closed());
}

#[test]
fn align_and_distribute_translate_without_resizing() {
    let b = [
        Bounds::from_corners(p(0.0, 0.0), p(10.0, 5.0)),
        Bounds::from_corners(p(20.0, 3.0), p(24.0, 9.0)),
        Bounds::from_corners(p(50.0, -2.0), p(60.0, 2.0)),
    ];
    let left = align_offsets(&b, AlignEdge::Left, AlignTo::Selection).unwrap();
    assert_eq!(left, vec![Vector::new(0.0, 0.0), Vector::new(-20.0, 0.0), Vector::new(-50.0, 0.0)]);
    let top_to_second = align_offsets(&b, AlignEdge::Top, AlignTo::Object(1)).unwrap();
    assert_eq!(top_to_second, vec![Vector::new(0.0, 4.0), Vector::ZERO, Vector::new(0.0, 7.0)]);
    let cx = align_offsets(&b, AlignEdge::CenterX, AlignTo::Selection).unwrap();
    assert_eq!(cx[0], Vector::new(25.0, 0.0)); // selection centre x = 30, own centre 5

    // Centres 5, 22, 55 → 5, 30, 55 (ends fixed).
    let centers = distribute_offsets(&b, Direction::Horizontal, Spacing::Centers).unwrap();
    assert_eq!(centers, vec![Vector::ZERO, Vector::new(8.0, 0.0), Vector::ZERO]);
    // Gaps: widths 10, 4, 10 in the span 0..60 → equal gaps (60 − 24)/2 = 18, middle box at x = 28.
    let gaps = distribute_offsets(&b, Direction::Horizontal, Spacing::Gaps).unwrap();
    assert_eq!(gaps, vec![Vector::ZERO, Vector::new(8.0, 0.0), Vector::ZERO]);
    assert!(distribute_offsets(&b[..2], Direction::Horizontal, Spacing::Gaps).is_err());
    assert!(align_offsets(&b, AlignEdge::Left, AlignTo::Object(7)).is_err());
}
